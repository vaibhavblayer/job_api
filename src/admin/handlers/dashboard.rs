// src/admin/handlers/dashboard.rs

use axum::{extract::Extension, Json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::admin::models::{ActivityLog, DashboardMetrics, SystemHealth};
use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};

/// GET /api/admin/dashboard/metrics - Get comprehensive dashboard metrics
pub async fn get_dashboard_metrics(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<DashboardMetrics>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Dashboard metrics access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Fetching dashboard metrics"
    );

    // Get total jobs count
    let total_jobs = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching total jobs count for dashboard metrics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get active jobs count
    let active_jobs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE status = 'active'"
    )
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching active jobs count for dashboard metrics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get draft jobs count
    let draft_jobs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE status = 'draft'"
    )
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching draft jobs count for dashboard metrics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get closed jobs count
    let closed_jobs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE status = 'closed'"
    )
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching closed jobs count for dashboard metrics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get total applications count
    let total_applications = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM applications")
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching total applications count for dashboard metrics"
            );
            ApiError::DatabaseError(e)
        })?;

    // Get pending reviews count
    let pending_reviews = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM applications WHERE status = 'submitted'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            "Database error fetching pending reviews count for dashboard metrics"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get new messages count
    let new_messages = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversation_messages WHERE created_at >= datetime('now', '-1 day')",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            "Database error fetching new messages count for dashboard metrics"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get total candidates count
    let total_candidates = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(DISTINCT u.id) 
        FROM users u 
        LEFT JOIN profiles p ON u.id = p.user_id 
        LEFT JOIN applications a ON u.id = a.user_id 
        WHERE p.user_id IS NOT NULL OR a.user_id IS NOT NULL
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            "Database error fetching total candidates count for dashboard metrics"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get jobs by status breakdown
    let jobs_by_status_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) as count FROM jobs GROUP BY status"
    )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching jobs by status");
            ApiError::DatabaseError(e)
        })?;
    
    let mut jobs_by_status = std::collections::HashMap::new();
    for (status, count) in jobs_by_status_rows {
        jobs_by_status.insert(status, count);
    }

    // Get applications by status breakdown
    let apps_by_status_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) as count FROM applications GROUP BY status"
    )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching applications by status");
            ApiError::DatabaseError(e)
        })?;
    
    let mut applications_by_status = std::collections::HashMap::new();
    for (status, count) in apps_by_status_rows {
        applications_by_status.insert(status, count);
    }

    // Get top jobs by application count
    let top_jobs_rows = sqlx::query_as::<_, (String, String, String, i64, String)>(
        r#"
        SELECT j.id, j.title, j.company, COUNT(a.id) as app_count, j.status
        FROM jobs j
        LEFT JOIN applications a ON j.id = a.job_id
        GROUP BY j.id, j.title, j.company, j.status
        ORDER BY app_count DESC
        LIMIT 5
        "#
    )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching top jobs");
            ApiError::DatabaseError(e)
        })?;
    
    let top_jobs: Vec<crate::admin::models::TopJob> = top_jobs_rows
        .into_iter()
        .map(|(job_id, job_title, company, application_count, status)| {
            crate::admin::models::TopJob {
                job_id,
                job_title,
                company,
                application_count,
                status,
            }
        })
        .collect();

    // Get application trends (last 7 days)
    let trends_rows = sqlx::query_as::<_, (String, i64, i64)>(
        r#"
        SELECT 
            DATE(date) as trend_date,
            COALESCE(SUM(CASE WHEN type = 'application' THEN 1 ELSE 0 END), 0) as applications,
            COALESCE(SUM(CASE WHEN type = 'job' THEN 1 ELSE 0 END), 0) as jobs_posted
        FROM (
            SELECT DATE(applied_at) as date, 'application' as type FROM applications WHERE applied_at >= datetime('now', '-7 days')
            UNION ALL
            SELECT DATE(created_at) as date, 'job' as type FROM jobs WHERE created_at >= datetime('now', '-7 days')
        )
        GROUP BY DATE(date)
        ORDER BY trend_date DESC
        LIMIT 7
        "#
    )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching application trends");
            ApiError::DatabaseError(e)
        })?;
    
    let application_trends: Vec<crate::admin::models::TrendData> = trends_rows
        .into_iter()
        .map(|(date, applications, jobs_posted)| {
            crate::admin::models::TrendData {
                date,
                applications,
                jobs_posted,
            }
        })
        .collect();

    // Get recent activity (already implemented in get_recent_activity, reuse logic)
    let recent_activity = fetch_recent_activity_internal(&state, 10).await?;

    // Determine system health
    let system_health = determine_system_health(&state).await;

    let metrics = DashboardMetrics {
        total_jobs,
        active_jobs,
        draft_jobs,
        closed_jobs,
        total_applications,
        pending_reviews,
        new_messages,
        total_candidates,
        system_health,
        last_updated: chrono::Utc::now().to_rfc3339(),
        jobs_by_status,
        applications_by_status,
        recent_activity,
        top_jobs,
        application_trends,
    };

    info!(
        admin_user_id = %authed.id,
        total_jobs = total_jobs,
        active_jobs = active_jobs,
        total_applications = total_applications,
        pending_reviews = pending_reviews,
        new_messages = new_messages,
        total_candidates = total_candidates,
        system_health = %metrics.system_health,
        "Dashboard metrics fetched successfully"
    );

    Ok(Json(metrics))
}

