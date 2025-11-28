// src/candidates/handlers/saved_jobs.rs
//! Saved jobs handlers for user job bookmarking functionality

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::auth::AuthedUser;
use crate::common::{generate_raw_id, AppState};

#[derive(Debug, Serialize)]
pub struct SavedJob {
    pub id: String,
    pub job_id: String,
    pub saved_at: String,
    // Job details
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub job_type: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub status: Option<String>,
    pub company_logo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveJobRequest {
    pub job_id: String,
}

#[derive(Debug, Serialize)]
pub struct SaveJobResponse {
    pub success: bool,
    pub message: String,
    pub saved: bool,
}

/// Save a job for the authenticated user
pub async fn save_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(payload): Json<SaveJobRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<SaveJobResponse>)> {
    let state = state_lock.read().await;
    let user_id = &authed.id;
    let job_id = &payload.job_id;

    // Check if job exists
    let job_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM jobs WHERE id = ?")
        .bind(job_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!("Database error checking job: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(SaveJobResponse {
                success: false,
                message: "Database error".to_string(),
                saved: false,
            }))
        })?;

    if job_exists.is_none() {
        return Err((StatusCode::NOT_FOUND, Json(SaveJobResponse {
            success: false,
            message: "Job not found".to_string(),
            saved: false,
        })));
    }

    // Check if already saved
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM saved_jobs WHERE user_id = ? AND job_id = ?"
    )
    .bind(user_id)
    .bind(job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Database error checking saved job: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(SaveJobResponse {
            success: false,
            message: "Database error".to_string(),
            saved: false,
        }))
    })?;

    if existing.is_some() {
        return Ok(Json(SaveJobResponse {
            success: true,
            message: "Job already saved".to_string(),
            saved: true,
        }));
    }

    // Save the job
    let id = format!("SV_{}", generate_raw_id(6));
    sqlx::query(
        r#"
        INSERT INTO saved_jobs (id, user_id, job_id, saved_at)
        VALUES (?, ?, ?, datetime('now'))
        "#
    )
    .bind(&id)
    .bind(user_id)
    .bind(job_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to save job: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(SaveJobResponse {
            success: false,
            message: "Failed to save job".to_string(),
            saved: false,
        }))
    })?;

    info!(user_id = %user_id, job_id = %job_id, "Job saved");

    Ok(Json(SaveJobResponse {
        success: true,
        message: "Job saved successfully".to_string(),
        saved: true,
    }))
}

/// Unsave/remove a saved job
pub async fn unsave_job(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(job_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<SaveJobResponse>)> {
    let state = state_lock.read().await;
    let user_id = &authed.id;

    let result = sqlx::query("DELETE FROM saved_jobs WHERE user_id = ? AND job_id = ?")
        .bind(user_id)
        .bind(&job_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to unsave job: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(SaveJobResponse {
                success: false,
                message: "Failed to unsave job".to_string(),
                saved: false,
            }))
        })?;

    if result.rows_affected() == 0 {
        return Ok(Json(SaveJobResponse {
            success: true,
            message: "Job was not saved".to_string(),
            saved: false,
        }));
    }

    info!(user_id = %user_id, job_id = %job_id, "Job unsaved");

    Ok(Json(SaveJobResponse {
        success: true,
        message: "Job removed from saved".to_string(),
        saved: false,
    }))
}

/// Get all saved jobs for the authenticated user
pub async fn get_saved_jobs(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let state = state_lock.read().await;
    let user_id = &authed.id;

    let saved_jobs: Vec<SavedJob> = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>, Option<String>)>(
        r#"
        SELECT 
            sj.id,
            sj.job_id,
            sj.saved_at,
            j.title,
            j.company,
            j.location,
            j.job_type,
            j.salary_min,
            j.salary_max,
            j.status,
            j.company_logo_url
        FROM saved_jobs sj
        LEFT JOIN jobs j ON sj.job_id = j.id
        WHERE sj.user_id = ?
        ORDER BY sj.saved_at DESC
        "#
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to fetch saved jobs: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch saved jobs".to_string())
    })?
    .into_iter()
    .map(|(id, job_id, saved_at, title, company, location, job_type, salary_min, salary_max, status, company_logo_url)| {
        SavedJob {
            id,
            job_id,
            saved_at,
            title,
            company,
            location,
            job_type,
            salary_min,
            salary_max,
            status,
            company_logo_url,
        }
    })
    .collect();

    Ok(Json(saved_jobs))
}

/// Check if a job is saved by the user
pub async fn is_job_saved(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(job_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<SaveJobResponse>)> {
    let state = state_lock.read().await;
    let user_id = &authed.id;

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM saved_jobs WHERE user_id = ? AND job_id = ?"
    )
    .bind(user_id)
    .bind(&job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!("Database error checking saved job: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(SaveJobResponse {
            success: false,
            message: "Database error".to_string(),
            saved: false,
        }))
    })?;

    Ok(Json(SaveJobResponse {
        success: true,
        message: if existing.is_some() { "Job is saved".to_string() } else { "Job is not saved".to_string() },
        saved: existing.is_some(),
    }))
}
