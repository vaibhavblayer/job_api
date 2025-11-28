// src/admin/handlers/exports.rs

use axum::{extract::Extension, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};

/// GET /api/admin/export/jobs - Export jobs data in CSV or JSON format
pub async fn export_jobs(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Jobs export access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Exporting jobs data"
    );

    let format = params.get("format").map(|s| s.as_str()).unwrap_or("csv");

    let jobs = sqlx::query_as::<_, crate::jobs::Job>("SELECT * FROM jobs")
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching jobs for export"
            );
            ApiError::DatabaseError(e)
        })?;

    match format {
        "csv" => {
            let mut csv_content = String::from("ID,Title,Description,Location\n");
            let record_count = jobs.len();
            for job in jobs {
                let description = job.description.unwrap_or_default().replace("\"", "\"\"");
                let location = job.location.unwrap_or_default().replace("\"", "\"\"");
                csv_content.push_str(&format!(
                    "\"{}\",\"{}\",\"{}\",\"{}\"\n",
                    job.id, job.title, description, location
                ));
            }

            info!(
                admin_user_id = %authed.id,
                record_count = record_count,
                format = "csv",
                "Jobs data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "text/csv"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"jobs_export.csv\"",
                    ),
                ],
                csv_content,
            ))
        }
        "json" => {
            let json_content = serde_json::to_string_pretty(&jobs).map_err(|e| {
                error!(
                    error = %e,
                    "JSON serialization error during jobs export"
                );
                ApiError::ExportError("Failed to serialize jobs data".to_string())
            })?;

            info!(
                admin_user_id = %authed.id,
                record_count = jobs.len(),
                format = "json",
                "Jobs data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "application/json"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"jobs_export.json\"",
                    ),
                ],
                json_content,
            ))
        }
        _ => {
            warn!(
                admin_user_id = %authed.id,
                format = format,
                "Invalid export format requested"
            );
            Err(ApiError::BadRequest(
                "Invalid format. Use 'csv' or 'json'".to_string(),
            ))
        }
    }
}

/// GET /api/admin/export/applications - Export applications data in CSV or JSON format
pub async fn export_applications(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Applications export access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Exporting applications data"
    );

    let format = params.get("format").map(|s| s.as_str()).unwrap_or("csv");

    let applications_query = r#"
        SELECT 
            a.id,
            a.user_id,
            a.job_id,
            a.resume_id,
            a.status,
            a.cover_letter,
            a.applied_at,
            a.updated_at,
            j.title as job_title,
            u.email as candidate_email,
            u.name as candidate_name
        FROM applications a
        LEFT JOIN jobs j ON a.job_id = j.id
        LEFT JOIN users u ON a.user_id = u.id
        ORDER BY a.applied_at DESC
    "#;

    let applications = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(applications_query)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            "Database error fetching applications for export"
        );
        ApiError::DatabaseError(e)
    })?;

    match format {
        "csv" => {
            let mut csv_content = String::from("Application ID,User ID,Job ID,Job Title,Candidate Email,Candidate Name,Resume ID,Status,Applied At,Updated At\n");
            let record_count = applications.len();
            for app in applications {
                let (
                    id,
                    user_id,
                    job_id,
                    resume_id,
                    status,
                    _cover_letter,
                    applied_at,
                    updated_at,
                    job_title,
                    candidate_email,
                    candidate_name,
                ) = app;
                csv_content.push_str(&format!(
                    "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
                    id,
                    user_id,
                    job_id,
                    job_title.unwrap_or_default(),
                    candidate_email.unwrap_or_default(),
                    candidate_name.unwrap_or_default(),
                    resume_id.unwrap_or_default(),
                    status,
                    applied_at.unwrap_or_default(),
                    updated_at.unwrap_or_default()
                ));
            }

            info!(
                admin_user_id = %authed.id,
                record_count = record_count,
                format = "csv",
                "Applications data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "text/csv"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"applications_export.csv\"",
                    ),
                ],
                csv_content,
            ))
        }
        "json" => {
            let json_data: Vec<serde_json::Value> = applications
                .into_iter()
                .map(
                    |(
                        id,
                        user_id,
                        job_id,
                        resume_id,
                        status,
                        cover_letter,
                        applied_at,
                        updated_at,
                        job_title,
                        candidate_email,
                        candidate_name,
                    )| {
                        serde_json::json!({
                            "id": id,
                            "user_id": user_id,
                            "job_id": job_id,
                            "job_title": job_title,
                            "candidate_email": candidate_email,
                            "candidate_name": candidate_name,
                            "resume_id": resume_id,
                            "status": status,
                            "cover_letter": cover_letter,
                            "applied_at": applied_at,
                            "updated_at": updated_at
                        })
                    },
                )
                .collect();

            let json_content = serde_json::to_string_pretty(&json_data).map_err(|e| {
                error!(
                    error = %e,
                    "JSON serialization error during applications export"
                );
                ApiError::ExportError("Failed to serialize applications data".to_string())
            })?;

            info!(
                admin_user_id = %authed.id,
                record_count = json_data.len(),
                format = "json",
                "Applications data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "application/json"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"applications_export.json\"",
                    ),
                ],
                json_content,
            ))
        }
        _ => {
            warn!(
                admin_user_id = %authed.id,
                format = format,
                "Invalid export format requested"
            );
            Err(ApiError::BadRequest(
                "Invalid format. Use 'csv' or 'json'".to_string(),
            ))
        }
    }
}

