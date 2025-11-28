// src/admin/handlers/users.rs

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::admin::models::{
    AdminUser, CandidateProfile, CreateAdminUserRequest, UpdateAdminUserRequest,
};
use crate::auth::{AuthedUser, User};
use crate::common::{generate_user_id, ApiError, AppState};
use crate::profile::models::Profile;

/// GET /api/admin/users - Get admin user list
pub async fn get_admin_users(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<AdminUser>>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Admin users list access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Fetching admin users list"
    );

    let admin_users =
        sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    "Database error fetching admin users list"
                );
                ApiError::DatabaseError(e)
            })?;

    info!(
        admin_user_id = %authed.id,
        admin_user_count = admin_users.len(),
        "Admin users list fetched successfully"
    );

    Ok(Json(admin_users))
}

/// POST /api/admin/users - Create admin user
pub async fn create_admin_user(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateAdminUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Admin user creation access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        target_user_id = %request.user_id,
        role = %request.role,
        "Creating admin user"
    );

    // Validate the request
    if request.user_id.trim().is_empty() {
        return Err(ApiError::ValidationError("User ID is required".to_string()));
    }
    if request.role.trim().is_empty() {
        return Err(ApiError::ValidationError("Role is required".to_string()));
    }

    // Check if user exists
    let user_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id = ?")
        .bind(&request.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_user_id = %request.user_id,
                "Database error checking user existence for admin creation"
            );
            ApiError::DatabaseError(e)
        })?;

    if user_exists == 0 {
        warn!(
            target_user_id = %request.user_id,
            "Admin user creation failed: user not found"
        );
        return Err(ApiError::BadRequest("User not found".to_string()));
    }

    // Check if user is already an admin
    let admin_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admin_users WHERE user_id = ?")
            .bind(&request.user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    target_user_id = %request.user_id,
                    "Database error checking admin user existence"
                );
                ApiError::DatabaseError(e)
            })?;

    if admin_exists > 0 {
        warn!(
            target_user_id = %request.user_id,
            "Admin user creation failed: user is already an admin"
        );
        return Err(ApiError::BadRequest("User is already an admin".to_string()));
    }

    // Convert permissions to JSON string
    let permissions_json = request
        .permissions
        .as_ref()
        .map(|perms| serde_json::to_string(perms).unwrap_or_else(|_| "[]".to_string()));

    // Create admin user record
    let admin_id = generate_user_id();
    sqlx::query(
        r#"
        INSERT INTO admin_users (id, user_id, role, permissions, created_at, created_by)
        VALUES (?, ?, ?, ?, datetime('now'), ?)
        "#,
    )
    .bind(&admin_id)
    .bind(&request.user_id)
    .bind(&request.role)
    .bind(permissions_json.as_deref())
    .bind(&authed.id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            admin_id = %admin_id,
            target_user_id = %request.user_id,
            "Database error creating admin user"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the created admin user
    let admin_user = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE id = ?")
        .bind(&admin_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                admin_id = %admin_id,
                "Database error fetching created admin user"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        admin_user_id = %authed.id,
        created_admin_id = %admin_id,
        target_user_id = %request.user_id,
        role = %request.role,
        "Admin user created successfully"
    );

    Ok(Json(admin_user))
}

