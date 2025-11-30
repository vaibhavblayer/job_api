// src/jobs/handlers/public.rs

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::common::{generate_view_id, ApiError, AppState, Validator};
use crate::jobs::models::*;
use crate::jobs::validators::*;

/// GET /api/jobs - List jobs (with optional featured filter and pagination)
pub async fn list_jobs_or_featured(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Query(params): Query<JobQueryParams>,
) -> Result<Json<JobListResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    // Parse pagination parameters with defaults
    let page = params.page.unwrap_or(1).max(1); // Ensure page is at least 1
    let limit = params.limit.unwrap_or(20).clamp(1, 100); // Limit between 1 and 100
    let offset = (page - 1) * limit;

    // Check if featured parameter is set to "true"
    let is_featured_query = params.featured.as_deref() == Some("true");

    // Get total count
    let total: i64 = if is_featured_query {
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'active' AND is_featured = 1")
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'active'")
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?
    };

    // Get paginated jobs
    let jobs = if is_featured_query {
        // Get featured jobs only
        sqlx::query_as::<_, Job>(
            r#"SELECT 
                id, title, summary, description, location, company, company_id, company_logo_url, job_image_url,
                salary_min, salary_max, job_type, experience_level, requirements, benefits,
                status, is_featured, created_at, updated_at, published_at
            FROM jobs 
            WHERE status = 'active' AND is_featured = 1
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?"#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
    } else {
        // Get all active jobs
        sqlx::query_as::<_, Job>(
            r#"SELECT 
                id, title, summary, description, location, company, company_id, company_logo_url, job_image_url,
                salary_min, salary_max, job_type, experience_level, requirements, benefits,
                status, is_featured, created_at, updated_at, published_at
            FROM jobs 
            WHERE status = 'active'
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?"#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
    };

    // Convert to JobResponse to parse requirements and benefits JSON to arrays
    let job_responses: Vec<JobResponse> = jobs.into_iter().map(|j| j.into()).collect();

    debug!(
        job_count = job_responses.len(),
        total = total,
        page = page,
        limit = limit,
        featured = is_featured_query,
        "Successfully loaded paginated jobs list"
    );

    Ok(Json(JobListResponse {
        jobs: job_responses,
        total: total as usize,
        page,
        page_size: limit,
    }))
}

/// GET /api/jobs/:id - Get a specific job by ID (public endpoint)
pub async fn get_job_by_id(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    // Fetch the job, but only if it's active (published)
    let job = sqlx::query_as::<_, Job>(
        r#"SELECT 
            id, title, summary, description, location, company, company_id, company_logo_url, job_image_url,
            salary_min, salary_max, job_type, experience_level, requirements, benefits,
            status, is_featured, created_at, updated_at, published_at
        FROM jobs 
        WHERE id = ? AND status = 'active'"#,
    )
    .bind(&job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest(format!("Job not found: {}", job_id)))?;

    debug!(job_id = %job_id, job_title = %job.title, "Successfully loaded job details");

    Ok(Json(job.into()))
}

/// POST /api/jobs/:id/view - Track a job view
pub async fn track_job_view(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(job_id): Path<String>,
    Json(request): Json<JobViewRequest>,
) -> Result<StatusCode, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        job_id = %job_id,
        "Tracking job view"
    );

    // Validate the request
    let validator = JobAnalyticsValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            job_id = %job_id,
            errors = ?validation_result.errors,
            "Job view tracking validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    // Check if job exists
    let job_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                job_id = %job_id,
                "Database error checking job existence for view tracking"
            );
            ApiError::DatabaseError(e)
        })?;

    if job_exists == 0 {
        warn!(
            job_id = %job_id,
            "Job view tracking failed: job not found"
        );
        return Err(ApiError::BadRequest("Job not found".to_string()));
    }

    // Create job view record
    let view_id = generate_view_id();
    sqlx::query(
        r#"
        INSERT INTO job_views (id, job_id, user_id, ip_address, user_agent, viewed_at)
        VALUES (?, ?, ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(&view_id)
    .bind(&job_id)
    .bind(None::<String>) // user_id - not available without auth
    .bind("unknown") // ip_address - not available without ConnectInfo
    .bind(request.user_agent.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %job_id,
            view_id = %view_id,
            "Database error creating job view record"
        );
        ApiError::DatabaseError(e)
    })?;

    debug!(
        job_id = %job_id,
        view_id = %view_id,
        "Job view tracked successfully"
    );

    Ok(StatusCode::CREATED)
}


/// Public statistics response for the home page
#[derive(serde::Serialize)]
pub struct PublicStats {
    pub total_jobs: i64,
    pub active_jobs: i64,
    pub total_companies: i64,
    pub total_placements: i64,
    pub total_candidates: i64,
    pub success_rate: f64,
}

/// GET /api/public/stats - Get public statistics for the home page
pub async fn get_public_stats(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
) -> Result<Json<PublicStats>, ApiError> {
    let state = state_lock.read().await.clone();

    // Get total active jobs
    let active_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'active'")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Get total jobs (all statuses)
    let total_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Get unique companies from jobs
    let total_companies: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT company) FROM jobs WHERE company IS NOT NULL AND company != ''")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Get total candidates (users who are not admins)
    let total_candidates: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE id NOT IN (SELECT user_id FROM admin_users)"
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get total successful placements (hired applications)
    let total_placements: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM applications WHERE status = 'hired'"
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Calculate success rate (hired / total applications)
    let total_applications: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM applications")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let success_rate = if total_applications > 0 {
        (total_placements as f64 / total_applications as f64) * 100.0
    } else {
        0.0
    };

    debug!(
        active_jobs = active_jobs,
        total_companies = total_companies,
        total_candidates = total_candidates,
        total_placements = total_placements,
        "Public stats fetched successfully"
    );

    Ok(Json(PublicStats {
        total_jobs,
        active_jobs,
        total_companies,
        total_placements,
        total_candidates,
        success_rate,
    }))
}
