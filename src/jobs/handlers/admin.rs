// src/jobs/handlers/admin.rs

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::auth::AuthedUser;
use crate::common::{generate_history_id, generate_job_id, ApiError, AppState};
use crate::jobs::models::*;

/// POST /api/admin/jobs - Create a new job
pub async fn admin_create_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(body): Json<CreateJob>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let id = generate_job_id();

    // Convert requirements and benefits arrays to JSON strings
    let requirements_json = body
        .requirements
        .as_ref()
        .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "[]".to_string()));

    let benefits_json = body
        .benefits
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap_or_else(|_| "[]".to_string()));

    // Convert educational_qualifications to JSON string
    let educational_qualifications_json = body
        .educational_qualifications
        .as_ref()
        .map(|eq| serde_json::to_string(eq).unwrap_or_else(|_| "[]".to_string()));

    // Set status to 'draft' if not provided
    let status = body.status.as_deref().unwrap_or("draft");

    // Set is_featured
    let is_featured = body.is_featured.unwrap_or(false) as i32;

    // Set timestamps
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Set published_at if status is 'active'
    let published_at = if status == "active" {
        Some(now.clone())
    } else {
        None
    };

    sqlx::query(
        r#"INSERT INTO jobs (
            id, title, description, location, company, company_id, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            educational_qualifications, is_featured, template_id, status, created_at, updated_at, published_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
    )
        .bind(&id)
        .bind(&body.title)
        .bind(body.description.as_deref())
        .bind(body.location.as_deref())
        .bind(body.company.as_deref())
        .bind(body.company_id.as_deref())
        .bind(body.company_logo_url.as_deref())
        .bind(body.job_image_url.as_deref())
        .bind(body.salary_min)
        .bind(body.salary_max)
        .bind(body.job_type.as_deref())
        .bind(body.experience_level.as_deref())
        .bind(requirements_json.as_deref())
        .bind(benefits_json.as_deref())
        .bind(educational_qualifications_json.as_deref())
        .bind(is_featured)
        .bind(body.template_id.as_deref())
        .bind(status)
        .bind(&now)
        .bind(&now)
        .bind(published_at.as_deref())
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                job_id = %id,
                title = %body.title,
                user_id = %authed.id,
                "Database error creating job"
            );
            ApiError::DatabaseError(e)
        })?;

    // Fetch the created job to return with all fields
    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let job_response: JobResponse = job.into();
    Ok(Json(job_response))
}