/// PUT /api/admin/users/:id - Update admin user
pub async fn update_admin_user(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(admin_user_id): Path<String>,
    Json(request): Json<UpdateAdminUserRequest>,
) -> Result<Json<AdminUser>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            admin_user_id = %admin_user_id,
            "Admin user update access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        "Updating admin user"
    );

    // Check if at least one field is provided
    if request.role.is_none() && request.permissions.is_none() {
        return Err(ApiError::BadRequest(
            "At least one field must be provided for update".to_string(),
        ));
    }

    // Check if admin user exists
    let admin_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admin_users WHERE id = ?")
            .bind(&admin_user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    target_admin_id = %admin_user_id,
                    "Database error checking admin user existence for update"
                );
                ApiError::DatabaseError(e)
            })?;

    if admin_exists == 0 {
        warn!(
            target_admin_id = %admin_user_id,
            "Admin user update failed: admin user not found"
        );
        return Err(ApiError::BadRequest("Admin user not found".to_string()));
    }

    // Convert permissions to JSON string if provided
    let permissions_json = request
        .permissions
        .as_ref()
        .map(|perms| serde_json::to_string(perms).unwrap_or_else(|_| "[]".to_string()));

    // Update admin user
    sqlx::query(
        r#"
        UPDATE admin_users 
        SET role = COALESCE(?, role),
            permissions = COALESCE(?, permissions)
        WHERE id = ?
        "#,
    )
    .bind(request.role.as_deref())
    .bind(permissions_json.as_deref())
    .bind(&admin_user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            target_admin_id = %admin_user_id,
            "Database error updating admin user"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the updated admin user
    let admin_user = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE id = ?")
        .bind(&admin_user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_admin_id = %admin_user_id,
                "Database error fetching updated admin user"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        "Admin user updated successfully"
    );

    Ok(Json(admin_user))
}

/// DELETE /api/admin/users/:id - Delete admin user
pub async fn delete_admin_user(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(admin_user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            admin_user_id = %admin_user_id,
            "Admin user deletion access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        "Deleting admin user"
    );

    // Prevent self-deletion
    let admin_user_info =
        sqlx::query_as::<_, (String, String)>("SELECT id, user_id FROM admin_users WHERE id = ?")
            .bind(&admin_user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    target_admin_id = %admin_user_id,
                    "Database error checking admin user for deletion"
                );
                ApiError::DatabaseError(e)
            })?;

    let (_, target_user_id) = admin_user_info.ok_or_else(|| {
        warn!(
            target_admin_id = %admin_user_id,
            "Admin user deletion failed: admin user not found"
        );
        ApiError::BadRequest("Admin user not found".to_string())
    })?;

    if target_user_id == authed.id {
        warn!(
            admin_user_id = %authed.id,
            target_admin_id = %admin_user_id,
            "Admin user deletion failed: cannot delete self"
        );
        return Err(ApiError::BadRequest(
            "Cannot delete your own admin account".to_string(),
        ));
    }

    // Delete admin user
    let result = sqlx::query("DELETE FROM admin_users WHERE id = ?")
        .bind(&admin_user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_admin_id = %admin_user_id,
                "Database error deleting admin user"
            );
            ApiError::DatabaseError(e)
        })?;

    if result.rows_affected() == 0 {
        warn!(
            target_admin_id = %admin_user_id,
            "Admin user deletion failed: admin user not found"
        );
        return Err(ApiError::BadRequest("Admin user not found".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        "Admin user deleted successfully"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/admin/users/:id/toggle-status - Toggle admin user status
pub async fn toggle_admin_user_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(admin_user_id): Path<String>,
) -> Result<Json<AdminUser>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            admin_user_id = %admin_user_id,
            "Admin user status toggle access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        "Toggling admin user status"
    );

    // Get current admin user
    let current_admin = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE id = ?")
        .bind(&admin_user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_admin_id = %admin_user_id,
                "Database error fetching admin user for status toggle"
            );
            ApiError::DatabaseError(e)
        })?
        .ok_or_else(|| {
            warn!(
                target_admin_id = %admin_user_id,
                "Admin user status toggle failed: admin user not found"
            );
            ApiError::BadRequest("Admin user not found".to_string())
        })?;

    // Prevent self-status toggle
    if current_admin.user_id == authed.id {
        warn!(
            admin_user_id = %authed.id,
            target_admin_id = %admin_user_id,
            "Admin user status toggle failed: cannot toggle own status"
        );
        return Err(ApiError::BadRequest(
            "Cannot toggle your own admin status".to_string(),
        ));
    }

    // Toggle status (role)
    let new_role = match current_admin.role.as_str() {
        "admin" => "inactive",
        "inactive" => "admin",
        _ => "admin",
    };

    // Update admin user role
    sqlx::query("UPDATE admin_users SET role = ? WHERE id = ?")
        .bind(new_role)
        .bind(&admin_user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_admin_id = %admin_user_id,
                new_role = %new_role,
                "Database error toggling admin user status"
            );
            ApiError::DatabaseError(e)
        })?;

    // Fetch the updated admin user
    let updated_admin = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE id = ?")
        .bind(&admin_user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                target_admin_id = %admin_user_id,
                "Database error fetching updated admin user after status toggle"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        admin_user_id = %authed.id,
        target_admin_id = %admin_user_id,
        old_role = %current_admin.role,
        new_role = %updated_admin.role,
        "Admin user status toggled successfully"
    );

    Ok(Json(updated_admin))
}

