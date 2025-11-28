// src/jobs/handlers/analytics.rs

use axum::{
    extract::{Extension, Path},
    response::Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};
use crate::jobs::models::*;

/// GET /api/admin/jobs/analytics - Get job analytics with optional filtering
pub async fn get_job_analytics(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<JobAnalyticsResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Job analytics access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Fetching job analytics"
    );

    // Get total jobs count
    let total_jobs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching total jobs count for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get total views
    let total_views = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM job_views")
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching total views count for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get total applications
    let total_applications_query = "SELECT COUNT(*) FROM applications".to_string();

    let total_applications = sqlx::query_scalar::<_, i64>(&total_applications_query)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching total applications count for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Calculate average conversion rate
    let average_conversion_rate = if total_views > 0 {
        total_applications as f64 / total_views as f64
    } else {
        0.0
    };

    // Get top performing jobs
    let top_jobs_query = r#"
        SELECT 
            j.id as job_id,
            j.title as job_title,
            COUNT(DISTINCT jv.id) as views,
            COUNT(DISTINCT a.id) as applications,
            CASE 
                WHEN COUNT(DISTINCT jv.id) > 0 
                THEN CAST(COUNT(DISTINCT a.id) AS REAL) / COUNT(DISTINCT jv.id)
                ELSE 0.0 
            END as conversion_rate,
            j.created_at
        FROM jobs j
        LEFT JOIN job_views jv ON j.id = jv.job_id
        LEFT JOIN applications a ON j.id = a.job_id
        GROUP BY j.id, j.title, j.created_at
        ORDER BY views DESC, applications DESC
        LIMIT 10
        "#;

    let top_jobs_query_builder =
        sqlx::query_as::<_, (String, String, i64, i64, f64, Option<String>)>(&top_jobs_query);

    let top_jobs_data = top_jobs_query_builder
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching top performing jobs for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    let top_performing_jobs: Vec<JobPerformanceStats> = top_jobs_data
        .into_iter()
        .map(
            |(job_id, job_title, views, applications, conversion_rate, created_at)| {
                JobPerformanceStats {
                    job_id,
                    job_title,
                    views,
                    applications,
                    conversion_rate,
                    created_at,
                }
            },
        )
        .collect();

    // Get view trends (daily aggregation)
    let view_trends_query = r#"
        SELECT 
            DATE(viewed_at) as date,
            COUNT(*) as count
        FROM job_views 
        GROUP BY DATE(viewed_at)
        ORDER BY date DESC
        LIMIT 30
        "#;

    let view_trends_query_builder = sqlx::query_as::<_, (String, i64)>(&view_trends_query);

    let view_trends_data = view_trends_query_builder
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching view trends for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    let view_trends: Vec<DailyMetric> = view_trends_data
        .into_iter()
        .map(|(date, count)| DailyMetric { date, count })
        .collect();

    // Get application trends (daily aggregation)
    let app_trends_query = r#"
        SELECT 
            DATE(applied_at) as date,
            COUNT(*) as count
        FROM applications 
        GROUP BY DATE(applied_at)
        ORDER BY date DESC
        LIMIT 30
        "#;

    let app_trends_query_builder = sqlx::query_as::<_, (String, i64)>(app_trends_query);

    let app_trends_data = app_trends_query_builder
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching application trends for analytics"
            );
            ApiError::DatabaseError(e)
        })?;

    let application_trends: Vec<DailyMetric> = app_trends_data
        .into_iter()
        .map(|(date, count)| DailyMetric { date, count })
        .collect();

    let analytics = JobAnalyticsResponse {
        total_jobs,
        total_views,
        total_applications,
        average_conversion_rate,
        top_performing_jobs,
        view_trends,
        application_trends,
    };

    info!(
        admin_user_id = %authed.id,
        total_jobs = total_jobs,
        total_views = total_views,
        total_applications = total_applications,
        "Job analytics fetched successfully"
    );

    Ok(Json(analytics))
}