/// GET /api/admin/system/health - Get system health status
pub async fn get_system_health(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<SystemHealth>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "System health access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Performing system health check"
    );

    let mut details = std::collections::HashMap::new();

    // Check database connectivity
    let database_status = match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => {
            details.insert("database".to_string(), "Connection successful".to_string());
            "healthy"
        }
        Err(e) => {
            error!(error = %e, "Database health check failed");
            details.insert("database".to_string(), format!("Connection failed: {}", e));
            "error"
        }
    };

    // Check storage availability
    let storage_status = match tokio::fs::metadata(&state.resumes_dir).await {
        Ok(metadata) => {
            if metadata.is_dir() {
                details.insert(
                    "storage".to_string(),
                    "Resume directory accessible".to_string(),
                );
                match tokio::fs::metadata(&state.avatars_dir).await {
                    Ok(avatar_metadata) => {
                        if avatar_metadata.is_dir() {
                            details.insert(
                                "avatar_storage".to_string(),
                                "Avatar directory accessible".to_string(),
                            );
                            "healthy"
                        } else {
                            details.insert(
                                "avatar_storage".to_string(),
                                "Avatar directory not accessible".to_string(),
                            );
                            "warning"
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Avatar directory health check failed");
                        details.insert(
                            "avatar_storage".to_string(),
                            format!("Avatar directory error: {}", e),
                        );
                        "warning"
                    }
                }
            } else {
                details.insert(
                    "storage".to_string(),
                    "Resume directory is not a directory".to_string(),
                );
                "error"
            }
        }
        Err(e) => {
            error!(error = %e, "Storage health check failed");
            details.insert("storage".to_string(), format!("Storage error: {}", e));
            "error"
        }
    };

    // API status is healthy if we can respond to this request
    let api_status = "healthy";
    details.insert("api".to_string(), "API responding normally".to_string());

    // Determine overall health
    let overall_health = match (database_status, storage_status, api_status) {
        ("healthy", "healthy", "healthy") => "healthy",
        ("error", _, _) | (_, "error", _) | (_, _, "error") => "error",
        _ => "warning",
    };

    let health = SystemHealth {
        database_status: database_status.to_string(),
        api_status: api_status.to_string(),
        storage_status: storage_status.to_string(),
        overall_health: overall_health.to_string(),
        last_check: chrono::Utc::now().to_rfc3339(),
        details,
    };

    info!(
        admin_user_id = %authed.id,
        database_status = %health.database_status,
        api_status = %health.api_status,
        storage_status = %health.storage_status,
        overall_health = %health.overall_health,
        "System health check completed"
    );

    Ok(Json(health))
}

