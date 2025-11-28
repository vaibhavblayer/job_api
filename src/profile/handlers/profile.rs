// src/profile/handlers/profile.rs

use axum::extract::{Extension, Json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::super::models::{Profile, UpdateProfileRequest};
use crate::auth::{AuthedUser, User};
use crate::candidates::models::Resume;
use crate::common::{ApiError, AppState};

/// GET /api/profile - Get user profile
pub async fn profile_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE user_id = ?")
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let latest_resume = match sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE user_id = ? ORDER BY submitted_at DESC LIMIT 1",
    )
    .bind(&authed.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                error = %e,
                user_id = %authed.id,
                "Database error loading latest resume for user"
            );
            None
        }
    };

    let resume_status = profile
        .as_ref()
        .and_then(|p| p.resume_status.clone())
        .or_else(|| latest_resume.as_ref().map(|r| r.status.clone()))
        .unwrap_or_else(|| "pending".to_string());

    // Fetch full user data for response
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&authed.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let response = serde_json::json!({
        "user": user,
        "is_admin": authed.is_admin,
        "resume_status": resume_status,
        "profile": profile,
        "latest_resume": latest_resume,
    });

    Ok(Json(response))
}

/// PUT /api/profile - Update user profile
pub async fn update_profile_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<Profile>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(user_id = %authed.id, "Profile update request received");

    // Convert skills array to JSON string if provided
    let skills_json = request
        .skills
        .as_ref()
        .map(|skills| serde_json::to_string(skills).unwrap_or_else(|_| "[]".to_string()));

    // Update or insert profile
    sqlx::query(
        r#"
        INSERT INTO profiles (
            user_id, first_name, last_name, phone, location, bio, 
            website, linkedin_url, github_url, skills, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
        ON CONFLICT(user_id) DO UPDATE SET
            first_name = COALESCE(excluded.first_name, first_name),
            last_name = COALESCE(excluded.last_name, last_name),
            phone = COALESCE(excluded.phone, phone),
            location = COALESCE(excluded.location, location),
            bio = COALESCE(excluded.bio, bio),
            website = COALESCE(excluded.website, website),
            linkedin_url = COALESCE(excluded.linkedin_url, linkedin_url),
            github_url = COALESCE(excluded.github_url, github_url),
            skills = COALESCE(excluded.skills, skills),
            updated_at = datetime('now')
        "#,
    )
    .bind(&authed.id)
    .bind(request.first_name.as_deref())
    .bind(request.last_name.as_deref())
    .bind(request.phone.as_deref())
    .bind(request.location.as_deref())
    .bind(request.bio.as_deref())
    .bind(request.website.as_deref())
    .bind(request.linkedin_url.as_deref())
    .bind(request.github_url.as_deref())
    .bind(skills_json.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            "Database error updating profile"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the updated profile
    let profile = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE user_id = ?")
        .bind(&authed.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %authed.id,
                "Database error fetching updated profile"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(user_id = %authed.id, "Profile updated successfully");

    Ok(Json(profile))
}
