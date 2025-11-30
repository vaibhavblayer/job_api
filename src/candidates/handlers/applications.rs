// src/candidates/handlers/applications.rs

use crate::auth::AuthedUser;
use crate::candidates::models::*;
use crate::candidates::validators::ApplicationValidator;
use crate::common::{generate_application_id, generate_history_id, ApiError, AppState, Validator};
use axum::extract::{Extension, Json, Path};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Serialize)]
pub struct ApplicationAnalytics {
    pub total_applications: i64,
    pub applications_by_status: std::collections::HashMap<String, i64>,
    pub applications_by_job: Vec<JobApplicationStats>,
    pub conversion_rates: ConversionRates,
    pub recent_applications: Vec<Application>,
}

#[derive(Debug, Serialize)]
pub struct JobApplicationStats {
    pub job_id: String,
    pub job_title: String,
    pub application_count: i64,
    pub latest_application: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConversionRates {
    pub submitted_to_reviewed: f64,
    pub reviewed_to_interviewed: f64,
    pub interviewed_to_offered: f64,
    pub offered_to_hired: f64,
}

#[derive(Debug, Serialize)]
pub struct BulkOperationResult {
    pub success_count: usize,
    pub failed_count: usize,
    pub errors: Vec<String>,
}

/// POST /api/applications - Create a new job application
pub async fn create_application(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateApplicationRequest>,
) -> Result<Json<Application>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        job_id = %request.job_id,
        "Creating new job application"
    );
    
    // In dev mode, ensure the dev user exists in the database
    if state.dev_mode.is_enabled() {
        let user_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id = ?")
            .bind(&authed.id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        if user_exists == 0 {
            let dev_user = state.dev_mode.create_dev_user();
            // First, delete any existing user with the same email (from previous dev sessions)
            sqlx::query("DELETE FROM users WHERE email = ?")
                .bind(&dev_user.email)
                .execute(&state.db)
                .await
                .map_err(ApiError::DatabaseError)?;
            
            // Now insert the dev user
            sqlx::query(
                "INSERT INTO users (id, email, name, provider, provider_id, created_at) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&dev_user.id)
            .bind(&dev_user.email)
            .bind(&dev_user.name)
            .bind(&dev_user.provider)
            .bind(&dev_user.provider_id)
            .bind(&dev_user.created_at)
            .execute(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;
            
            info!(user_id = %dev_user.id, "Dev user created in database");
        }
    }

    let validator = ApplicationValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            user_id = %authed.id,
            job_id = %request.job_id,
            errors = ?validation_result.errors,
            "Application creation validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    let job_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE id = ?")
        .bind(&request.job_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    if job_exists == 0 {
        return Err(ApiError::BadRequest("Job not found".to_string()));
    }

    let existing_application = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM applications WHERE user_id = ? AND job_id = ?",
    )
    .bind(&authed.id)
    .bind(&request.job_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    if existing_application > 0 {
        return Err(ApiError::BadRequest(
            "You have already applied for this job".to_string(),
        ));
    }

    if let Some(resume_id) = &request.resume_id {
        let resume_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM resumes WHERE id = ? AND user_id = ?",
        )
        .bind(resume_id)
        .bind(&authed.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        if resume_exists == 0 {
            return Err(ApiError::BadRequest("Resume not found".to_string()));
        }
    }

    let application_id = generate_application_id();

    sqlx::query(
        r#"
        INSERT INTO applications (id, user_id, job_id, resume_id, status, cover_letter, applied_at, updated_at)
        VALUES (?, ?, ?, ?, 'submitted', ?, datetime('now'), datetime('now'))
        "#
    )
    .bind(&application_id)
    .bind(&authed.id)
    .bind(&request.job_id)
    .bind(request.resume_id.as_deref())
    .bind(request.cover_letter.as_deref())
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let history_id = generate_history_id();
    sqlx::query(
        r#"
        INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
        VALUES (?, ?, 'submitted', ?, 'Application submitted', datetime('now'))
        "#
    )
    .bind(&history_id)
    .bind(&application_id)
    .bind(&authed.id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(
        user_id = %authed.id,
        application_id = %application_id,
        job_id = %request.job_id,
        "Application created successfully"
    );

    Ok(Json(application))
}

/// GET /api/applications - Get all applications for the authenticated user
pub async fn get_user_applications(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<EnhancedApplicationWithDetails>>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(user_id = %authed.id, "Fetching user applications");

    let query = r#"
        SELECT 
            a.id, a.user_id, a.job_id, a.resume_id, a.status, a.cover_letter, a.applied_at, a.updated_at,
            j.title as job_title, j.company as job_company, j.location as job_location,
            j.salary_min as job_salary_min, j.salary_max as job_salary_max,
            j.job_image_url, j.company_logo_url,
            r.filename as resume_filename
        FROM applications a
        LEFT JOIN jobs j ON a.job_id = j.id
        LEFT JOIN resumes r ON a.resume_id = r.id
        WHERE a.user_id = ?
        ORDER BY a.applied_at DESC
    "#;

    let rows = sqlx::query(query)
        .bind(&authed.id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let mut result = Vec::new();

    for row in rows {
        let application_id: String = row.try_get("id").unwrap_or_default();

        let status_history = sqlx::query_as::<_, ApplicationStatusHistory>(
            "SELECT * FROM application_status_history WHERE application_id = ? ORDER BY changed_at DESC"
        )
        .bind(&application_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        result.push(EnhancedApplicationWithDetails {
            id: application_id,
            user_id: row.try_get("user_id").unwrap_or_default(),
            job_id: row.try_get("job_id").unwrap_or_default(),
            resume_id: row.try_get("resume_id").ok(),
            video_id: row.try_get("video_id").ok(),
            status: row.try_get("status").unwrap_or_default(),
            cover_letter: row.try_get("cover_letter").ok(),
            applied_at: row.try_get("applied_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            job_title: row.try_get("job_title").unwrap_or_default(),
            job_company: row.try_get("job_company").ok(),
            job_location: row.try_get("job_location").ok(),
            job_salary_min: row.try_get("job_salary_min").ok(),
            job_salary_max: row.try_get("job_salary_max").ok(),
            job_image_url: row.try_get("job_image_url").ok(),
            company_logo_url: row.try_get("company_logo_url").ok(),
            resume_filename: row.try_get("resume_filename").ok(),
            resume_label: None, // Column doesn't exist in database
            status_history,
        });
    }

    Ok(Json(result))
}

/// GET /api/applications/:id - Get application details
pub async fn get_application_details(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(application_id): Path<String>,
) -> Result<Json<ApplicationWithDetails>, ApiError> {
    let state = state_lock.read().await.clone();

    let application = if authed.is_admin {
        sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
            .bind(&application_id)
            .fetch_optional(&state.db)
            .await
    } else {
        sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ? AND user_id = ?")
            .bind(&application_id)
            .bind(&authed.id)
            .fetch_optional(&state.db)
            .await
    };

    let application = application
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Application not found".to_string()))?;

    let job_title = sqlx::query_scalar::<_, Option<String>>("SELECT title FROM jobs WHERE id = ?")
        .bind(&application.job_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default()
        .flatten();

    let (candidate_name, candidate_email) = if authed.is_admin {
        let user_details = sqlx::query_as::<_, (Option<String>, String)>(
            "SELECT name, email FROM users WHERE id = ?",
        )
        .bind(&application.user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();

        match user_details {
            Some((name, email)) => (name, Some(email)),
            None => (None, None),
        }
    } else {
        (None, None)
    };

    let status_history = sqlx::query_as::<_, ApplicationStatusHistory>(
        "SELECT * FROM application_status_history WHERE application_id = ? ORDER BY changed_at DESC"
    )
    .bind(&application_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(Json(ApplicationWithDetails {
        application,
        job_title,
        candidate_name,
        candidate_email,
        status_history,
    }))
}

/// PATCH /api/applications/:id/status - Update application status (admin only)
pub async fn update_application_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(application_id): Path<String>,
    Json(request): Json<UpdateApplicationStatusRequest>,
) -> Result<Json<Application>, ApiError> {
    let state = state_lock.read().await.clone();

    let validator = ApplicationValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        return Err(ApiError::from(validation_result));
    }

    let existing_application =
        sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
            .bind(&application_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::BadRequest("Application not found".to_string()))?;

    // Allow users to withdraw their own applications, admins can change any status
    if !authed.is_admin {
        // Check if user owns this application
        if existing_application.user_id != authed.id {
            return Err(ApiError::Forbidden("You can only update your own applications".to_string()));
        }
        // Users can only withdraw their own applications
        if request.status != "withdrawn" {
            return Err(ApiError::Forbidden("You can only withdraw your own applications. Other status changes require admin privileges".to_string()));
        }
    }

    // Validate status transition
    if let Err(msg) = super::email_templates::validate_status_transition(&existing_application.status, &request.status) {
        return Err(ApiError::BadRequest(msg));
    }

    // Log the status change
    info!(
        application_id = %application_id,
        old_status = %existing_application.status,
        new_status = %request.status,
        changed_by = %authed.id,
        "Updating application status"
    );

    let current_stage = status_to_stage(&request.status);

    sqlx::query(
        r#"
        UPDATE applications 
        SET status = ?, current_stage = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&request.status)
    .bind(current_stage)
    .bind(&application_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let history_id = generate_history_id();
    sqlx::query(
        r#"
        INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
        VALUES (?, ?, ?, ?, ?, datetime('now'))
        "#
    )
    .bind(&history_id)
    .bind(&application_id)
    .bind(&request.status)
    .bind(&authed.id)
    .bind(request.notes.as_deref())
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(
        application_id = %application_id,
        new_status = %request.status,
        "Application status updated successfully"
    );

    Ok(Json(application))
}

/// GET /api/admin/jobs/:id/applications - Get all applications for a specific job (admin only)
pub async fn get_job_applications(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<JobApplicationDetails>>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let job_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    if job_exists == 0 {
        return Err(ApiError::BadRequest("Job not found".to_string()));
    }

    let query = r#"
        SELECT 
            a.id as application_id, a.user_id as candidate_id,
            u.name as candidate_name, u.email as candidate_email,
            a.resume_id, r.filename as resume_filename,
            a.status, a.applied_at, a.cover_letter
        FROM applications a
        INNER JOIN users u ON a.user_id = u.id
        LEFT JOIN resumes r ON a.resume_id = r.id
        WHERE a.job_id = ?
        ORDER BY a.applied_at DESC
    "#;

    let rows = sqlx::query(query)
        .bind(&job_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let mut result = Vec::new();

    for row in rows {
        result.push(JobApplicationDetails {
            application_id: row.try_get("application_id").unwrap_or_default(),
            candidate_id: row.try_get("candidate_id").unwrap_or_default(),
            candidate_name: row.try_get("candidate_name").unwrap_or_default(),
            candidate_email: row.try_get("candidate_email").unwrap_or_default(),
            resume_id: row.try_get("resume_id").ok(),
            resume_filename: row.try_get("resume_filename").ok(),
            resume_label: None, // Column doesn't exist in database
            status: row.try_get("status").unwrap_or_default(),
            applied_at: row.try_get("applied_at").ok(),
            cover_letter: row.try_get("cover_letter").ok(),
        });
    }

    Ok(Json(result))
}

/// GET /api/admin/applications/analytics - Get application analytics (admin only)
pub async fn get_application_analytics(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<ApplicationAnalytics>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let total_applications = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM applications")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let status_counts = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) as count FROM applications GROUP BY status",
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let mut applications_by_status = std::collections::HashMap::new();
    for (status, count) in status_counts {
        applications_by_status.insert(status, count);
    }

    let job_stats = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
        r#"
        SELECT 
            a.job_id, j.title, COUNT(*) as application_count, MAX(a.applied_at) as latest_application
        FROM applications a
        LEFT JOIN jobs j ON a.job_id = j.id
        GROUP BY a.job_id, j.title
        ORDER BY application_count DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let applications_by_job: Vec<JobApplicationStats> = job_stats
        .into_iter()
        .map(
            |(job_id, job_title, application_count, latest_application)| JobApplicationStats {
                job_id,
                job_title,
                application_count,
                latest_application,
            },
        )
        .collect();

    let status_totals = &applications_by_status;
    let submitted = *status_totals.get("submitted").unwrap_or(&0) as f64;
    let reviewed = *status_totals.get("reviewed").unwrap_or(&0) as f64;
    let interviewing = *status_totals.get("interviewing").unwrap_or(&0) as f64;
    let offered = *status_totals.get("offered").unwrap_or(&0) as f64;
    let hired = *status_totals.get("hired").unwrap_or(&0) as f64;

    let conversion_rates = ConversionRates {
        submitted_to_reviewed: if submitted > 0.0 {
            reviewed / submitted
        } else {
            0.0
        },
        reviewed_to_interviewed: if reviewed > 0.0 {
            interviewing / reviewed
        } else {
            0.0
        },
        interviewed_to_offered: if interviewing > 0.0 {
            offered / interviewing
        } else {
            0.0
        },
        offered_to_hired: if offered > 0.0 { hired / offered } else { 0.0 },
    };

    let recent_applications = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications ORDER BY applied_at DESC LIMIT 10",
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(Json(ApplicationAnalytics {
        total_applications,
        applications_by_status,
        applications_by_job,
        conversion_rates,
        recent_applications,
    }))
}

/// POST /api/admin/applications/bulk-update-status - Bulk update application status (admin only)
pub async fn bulk_update_application_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<BulkApplicationStatusUpdate>,
) -> Result<Json<BulkOperationResult>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for application_id in &request.application_ids {
        let existing_application =
            sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
                .bind(application_id)
                .fetch_optional(&state.db)
                .await;

        match existing_application {
            Ok(Some(_)) => {
                let current_stage = status_to_stage(&request.status);

                let update_result = sqlx::query(
                    r#"
                    UPDATE applications 
                    SET status = ?, current_stage = ?, updated_at = datetime('now')
                    WHERE id = ?
                    "#,
                )
                .bind(&request.status)
                .bind(current_stage)
                .bind(application_id)
                .execute(&state.db)
                .await;

                match update_result {
                    Ok(result) => {
                        if result.rows_affected() > 0 {
                            let history_id = generate_history_id();
                            let history_result = sqlx::query(
                                r#"
                                INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
                                VALUES (?, ?, ?, ?, ?, datetime('now'))
                                "#
                            )
                            .bind(&history_id)
                            .bind(application_id)
                            .bind(&request.status)
                            .bind(&authed.id)
                            .bind(request.notes.as_deref())
                            .execute(&state.db)
                            .await;

                            match history_result {
                                Ok(_) => success_count += 1,
                                Err(e) => {
                                    failed_count += 1;
                                    errors.push(format!(
                                        "Failed to create status history for application {}: {}",
                                        application_id, e
                                    ));
                                }
                            }
                        } else {
                            failed_count += 1;
                            errors.push(format!(
                                "Application {} not found or already has the same status",
                                application_id
                            ));
                        }
                    }
                    Err(e) => {
                        failed_count += 1;
                        errors.push(format!(
                            "Failed to update application {}: {}",
                            application_id, e
                        ));
                    }
                }
            }
            Ok(None) => {
                failed_count += 1;
                errors.push(format!("Application {} not found", application_id));
            }
            Err(e) => {
                failed_count += 1;
                errors.push(format!(
                    "Database error for application {}: {}",
                    application_id, e
                ));
            }
        }
    }

    Ok(Json(BulkOperationResult {
        success_count,
        failed_count,
        errors,
    }))
}


// ============================================================================
// Enhanced Application Management
// ============================================================================

use super::email_templates::{get_email_template, get_next_status, status_to_stage};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AdvanceStageRequest {
    pub send_email: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SendEmailRequest {
    pub custom_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkActionRequest {
    pub application_ids: Vec<String>,
    pub action: String, // "advance_stage", "send_email", "update_status"
    pub status: Option<String>,
    pub send_email: Option<bool>,
}

/// POST /api/admin/applications/:id/advance-stage - Move application to next stage
pub async fn advance_application_stage(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(application_id): Path<String>,
    Json(request): Json<AdvanceStageRequest>,
) -> Result<Json<Application>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    // Get current application
    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Application not found".to_string()))?;

    // Get next status
    let next_status = get_next_status(&application.status)
        .ok_or_else(|| ApiError::BadRequest("Application is already at final stage".to_string()))?;

    let current_stage = status_to_stage(next_status);

    // Update status
    sqlx::query(
        r#"
        UPDATE applications 
        SET status = ?, current_stage = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(next_status)
    .bind(current_stage)
    .bind(&application_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Add to history
    let history_id = generate_history_id();
    sqlx::query(
        r#"
        INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
        VALUES (?, ?, ?, ?, ?, datetime('now'))
        "#
    )
    .bind(&history_id)
    .bind(&application_id)
    .bind(next_status)
    .bind(&authed.id)
    .bind(format!("Advanced to {} stage", next_status))
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Send email if requested
    if request.send_email.unwrap_or(false) {
        if let Err(e) = send_status_email(&state, &application, next_status).await {
            warn!("Failed to send email: {}", e);
        }
    }

    let updated_application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(
        application_id = %application_id,
        old_status = %application.status,
        new_status = %next_status,
        "Advanced application stage"
    );

    Ok(Json(updated_application))
}

/// POST /api/admin/applications/:id/send-email - Send status email to candidate
pub async fn send_application_email(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(application_id): Path<String>,
    Json(_request): Json<SendEmailRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Application not found".to_string()))?;

    send_status_email(&state, &application, &application.status).await?;

    info!(
        application_id = %application_id,
        status = %application.status,
        "Sent status email to candidate"
    );

    Ok(Json(serde_json::json!({
        "message": "Email sent successfully"
    })))
}

/// POST /api/admin/applications/bulk-action - Perform bulk action on applications
pub async fn bulk_application_action(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<BulkActionRequest>,
) -> Result<Json<BulkOperationResult>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for app_id in &request.application_ids {
        match request.action.as_str() {
            "advance_stage" => {
                match advance_single_application(&state, app_id, &authed.id, request.send_email.unwrap_or(false)).await {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        failed_count += 1;
                        errors.push(format!("{}: {}", app_id, e));
                    }
                }
            }
            "send_email" => {
                match send_email_for_application(&state, app_id).await {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        failed_count += 1;
                        errors.push(format!("{}: {}", app_id, e));
                    }
                }
            }
            "update_status" => {
                if let Some(status) = &request.status {
                    match update_single_application_status(&state, app_id, status, &authed.id, request.send_email.unwrap_or(false)).await {
                        Ok(_) => success_count += 1,
                        Err(e) => {
                            failed_count += 1;
                            errors.push(format!("{}: {}", app_id, e));
                        }
                    }
                }
            }
            _ => {
                failed_count += 1;
                errors.push(format!("{}: Unknown action", app_id));
            }
        }
    }

    info!(
        action = %request.action,
        success_count = %success_count,
        failed_count = %failed_count,
        "Bulk application action completed"
    );

    Ok(Json(BulkOperationResult {
        success_count,
        failed_count,
        errors,
    }))
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn send_status_email(
    state: &AppState,
    application: &Application,
    status: &str,
) -> Result<(), ApiError> {
    // Get candidate info
    let user: (String, String) = sqlx::query_as(
        "SELECT name, email FROM users WHERE id = ?"
    )
    .bind(&application.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("User not found".to_string()))?;

    // Get job info
    let job: (String, Option<String>) = sqlx::query_as(
        "SELECT title, company FROM jobs WHERE id = ?"
    )
    .bind(&application.job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Job not found".to_string()))?;

    let candidate_name = user.0;
    let candidate_email = user.1;
    let job_title = job.0;
    let company_name = job.1.unwrap_or_else(|| "Our Company".to_string());

    let template = get_email_template(status, &candidate_name, &job_title, &company_name);

    state
        .aws_service
        .send_email(vec![candidate_email], &template.subject, &template.body, None)
        .await
        .map_err(|e| ApiError::ProcessingError(format!("Failed to send email: {}", e)))?;

    Ok(())
}

async fn advance_single_application(
    state: &AppState,
    application_id: &str,
    admin_id: &str,
    send_email: bool,
) -> Result<(), String> {
    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(application_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Application not found".to_string())?;

    let next_status = get_next_status(&application.status)
        .ok_or_else(|| "Already at final stage".to_string())?;

    let current_stage = status_to_stage(next_status);

    sqlx::query("UPDATE applications SET status = ?, current_stage = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(next_status)
        .bind(current_stage)
        .bind(application_id)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let history_id = generate_history_id();
    sqlx::query(
        "INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at) VALUES (?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(&history_id)
    .bind(application_id)
    .bind(next_status)
    .bind(admin_id)
    .bind(format!("Bulk advanced to {}", next_status))
    .execute(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    if send_email {
        let updated_app = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
            .bind(application_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| e.to_string())?;
        
        let _ = send_status_email(state, &updated_app, next_status).await;
    }

    Ok(())
}

async fn send_email_for_application(
    state: &AppState,
    application_id: &str,
) -> Result<(), String> {
    let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(application_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Application not found".to_string())?;

    send_status_email(state, &application, &application.status)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn update_single_application_status(
    state: &AppState,
    application_id: &str,
    status: &str,
    admin_id: &str,
    send_email: bool,
) -> Result<(), String> {
    let current_stage = status_to_stage(status);

    sqlx::query("UPDATE applications SET status = ?, current_stage = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(status)
        .bind(current_stage)
        .bind(application_id)
        .execute(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let history_id = generate_history_id();
    sqlx::query(
        "INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at) VALUES (?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(&history_id)
    .bind(application_id)
    .bind(status)
    .bind(admin_id)
    .bind(format!("Bulk updated to {}", status))
    .execute(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    if send_email {
        let application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
            .bind(application_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| e.to_string())?;
        
        let _ = send_status_email(state, &application, status).await;
    }

    Ok(())
}

// ============================================================================
// Job-Centric Candidate Management (Frontend Compatibility)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ApproveCandidateRequest {
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectCandidateRequest {
    pub reason: String,
}

/// Response type that matches frontend CandidateApplication expectations
#[derive(Debug, Serialize)]
pub struct CandidateApplicationResponse {
    pub id: String,
    pub job_id: String,
    pub candidate_id: String,
    pub resume_id: Option<String>,
    pub current_stage: String,
    pub status: String,
    pub cover_letter: Option<String>,
    pub applied_at: Option<String>,
    pub updated_at: Option<String>,
    pub stage_history: Vec<StageHistoryEntry>,
}

#[derive(Debug, Serialize)]
pub struct StageHistoryEntry {
    pub id: String,
    pub stage: String,
    pub changed_by: String,
    pub changed_by_name: Option<String>,
    pub notes: Option<String>,
    pub changed_at: Option<String>,
}

/// POST /api/admin/jobs/:job_id/candidates/:candidate_id/approve - Approve candidate for next stage
pub async fn approve_candidate_for_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path((job_id, candidate_id)): Path<(String, String)>,
    Json(request): Json<ApproveCandidateRequest>,
) -> Result<Json<CandidateApplicationResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    // Find the application for this job and candidate
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE job_id = ? AND user_id = ?"
    )
    .bind(&job_id)
    .bind(&candidate_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Application not found for this job and candidate".to_string()))?;

    // Get next status
    let next_status = get_next_status(&application.status)
        .ok_or_else(|| ApiError::BadRequest("Application is already at final stage".to_string()))?;

    let current_stage = status_to_stage(next_status);

    // Update status
    sqlx::query(
        r#"
        UPDATE applications 
        SET status = ?, current_stage = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(next_status)
    .bind(current_stage)
    .bind(&application.id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Add to history
    let history_id = generate_history_id();
    let notes = request.notes.unwrap_or_else(|| format!("Approved and advanced to {} stage", next_status));
    sqlx::query(
        r#"
        INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
        VALUES (?, ?, ?, ?, ?, datetime('now'))
        "#
    )
    .bind(&history_id)
    .bind(&application.id)
    .bind(next_status)
    .bind(&authed.id)
    .bind(&notes)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Fetch updated application
    let updated_application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Fetch stage history
    let history = sqlx::query_as::<_, ApplicationStatusHistory>(
        "SELECT * FROM application_status_history WHERE application_id = ? ORDER BY changed_at DESC"
    )
    .bind(&application.id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get admin name for history entries
    let stage_history: Vec<StageHistoryEntry> = history.into_iter().map(|h| {
        StageHistoryEntry {
            id: h.id,
            stage: status_to_stage(&h.status).to_string(),
            changed_by: h.changed_by.clone(),
            changed_by_name: None, // Could fetch from users table if needed
            notes: h.notes,
            changed_at: h.changed_at,
        }
    }).collect();

    info!(
        job_id = %job_id,
        candidate_id = %candidate_id,
        application_id = %application.id,
        old_status = %application.status,
        new_status = %next_status,
        "Approved candidate for next stage"
    );

    // Note: Email is NOT auto-sent here - stage manager has manual email sending via AI generation

    Ok(Json(CandidateApplicationResponse {
        id: updated_application.id,
        job_id: updated_application.job_id,
        candidate_id: updated_application.user_id,
        resume_id: updated_application.resume_id,
        current_stage: current_stage.to_string(),
        status: updated_application.status,
        cover_letter: updated_application.cover_letter,
        applied_at: updated_application.applied_at,
        updated_at: updated_application.updated_at,
        stage_history,
    }))
}

/// POST /api/admin/jobs/:job_id/candidates/:candidate_id/reject - Reject candidate
pub async fn reject_candidate_for_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path((job_id, candidate_id)): Path<(String, String)>,
    Json(request): Json<RejectCandidateRequest>,
) -> Result<Json<CandidateApplicationResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    // Find the application for this job and candidate
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE job_id = ? AND user_id = ?"
    )
    .bind(&job_id)
    .bind(&candidate_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Application not found for this job and candidate".to_string()))?;

    // Update status to rejected
    sqlx::query(
        r#"
        UPDATE applications 
        SET status = 'rejected', current_stage = 'Rejected', updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&application.id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Add to history
    let history_id = generate_history_id();
    sqlx::query(
        r#"
        INSERT INTO application_status_history (id, application_id, status, changed_by, notes, changed_at)
        VALUES (?, ?, 'rejected', ?, ?, datetime('now'))
        "#
    )
    .bind(&history_id)
    .bind(&application.id)
    .bind(&authed.id)
    .bind(&request.reason)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Fetch updated application
    let updated_application = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(&application.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Fetch stage history
    let history = sqlx::query_as::<_, ApplicationStatusHistory>(
        "SELECT * FROM application_status_history WHERE application_id = ? ORDER BY changed_at DESC"
    )
    .bind(&application.id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let stage_history: Vec<StageHistoryEntry> = history.into_iter().map(|h| {
        StageHistoryEntry {
            id: h.id,
            stage: status_to_stage(&h.status).to_string(),
            changed_by: h.changed_by.clone(),
            changed_by_name: None,
            notes: h.notes,
            changed_at: h.changed_at,
        }
    }).collect();

    info!(
        job_id = %job_id,
        candidate_id = %candidate_id,
        application_id = %application.id,
        reason = %request.reason,
        "Rejected candidate"
    );

    // Note: Email is NOT auto-sent here - stage manager has manual email sending via AI generation

    Ok(Json(CandidateApplicationResponse {
        id: updated_application.id,
        job_id: updated_application.job_id,
        candidate_id: updated_application.user_id,
        resume_id: updated_application.resume_id,
        current_stage: "Rejected".to_string(),
        status: updated_application.status,
        cover_letter: updated_application.cover_letter,
        applied_at: updated_application.applied_at,
        updated_at: updated_application.updated_at,
        stage_history,
    }))
}

// ============================================================================
// Send Email to Candidate
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SendCandidateEmailRequest {
    pub subject: String,
    pub content: String,
    pub cc: Option<Vec<String>>,
}

/// POST /api/admin/jobs/:job_id/candidates/:candidate_id/email - Send email to candidate
pub async fn send_candidate_email_for_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path((job_id, candidate_id)): Path<(String, String)>,
    Json(request): Json<SendCandidateEmailRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    // Find the application for this job and candidate
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE job_id = ? AND user_id = ?"
    )
    .bind(&job_id)
    .bind(&candidate_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Application not found for this job and candidate".to_string()))?;

    // Get candidate email
    let candidate_email: String = sqlx::query_scalar(
        "SELECT email FROM users WHERE id = ?"
    )
    .bind(&candidate_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Build recipient list
    let mut recipients = vec![candidate_email.clone()];
    if let Some(cc_list) = &request.cc {
        recipients.extend(cc_list.clone());
    }

    // Send email via AWS SES
    state
        .aws_service
        .send_email(recipients.clone(), &request.subject, &request.content, None)
        .await
        .map_err(|e| ApiError::ProcessingError(format!("Failed to send email: {}", e)))?;

    // Log the email in email_history table if it exists
    let email_id = crate::common::generate_history_id();
    let cc_json = request.cc.as_ref().map(|cc| serde_json::to_string(cc).unwrap_or_default());
    
    let _ = sqlx::query(
        r#"
        INSERT INTO email_history (id, application_id, candidate_id, job_id, subject, content, cc, sent_by, sent_at, email_type)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), 'manual')
        "#
    )
    .bind(&email_id)
    .bind(&application.id)
    .bind(&candidate_id)
    .bind(&job_id)
    .bind(&request.subject)
    .bind(&request.content)
    .bind(&cc_json)
    .bind(&authed.id)
    .execute(&state.db)
    .await;

    info!(
        job_id = %job_id,
        candidate_id = %candidate_id,
        candidate_email = %candidate_email,
        subject = %request.subject,
        recipient_count = recipients.len(),
        "Email sent to candidate"
    );

    Ok(Json(serde_json::json!({
        "message": "Email sent successfully",
        "recipient": candidate_email,
        "cc_count": request.cc.as_ref().map(|c| c.len()).unwrap_or(0)
    })))
}