/// GET /api/admin/candidates - Get candidate list
pub async fn get_admin_candidates(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<CandidateProfile>>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Candidate list access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Fetching candidate list"
    );

    let candidates_query = r#"
        SELECT DISTINCT u.id, u.email, u.name, u.avatar, u.provider, u.provider_id, u.created_at
        FROM users u
        LEFT JOIN profiles p ON u.id = p.user_id
        LEFT JOIN applications a ON u.id = a.user_id
        WHERE u.id != ?
        ORDER BY u.created_at DESC
        "#;

    let users = sqlx::query_as::<_, User>(candidates_query)
        .bind(&authed.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Database error fetching candidates list"
            );
            ApiError::DatabaseError(e)
        })?;

    let mut candidates = Vec::new();

    for user in users {
        let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE user_id = ?")
            .bind(&user.id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    user_id = %user.id,
                    "Database error fetching candidate profile"
                );
                ApiError::DatabaseError(e)
            })?;

        let experiences = sqlx::query_as::<_, crate::profile::models::Experience>(
            "SELECT * FROM experiences WHERE user_id = ? ORDER BY start_date DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                "Database error fetching candidate experiences"
            );
            ApiError::DatabaseError(e)
        })?;

        let education = sqlx::query_as::<_, crate::profile::models::Education>(
            "SELECT * FROM education WHERE user_id = ? ORDER BY start_date DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                "Database error fetching candidate education"
            );
            ApiError::DatabaseError(e)
        })?;

        let applications = sqlx::query_as::<_, crate::candidates::models::Application>(
            "SELECT * FROM applications WHERE user_id = ? ORDER BY applied_at DESC",
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                "Database error fetching candidate applications"
            );
            ApiError::DatabaseError(e)
        })?;

        // Get the latest resume for this candidate
        let resume_info: Option<(String, String)> = sqlx::query_as(
            "SELECT id, filename FROM resumes WHERE user_id = ? AND (deleted_at IS NULL OR deleted_at = '') ORDER BY submitted_at DESC LIMIT 1"
        )
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                "Database error fetching resume info"
            );
            ApiError::DatabaseError(e)
        })?;

        let (resume_status, resume_id, resume_filename) = if let Some((id, filename)) = resume_info {
            ("uploaded".to_string(), Some(id), Some(filename))
        } else {
            ("not_uploaded".to_string(), None, None)
        };

        let last_activity = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT MAX(datetime) as last_activity FROM (
                SELECT applied_at as datetime FROM applications WHERE user_id = ?
                UNION ALL
                SELECT updated_at as datetime FROM profiles WHERE user_id = ?
                UNION ALL
                SELECT submitted_at as datetime FROM resumes WHERE user_id = ? AND (deleted_at IS NULL OR deleted_at = '')
            )
            "#
        )
        .bind(&user.id)
        .bind(&user.id)
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                "Database error fetching candidate last activity"
            );
            ApiError::DatabaseError(e)
        })?
        .flatten();

        let total_applications = applications.len() as i64;
        let successful_applications = applications
            .iter()
            .filter(|app| matches!(app.status.as_str(), "hired" | "offered"))
            .count() as i64;

        let application_success_rate = if total_applications > 0 {
            successful_applications as f64 / total_applications as f64
        } else {
            0.0
        };

        let candidate = CandidateProfile {
            user,
            profile,
            experiences,
            education,
            applications,
            resume_status,
            resume_id,
            resume_filename,
            last_activity,
            total_applications,
            application_success_rate,
        };

        candidates.push(candidate);
    }

    info!(
        admin_user_id = %authed.id,
        candidate_count = candidates.len(),
        "Candidate list fetched successfully"
    );

    Ok(Json(candidates))
}