/// GET /api/admin/activity - Get recent system activity
pub async fn get_recent_activity(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<ActivityLog>>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Activity log access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let limit = 10i64;

    info!(
        admin_user_id = %authed.id,
        limit = limit,
        "Fetching recent system activity"
    );

    let activities = fetch_recent_activity_internal(&state, limit).await?;

    info!(
        admin_user_id = %authed.id,
        activity_count = activities.len(),
        "Recent system activity fetched successfully"
    );

    Ok(Json(activities))
}

// Helper function to fetch recent activity (internal use)
async fn fetch_recent_activity_internal(state: &AppState, limit: i64) -> Result<Vec<ActivityLog>, ApiError> {
    // Get recent resume submissions
    let recent_resumes = sqlx::query_as::<_, (String, String, Option<String>, String)>(
        r#"
        SELECT r.id, r.user_id, u.email, r.submitted_at
        FROM resumes r
        LEFT JOIN users u ON r.user_id = u.id
        ORDER BY r.submitted_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit / 3)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching recent resume activities");
        ApiError::DatabaseError(e)
    })?;

    // Get recent applications
    let recent_applications =
        sqlx::query_as::<_, (String, String, String, Option<String>, String)>(
            r#"
        SELECT a.id, a.user_id, a.job_id, u.email, a.applied_at
        FROM applications a
        LEFT JOIN users u ON a.user_id = u.id
        ORDER BY a.applied_at DESC
        LIMIT ?
        "#,
        )
        .bind(limit / 3)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching recent application activities");
            ApiError::DatabaseError(e)
        })?;

    // Get recent messages
    let recent_messages = sqlx::query_as::<_, (String, String, String, Option<String>, String)>(
        r#"
        SELECT cm.id, cm.user_id, cm.sender, u.email, cm.created_at
        FROM conversation_messages cm
        LEFT JOIN users u ON cm.user_id = u.id
        ORDER BY cm.created_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit / 3)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching recent message activities");
        ApiError::DatabaseError(e)
    })?;

    let mut activities = Vec::new();

    // Convert resume activities
    for (resume_id, user_id, user_email, timestamp) in recent_resumes {
        activities.push(ActivityLog {
            id: format!("resume_{}", resume_id),
            activity_type: "resume_submitted".to_string(),
            description: "Resume uploaded".to_string(),
            user_id: Some(user_id),
            user_email,
            metadata: Some(format!(r#"{{"resume_id": "{}"}}"#, resume_id)),
            timestamp,
        });
    }

    // Convert application activities
    for (app_id, user_id, job_id, user_email, timestamp) in recent_applications {
        activities.push(ActivityLog {
            id: format!("application_{}", app_id),
            activity_type: "application_submitted".to_string(),
            description: "Job application submitted".to_string(),
            user_id: Some(user_id),
            user_email,
            metadata: Some(format!(
                r#"{{"application_id": "{}", "job_id": "{}"}}"#,
                app_id, job_id
            )),
            timestamp,
        });
    }

    // Convert message activities
    for (msg_id, user_id, sender, user_email, timestamp) in recent_messages {
        let activity_type = if sender == "admin" {
            "admin_message_sent"
        } else {
            "user_message_sent"
        };

        activities.push(ActivityLog {
            id: format!("message_{}", msg_id),
            activity_type: activity_type.to_string(),
            description: format!("Message sent by {}", sender),
            user_id: Some(user_id),
            user_email,
            metadata: Some(format!(
                r#"{{"message_id": "{}", "sender": "{}"}}"#,
                msg_id, sender
            )),
            timestamp,
        });
    }

    // Sort activities by timestamp
    activities.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    activities.truncate(limit as usize);

    Ok(activities)
}

// Helper function to determine system health
async fn determine_system_health(state: &AppState) -> String {
    let db_healthy = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    let storage_healthy = tokio::fs::metadata(&state.resumes_dir).await.is_ok()
        && tokio::fs::metadata(&state.avatars_dir).await.is_ok();

    match (db_healthy, storage_healthy) {
        (true, true) => "healthy".to_string(),
        (false, _) => "error".to_string(),
        (_, false) => "warning".to_string(),
    }
}