/// GET /api/admin/export/candidates - Export candidates data in CSV or JSON format
pub async fn export_candidates(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Candidates export access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Exporting candidates data"
    );

    let format = params.get("format").map(|s| s.as_str()).unwrap_or("csv");

    let candidates_query = r#"
        SELECT 
            u.id,
            u.email,
            u.name,
            u.created_at,
            p.first_name,
            p.last_name,
            p.phone,
            p.location,
            p.bio,
            p.website,
            p.linkedin_url,
            p.github_url,
            p.skills,
            p.resume_status,
            (SELECT COUNT(*) FROM applications WHERE user_id = u.id) as application_count
        FROM users u
        LEFT JOIN profiles p ON u.id = p.user_id
        ORDER BY u.created_at DESC
    "#;

    let candidates = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        ),
    >(candidates_query)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            "Database error fetching candidates for export"
        );
        ApiError::DatabaseError(e)
    })?;

    match format {
        "csv" => {
            let mut csv_content = String::from("User ID,Email,Name,First Name,Last Name,Phone,Location,Website,LinkedIn,GitHub,Resume Status,Application Count,Created At\n");
            let record_count = candidates.len();
            for candidate in candidates {
                let (
                    id,
                    email,
                    name,
                    created_at,
                    first_name,
                    last_name,
                    phone,
                    location,
                    _bio,
                    website,
                    linkedin_url,
                    github_url,
                    _skills,
                    resume_status,
                    application_count,
                ) = candidate;
                csv_content.push_str(&format!(
                    "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
                    id,
                    email,
                    name.unwrap_or_default(),
                    first_name.unwrap_or_default(),
                    last_name.unwrap_or_default(),
                    phone.unwrap_or_default(),
                    location.unwrap_or_default(),
                    website.unwrap_or_default(),
                    linkedin_url.unwrap_or_default(),
                    github_url.unwrap_or_default(),
                    resume_status.unwrap_or_default(),
                    application_count,
                    created_at.unwrap_or_default()
                ));
            }

            info!(
                admin_user_id = %authed.id,
                record_count = record_count,
                format = "csv",
                "Candidates data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "text/csv"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"candidates_export.csv\"",
                    ),
                ],
                csv_content,
            ))
        }
        "json" => {
            let json_data: Vec<serde_json::Value> = candidates
                .into_iter()
                .map(
                    |(
                        id,
                        email,
                        name,
                        created_at,
                        first_name,
                        last_name,
                        phone,
                        location,
                        bio,
                        website,
                        linkedin_url,
                        github_url,
                        skills,
                        resume_status,
                        application_count,
                    )| {
                        serde_json::json!({
                            "id": id,
                            "email": email,
                            "name": name,
                            "first_name": first_name,
                            "last_name": last_name,
                            "phone": phone,
                            "location": location,
                            "bio": bio,
                            "website": website,
                            "linkedin_url": linkedin_url,
                            "github_url": github_url,
                            "skills": skills,
                            "resume_status": resume_status,
                            "application_count": application_count,
                            "created_at": created_at
                        })
                    },
                )
                .collect();

            let json_content = serde_json::to_string_pretty(&json_data).map_err(|e| {
                error!(
                    error = %e,
                    "JSON serialization error during candidates export"
                );
                ApiError::ExportError("Failed to serialize candidates data".to_string())
            })?;

            info!(
                admin_user_id = %authed.id,
                record_count = json_data.len(),
                format = "json",
                "Candidates data exported successfully"
            );

            Ok((
                StatusCode::OK,
                [
                    ("Content-Type", "application/json"),
                    (
                        "Content-Disposition",
                        "attachment; filename=\"candidates_export.json\"",
                    ),
                ],
                json_content,
            ))
        }
        _ => {
            warn!(
                admin_user_id = %authed.id,
                format = format,
                "Invalid export format requested"
            );
            Err(ApiError::BadRequest(
                "Invalid format. Use 'csv' or 'json'".to_string(),
            ))
        }
    }
}