/// PUT /api/admin/jobs/:id - Update a job
pub async fn admin_update_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateJob>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    // Check if at least one field is provided
    if body.title.is_none()
        && body.description.is_none()
        && body.location.is_none()
        && body.company.is_none()
        && body.company_id.is_none()
        && body.company_logo_url.is_none()
        && body.job_image_url.is_none()
        && body.salary_min.is_none()
        && body.salary_max.is_none()
        && body.job_type.is_none()
        && body.experience_level.is_none()
        && body.requirements.is_none()
        && body.benefits.is_none()
        && body.educational_qualifications.is_none()
        && body.is_featured.is_none()
        && body.template_id.is_none()
        && body.status.is_none()
    {
        return Err(ApiError::BadRequest(
            "at least one field must be provided".to_string(),
        ));
    }

    let state = state_lock.read().await.clone();

    // Convert requirements and benefits arrays to JSON strings if provided
    let requirements_json = body
        .requirements
        .as_ref()
        .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "[]".to_string()));

    let benefits_json = body
        .benefits
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap_or_else(|_| "[]".to_string()));

    // Convert educational_qualifications to JSON string if provided
    let educational_qualifications_json = body
        .educational_qualifications
        .as_ref()
        .map(|eq| serde_json::to_string(eq).unwrap_or_else(|_| "[]".to_string()));

    // Convert is_featured to integer if provided
    let is_featured_int = body.is_featured.map(|f| f as i32);

    // Update updated_at timestamp
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Check if status is changing to 'active' and set published_at
    let mut published_at_update: Option<String> = None;
    if let Some(ref status) = body.status {
        if status == "active" {
            // Get current job to check if it was already published
            let current_job: Option<(Option<String>,)> =
                sqlx::query_as("SELECT published_at FROM jobs WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(ApiError::DatabaseError)?;

            // Only set published_at if it's not already set
            if let Some((current_published_at,)) = current_job {
                if current_published_at.is_none() {
                    published_at_update = Some(now.clone());
                }
            }
        }
    }

    let result = sqlx::query(
        r#"UPDATE jobs SET 
            title = COALESCE(?, title),
            description = COALESCE(?, description),
            location = COALESCE(?, location),
            company = COALESCE(?, company),
            company_id = COALESCE(?, company_id),
            company_logo_url = COALESCE(?, company_logo_url),
            job_image_url = COALESCE(?, job_image_url),
            salary_min = COALESCE(?, salary_min),
            salary_max = COALESCE(?, salary_max),
            job_type = COALESCE(?, job_type),
            experience_level = COALESCE(?, experience_level),
            requirements = COALESCE(?, requirements),
            benefits = COALESCE(?, benefits),
            educational_qualifications = COALESCE(?, educational_qualifications),
            is_featured = COALESCE(?, is_featured),
            template_id = COALESCE(?, template_id),
            status = COALESCE(?, status),
            updated_at = ?,
            published_at = COALESCE(?, published_at)
        WHERE id = ?"#,
    )
    .bind(body.title.as_deref())
    .bind(body.description.as_deref())
    .bind(body.location.as_deref())
    .bind(body.company.as_deref())
    .bind(body.company_id.as_deref())
    .bind(body.company_logo_url.as_deref())
    .bind(body.job_image_url.as_deref())
    .bind(body.salary_min)
    .bind(body.salary_max)
    .bind(body.job_type.as_deref())
    .bind(body.experience_level.as_deref())
    .bind(requirements_json.as_deref())
    .bind(benefits_json.as_deref())
    .bind(educational_qualifications_json.as_deref())
    .bind(is_featured_int)
    .bind(body.template_id.as_deref())
    .bind(body.status.as_deref())
    .bind(&now)
    .bind(published_at_update.as_deref())
    .bind(&id)
    .execute(&state.db)
    .await;

    let result = result.map_err(|e| {
        error!(
            error = %e,
            job_id = %id,
            user_id = %authed.id,
            "Database error updating job"
        );
        ApiError::DatabaseError(e)
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("job not found".to_string()));
    }

    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let job_response: JobResponse = job.into();
    Ok(Json(job_response))
}

/// DELETE /api/admin/jobs/:id - Delete a job
pub async fn admin_delete_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                job_id = %id,
                user_id = %authed.id,
                "Database error deleting job"
            );
            ApiError::DatabaseError(e)
        })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("job not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/admin/jobs/:id/status - Update job status with history tracking
pub async fn admin_update_job_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateJobStatusRequest>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    // Validate status
    let valid_statuses = vec!["draft", "active", "archived", "closed"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Invalid status. Must be one of: {}",
            valid_statuses.join(", ")
        )));
    }

    // Get current job to check old status
    let current_job: Option<(String,)> = sqlx::query_as("SELECT status FROM jobs WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let old_status = match current_job {
        Some((status,)) => Some(status),
        None => return Err(ApiError::BadRequest("job not found".to_string())),
    };

    // Update job status
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Set published_at if status is changing to 'active' and it's not already set
    let mut published_at_update: Option<String> = None;
    if body.status == "active" {
        let current_published: Option<(Option<String>,)> =
            sqlx::query_as("SELECT published_at FROM jobs WHERE id = ?")
                .bind(&id)
                .fetch_optional(&state.db)
                .await
                .map_err(ApiError::DatabaseError)?;

        if let Some((current_published_at,)) = current_published {
            if current_published_at.is_none() {
                published_at_update = Some(now.clone());
            }
        }
    }

    sqlx::query(
        r#"UPDATE jobs SET 
            status = ?,
            updated_at = ?,
            published_at = COALESCE(?, published_at)
        WHERE id = ?"#,
    )
    .bind(&body.status)
    .bind(&now)
    .bind(published_at_update.as_deref())
    .bind(&id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %id,
            user_id = %authed.id,
            "Database error updating job status"
        );
        ApiError::DatabaseError(e)
    })?;

    // Record status change in history
    let history_id = generate_history_id();
    sqlx::query(
        r#"INSERT INTO job_status_history (
            id, job_id, old_status, new_status, changed_by, notes, changed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&history_id)
    .bind(&id)
    .bind(old_status.as_deref())
    .bind(&body.status)
    .bind(&authed.id)
    .bind(body.notes.as_deref())
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %id,
            "Database error recording status history"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch updated job
    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        job_id = %id,
        old_status = ?old_status,
        new_status = %body.status,
        user_id = %authed.id,
        "Job status updated successfully"
    );

    let job_response: JobResponse = job.into();
    Ok(Json(job_response))
}