/// GET /api/jobs/:id/stats - Get statistics for a specific job
pub async fn get_job_stats(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(job_id): Path<String>,
    authed: AuthedUser,
) -> Result<Json<JobStats>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            job_id = %job_id,
            "Job stats access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        job_id = %job_id,
        "Fetching job statistics"
    );

    // Check if job exists and get job title
    let job_info = sqlx::query_as::<_, (String, String)>("SELECT id, title FROM jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                job_id = %job_id,
                "Database error checking job existence for stats"
            );
            ApiError::DatabaseError(e)
        })?;

    let (_, job_title) = job_info.ok_or_else(|| {
        warn!(
            job_id = %job_id,
            "Job stats request failed: job not found"
        );
        ApiError::BadRequest("Job not found".to_string())
    })?;

    // Get total views
    let total_views =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM job_views WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    job_id = %job_id,
                    "Database error fetching total views for job stats"
                );
                ApiError::DatabaseError(e)
            })?;

    // Get unique views (distinct by user_id and ip_address combination)
    let unique_views = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(DISTINCT COALESCE(user_id, '') || '-' || COALESCE(ip_address, ''))
        FROM job_views 
        WHERE job_id = ?
        "#,
    )
    .bind(&job_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %job_id,
            "Database error fetching unique views for job stats"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get total applications
    let total_applications =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM applications WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    job_id = %job_id,
                    "Database error fetching total applications for job stats"
                );
                ApiError::DatabaseError(e)
            })?;

    // Calculate conversion rate
    let conversion_rate = if total_views > 0 {
        total_applications as f64 / total_views as f64
    } else {
        0.0
    };

    // Get recent views (last 10)
    let recent_views = sqlx::query_as::<_, JobView>(
        "SELECT * FROM job_views WHERE job_id = ? ORDER BY viewed_at DESC LIMIT 10",
    )
    .bind(&job_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %job_id,
            "Database error fetching recent views for job stats"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get view trend (last 30 days)
    let view_trend = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT 
            DATE(viewed_at) as date,
            COUNT(*) as count
        FROM job_views 
        WHERE job_id = ? AND viewed_at >= date('now', '-30 days')
        GROUP BY DATE(viewed_at)
        ORDER BY date DESC
        "#,
    )
    .bind(&job_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %job_id,
            "Database error fetching view trend for job stats"
        );
        ApiError::DatabaseError(e)
    })?
    .into_iter()
    .map(|(date, count)| DailyMetric { date, count })
    .collect();

    // Get application trend (last 30 days)
    let application_trend = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT 
            DATE(applied_at) as date,
            COUNT(*) as count
        FROM applications 
        WHERE job_id = ? AND applied_at >= date('now', '-30 days')
        GROUP BY DATE(applied_at)
        ORDER BY date DESC
        "#,
    )
    .bind(&job_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            job_id = %job_id,
            "Database error fetching application trend for job stats"
        );
        ApiError::DatabaseError(e)
    })?
    .into_iter()
    .map(|(date, count)| DailyMetric { date, count })
    .collect();

    let stats = JobStats {
        job_id: job_id.clone(),
        job_title,
        total_views,
        unique_views,
        total_applications,
        conversion_rate,
        recent_views,
        view_trend,
        application_trend,
    };

    info!(
        admin_user_id = %authed.id,
        job_id = %job_id,
        total_views = total_views,
        unique_views = unique_views,
        total_applications = total_applications,
        conversion_rate = conversion_rate,
        "Job statistics fetched successfully"
    );

    Ok(Json(stats))
}

/// GET /api/admin/jobs/:id/detailed-analytics - Get detailed job analytics with candidate data
pub async fn admin_get_job_detailed_analytics(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<Json<JobDetailedAnalytics>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    // Get job title
    let job_title: Option<(String,)> = sqlx::query_as("SELECT title FROM jobs WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let job_title = match job_title {
        Some((title,)) => title,
        None => return Err(ApiError::BadRequest("job not found".to_string())),
    };

    // Get view count
    let view_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM job_views WHERE job_id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Get application count
    let application_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM applications WHERE job_id = ?")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

    // Calculate conversion rate
    let conversion_rate = if view_count.0 > 0 {
        (application_count.0 as f64 / view_count.0 as f64) * 100.0
    } else {
        0.0
    };

    // Get view trend (last 30 days)
    let view_trend: Vec<DailyMetric> = sqlx::query_as(
        r#"SELECT 
            DATE(viewed_at) as date,
            COUNT(*) as count
        FROM job_views
        WHERE job_id = ? AND viewed_at >= datetime('now', '-30 days')
        GROUP BY DATE(viewed_at)
        ORDER BY date"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get application trend (last 30 days)
    let application_trend: Vec<DailyMetric> = sqlx::query_as(
        r#"SELECT 
            DATE(applied_at) as date,
            COUNT(*) as count
        FROM applications
        WHERE job_id = ? AND applied_at >= datetime('now', '-30 days')
        GROUP BY DATE(applied_at)
        ORDER BY date"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get top referrers
    let top_referrers: Vec<ReferrerStats> = sqlx::query_as(
        r#"SELECT 
            COALESCE(user_agent, 'Direct') as referrer,
            COUNT(*) as count
        FROM job_views
        WHERE job_id = ?
        GROUP BY user_agent
        ORDER BY count DESC
        LIMIT 5"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get applications by stage
    let stage_counts: Vec<(String, i64)> = sqlx::query_as(
        r#"SELECT current_stage, COUNT(*) as count
        FROM applications
        WHERE job_id = ?
        GROUP BY current_stage"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let mut applications_by_stage = HashMap::new();
    for (stage, count) in stage_counts {
        applications_by_stage.insert(stage, count);
    }

    // Get candidate list
    let candidate_list: Vec<CandidateApplication> = sqlx::query_as(
        r#"SELECT 
            id, job_id, user_id as candidate_id, resume_id, current_stage, status,
            cover_letter, applied_at, updated_at
        FROM applications
        WHERE job_id = ?
        ORDER BY applied_at DESC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Get status history
    let status_history: Vec<JobStatusHistory> = sqlx::query_as(
        r#"SELECT 
            jsh.id, jsh.job_id, jsh.old_status, jsh.new_status, 
            jsh.changed_by, u.email as changed_by_name, jsh.notes, jsh.changed_at
        FROM job_status_history jsh
        LEFT JOIN users u ON jsh.changed_by = u.id
        WHERE jsh.job_id = ?
        ORDER BY jsh.changed_at DESC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let analytics = JobDetailedAnalytics {
        job_id: id.clone(),
        job_title,
        views: view_count.0,
        applications: application_count.0,
        conversion_rate,
        view_trend,
        application_trend,
        top_referrers,
        applications_by_stage,
        candidate_list,
        status_history,
    };

    info!(
        job_id = %id,
        user_id = %authed.id,
        "Job detailed analytics retrieved successfully"
    );

    Ok(Json(analytics))
}