/// GET /api/admin/candidates/:id - Get detailed candidate profile
pub async fn get_admin_candidate_details(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(candidate_id): Path<String>,
) -> Result<Json<CandidateProfile>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            candidate_id = %candidate_id,
            "Candidate details access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        candidate_id = %candidate_id,
        "Fetching candidate details"
    );

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&candidate_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                candidate_id = %candidate_id,
                "Database error fetching candidate user"
            );
            ApiError::DatabaseError(e)
        })?
        .ok_or_else(|| {
            warn!(
                candidate_id = %candidate_id,
                "Candidate not found"
            );
            ApiError::BadRequest("Candidate not found".to_string())
        })?;

    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE user_id = ?")
        .bind(&candidate_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                candidate_id = %candidate_id,
                "Database error fetching candidate profile"
            );
            ApiError::DatabaseError(e)
        })?;

    let experiences = sqlx::query_as::<_, crate::profile::models::Experience>(
        "SELECT * FROM experiences WHERE user_id = ? ORDER BY start_date DESC",
    )
    .bind(&candidate_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            candidate_id = %candidate_id,
            "Database error fetching candidate experiences"
        );
        ApiError::DatabaseError(e)
    })?;

    let education = sqlx::query_as::<_, crate::profile::models::Education>(
        "SELECT * FROM education WHERE user_id = ? ORDER BY start_date DESC",
    )
    .bind(&candidate_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            candidate_id = %candidate_id,
            "Database error fetching candidate education"
        );
        ApiError::DatabaseError(e)
    })?;

    let applications = sqlx::query_as::<_, crate::candidates::models::Application>(
        "SELECT * FROM applications WHERE user_id = ? ORDER BY applied_at DESC",
    )
    .bind(&candidate_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            candidate_id = %candidate_id,
            "Database error fetching candidate applications"
        );
        ApiError::DatabaseError(e)
    })?;

    // Get the latest resume for this candidate
    let resume_info: Option<(String, String)> = sqlx::query_as(
        "SELECT id, filename FROM resumes WHERE user_id = ? AND (deleted_at IS NULL OR deleted_at = '') ORDER BY submitted_at DESC LIMIT 1"
    )
    .bind(&candidate_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            candidate_id = %candidate_id,
            "Database error fetching resume info"
        );
        ApiError::DatabaseError(e)
    })?;

    let (resume_status, resume_id, resume_filename) = if let Some((id, filename)) = resume_info {
        ("uploaded".to_string(), Some(id), Some(filename))
    } else {
        ("not_uploaded".to_string(), None, None)
    };

    let last_activity = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT MAX(datetime) as last_activity FROM (
            SELECT applied_at as datetime FROM applications WHERE user_id = ?
            UNION ALL
            SELECT updated_at as datetime FROM profiles WHERE user_id = ?
            UNION ALL
            SELECT submitted_at as datetime FROM resumes WHERE user_id = ? AND (deleted_at IS NULL OR deleted_at = '')
        )
        "#
    )
    .bind(&candidate_id)
    .bind(&candidate_id)
    .bind(&candidate_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            candidate_id = %candidate_id,
            "Database error fetching candidate last activity"
        );
        ApiError::DatabaseError(e)
    })?
    .flatten();

    let total_applications = applications.len() as i64;
    let successful_applications = applications
        .iter()
        .filter(|app| matches!(app.status.as_str(), "hired" | "offered"))
        .count() as i64;

    let application_success_rate = if total_applications > 0 {
        successful_applications as f64 / total_applications as f64
    } else {
        0.0
    };

    let candidate = CandidateProfile {
        user,
        profile,
        experiences,
        education,
        applications,
        resume_status,
        resume_id,
        resume_filename,
        last_activity,
        total_applications,
        application_success_rate,
    };

    info!(
        admin_user_id = %authed.id,
        candidate_id = %candidate_id,
        total_applications = total_applications,
        application_success_rate = application_success_rate,
        "Candidate details fetched successfully"
    );

    Ok(Json(candidate))
}