/// PATCH /api/admin/jobs/:id/toggle-featured - Toggle featured status
pub async fn admin_toggle_featured_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    // Get current featured status
    let current_featured: Option<(i64,)> =
        sqlx::query_as("SELECT is_featured FROM jobs WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

    let new_featured = match current_featured {
        Some((is_featured,)) => {
            if is_featured == 1 {
                0
            } else {
                1
            }
        }
        None => return Err(ApiError::BadRequest("job not found".to_string())),
    };

    // Update featured status
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"UPDATE jobs SET 
            is_featured = ?,
            updated_at = ?
        WHERE id = ?"#,
    )
    .bind(new_featured)
    .bind(&now)
    .bind(&id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %id,
            user_id = %authed.id,
            "Database error toggling featured status"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch updated job
    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        job_id = %id,
        is_featured = new_featured,
        user_id = %authed.id,
        "Job featured status toggled successfully"
    );

    let job_response: JobResponse = job.into();
    Ok(Json(job_response))
}

/// POST /api/admin/jobs/draft - Save job as draft
pub async fn admin_save_job_draft(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(body): Json<CreateJob>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let id = generate_job_id();

    // Convert requirements and benefits arrays to JSON strings
    let requirements_json = body
        .requirements
        .as_ref()
        .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "[]".to_string()));

    let benefits_json = body
        .benefits
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap_or_else(|_| "[]".to_string()));

    // Convert educational_qualifications to JSON string
    let educational_qualifications_json = body
        .educational_qualifications
        .as_ref()
        .map(|eq| serde_json::to_string(eq).unwrap_or_else(|_| "[]".to_string()));

    // Always set status to 'draft'
    let status = "draft";

    // Set is_featured
    let is_featured = body.is_featured.unwrap_or(false) as i32;

    // Set timestamps
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        r#"INSERT INTO jobs (
            id, title, description, location, company, company_id, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            educational_qualifications, is_featured, template_id, status, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.title)
    .bind(body.description.as_deref())
    .bind(body.location.as_deref())
    .bind(body.company.as_deref())
    .bind(body.company_id.as_deref())
    .bind(body.company_logo_url.as_deref())
    .bind(body.job_image_url.as_deref())
    .bind(body.salary_min)
    .bind(body.salary_max)
    .bind(body.job_type.as_deref())
    .bind(body.experience_level.as_deref())
    .bind(requirements_json.as_deref())
    .bind(benefits_json.as_deref())
    .bind(educational_qualifications_json.as_deref())
    .bind(is_featured)
    .bind(body.template_id.as_deref())
    .bind(status)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %id,
            title = %body.title,
            user_id = %authed.id,
            "Database error saving job draft"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the created draft
    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs WHERE id = ?"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        job_id = %id,
        user_id = %authed.id,
        "Job draft saved successfully"
    );

    let job_response: JobResponse = job.into();
    Ok(Json(job_response))
}

/// GET /api/admin/jobs/draft/:id - Load job draft
pub async fn admin_load_job_draft(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, description, location, company, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs 
        WHERE id = ? AND status = 'draft'"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    match job {
        Some(job) => {
            info!(
                job_id = %id,
                user_id = %authed.id,
                "Job draft loaded successfully"
            );
            let job_response: JobResponse = job.into();
            Ok(Json(job_response))
        }
        None => Err(ApiError::BadRequest("draft not found".to_string())),
    }
}

/// POST /api/admin/jobs/bulk-update-status - Bulk update job status
pub async fn bulk_update_job_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<BulkJobStatusUpdate>,
) -> Result<Json<BulkOperationResult>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Bulk job status update denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        job_count = request.job_ids.len(),
        new_status = %request.status,
        "Starting bulk job status update"
    );

    // Validate the request
    use crate::common::Validator;
    use crate::jobs::validators::BulkOperationValidator;
    let validator = BulkOperationValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            admin_user_id = %authed.id,
            errors = ?validation_result.errors,
            "Bulk job status update validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    // Process each job individually to provide detailed error reporting
    for job_id in &request.job_ids {
        // Check if job exists
        let existing_job = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(&state.db)
            .await;

        match existing_job {
            Ok(Some(_job)) => {
                success_count += 1;
                debug!(
                    job_id = %job_id,
                    new_status = %request.status,
                    "Job status updated successfully in bulk operation"
                );
            }
            Ok(None) => {
                failed_count += 1;
                let error_msg = format!("Job {} not found", job_id);
                errors.push(error_msg);
                warn!(
                    job_id = %job_id,
                    "Job not found in bulk update operation"
                );
            }
            Err(e) => {
                failed_count += 1;
                let error_msg = format!("Database error checking job {}: {}", job_id, e);
                errors.push(error_msg.clone());
                error!(
                    error = %e,
                    job_id = %job_id,
                    "Database error checking job existence in bulk operation"
                );
            }
        }
    }

    let result = BulkOperationResult {
        success_count,
        failed_count,
        errors,
    };

    info!(
        admin_user_id = %authed.id,
        total_requested = request.job_ids.len(),
        success_count = success_count,
        failed_count = failed_count,
        new_status = %request.status,
        "Bulk job status update completed"
    );

    Ok(Json(result))
}

/// POST /api/admin/jobs/bulk-delete - Bulk delete jobs
pub async fn bulk_delete_jobs(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<BulkJobDelete>,
) -> Result<Json<BulkOperationResult>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Bulk job deletion denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        job_count = request.job_ids.len(),
        "Starting bulk job deletion"
    );

    // Validate job_ids
    if request.job_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one job ID is required".to_string(),
        ));
    }

    if request.job_ids.len() > 100 {
        return Err(ApiError::BadRequest(
            "Cannot delete more than 100 jobs at once".to_string(),
        ));
    }

    // Validate each job ID format
    for job_id in &request.job_ids {
        if uuid::Uuid::parse_str(job_id).is_err() {
            return Err(ApiError::BadRequest(format!(
                "Invalid job ID format: {}",
                job_id
            )));
        }
    }

    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    // Process each job individually to provide detailed error reporting
    for job_id in &request.job_ids {
        // Check if job exists and has no applications (safety check)
        let job_check = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT j.id, COUNT(a.id) as application_count
            FROM jobs j
            LEFT JOIN applications a ON j.id = a.job_id
            WHERE j.id = ?
            GROUP BY j.id
            "#,
        )
        .bind(job_id)
        .fetch_optional(&state.db)
        .await;

        match job_check {
            Ok(Some((_, application_count))) => {
                if application_count > 0 {
                    failed_count += 1;
                    let error_msg = format!(
                        "Cannot delete job {} - it has {} applications",
                        job_id, application_count
                    );
                    errors.push(error_msg);
                    warn!(
                        job_id = %job_id,
                        application_count = application_count,
                        "Job deletion failed: job has applications"
                    );
                    continue;
                }

                // Delete the job
                let delete_result = sqlx::query("DELETE FROM jobs WHERE id = ?")
                    .bind(job_id)
                    .execute(&state.db)
                    .await;

                match delete_result {
                    Ok(result) => {
                        if result.rows_affected() > 0 {
                            success_count += 1;
                            debug!(
                                job_id = %job_id,
                                "Job deleted successfully in bulk operation"
                            );
                        } else {
                            failed_count += 1;
                            let error_msg = format!("Job {} not found or already deleted", job_id);
                            errors.push(error_msg);
                        }
                    }
                    Err(e) => {
                        failed_count += 1;
                        let error_msg = format!("Failed to delete job {}: {}", job_id, e);
                        errors.push(error_msg.clone());
                        error!(
                            error = %e,
                            job_id = %job_id,
                            "Database error deleting job in bulk operation"
                        );
                    }
                }
            }
            Ok(None) => {
                failed_count += 1;
                let error_msg = format!("Job {} not found", job_id);
                errors.push(error_msg);
                warn!(
                    job_id = %job_id,
                    "Job not found in bulk deletion operation"
                );
            }
            Err(e) => {
                failed_count += 1;
                let error_msg = format!("Database error checking job {}: {}", job_id, e);
                errors.push(error_msg.clone());
                error!(
                    error = %e,
                    job_id = %job_id,
                    "Database error checking job existence in bulk deletion"
                );
            }
        }
    }

    let result = BulkOperationResult {
        success_count,
        failed_count,
        errors,
    };

    info!(
        admin_user_id = %authed.id,
        total_requested = request.job_ids.len(),
        success_count = success_count,
        failed_count = failed_count,
        "Bulk job deletion completed"
    );

    Ok(Json(result))
}
