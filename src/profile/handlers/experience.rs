// src/profile/handlers/experience.rs

use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::super::models::{CreateExperienceRequest, Experience, UpdateExperienceRequest};
use super::super::validators::ExperienceValidator;
use crate::auth::AuthedUser;
use crate::common::{generate_experience_id, ApiError, AppState, Validator};

/// GET /api/profile/experience - Get all experiences for the authenticated user
pub async fn get_experiences(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<Experience>>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(user_id = %authed.id, "Fetching user experiences");

    let experiences = sqlx::query_as::<_, Experience>(
        "SELECT * FROM experiences WHERE user_id = ? ORDER BY start_date DESC",
    )
    .bind(&authed.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            "Database error fetching user experiences"
        );
        ApiError::DatabaseError(e)
    })?;

    debug!(
        user_id = %authed.id,
        experience_count = experiences.len(),
        "Successfully fetched user experiences"
    );

    Ok(Json(experiences))
}

/// POST /api/profile/experience - Create a new experience for the authenticated user
pub async fn create_experience(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateExperienceRequest>,
) -> Result<Json<Experience>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        company = %request.company,
        title = %request.title,
        "Creating new experience"
    );

    // Validate the request
    let validator = ExperienceValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            user_id = %authed.id,
            errors = ?validation_result.errors,
            "Experience creation validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    let experience_id = generate_experience_id();

    // Insert the new experience
    sqlx::query(
        r#"
        INSERT INTO experiences (id, user_id, company, title, start_date, end_date, description, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        "#
    )
    .bind(&experience_id)
    .bind(&authed.id)
    .bind(&request.company)
    .bind(&request.title)
    .bind(&request.start_date)
    .bind(request.end_date.as_deref())
    .bind(request.description.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            experience_id = %experience_id,
            "Database error creating experience"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the created experience
    let experience = sqlx::query_as::<_, Experience>("SELECT * FROM experiences WHERE id = ?")
        .bind(&experience_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                experience_id = %experience_id,
                "Database error fetching created experience"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        user_id = %authed.id,
        experience_id = %experience_id,
        company = %request.company,
        "Experience created successfully"
    );

    Ok(Json(experience))
}

/// PUT /api/profile/experience/:id - Update an existing experience
pub async fn update_experience(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(experience_id): Path<String>,
    Json(request): Json<UpdateExperienceRequest>,
) -> Result<Json<Experience>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        experience_id = %experience_id,
        "Updating experience"
    );

    // Validate the request
    let validator = ExperienceValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            user_id = %authed.id,
            experience_id = %experience_id,
            errors = ?validation_result.errors,
            "Experience update validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    // Check if the experience exists and belongs to the user
    let existing_experience =
        sqlx::query_as::<_, Experience>("SELECT * FROM experiences WHERE id = ? AND user_id = ?")
            .bind(&experience_id)
            .bind(&authed.id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    user_id = %authed.id,
                    experience_id = %experience_id,
                    "Database error checking experience ownership"
                );
                ApiError::DatabaseError(e)
            })?;

    if existing_experience.is_none() {
        warn!(
            user_id = %authed.id,
            experience_id = %experience_id,
            "Experience not found or access denied"
        );
        return Err(ApiError::BadRequest("Experience not found".to_string()));
    }

    // Update the experience
    let result = sqlx::query(
        r#"
        UPDATE experiences 
        SET company = COALESCE(?, company),
            title = COALESCE(?, title),
            start_date = COALESCE(?, start_date),
            end_date = COALESCE(?, end_date),
            description = COALESCE(?, description),
            updated_at = datetime('now')
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(request.company.as_deref())
    .bind(request.title.as_deref())
    .bind(request.start_date.as_deref())
    .bind(request.end_date.as_deref())
    .bind(request.description.as_deref())
    .bind(&experience_id)
    .bind(&authed.id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            experience_id = %experience_id,
            "Database error updating experience"
        );
        ApiError::DatabaseError(e)
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Experience not found".to_string()));
    }

    // Fetch the updated experience
    let experience = sqlx::query_as::<_, Experience>("SELECT * FROM experiences WHERE id = ?")
        .bind(&experience_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                experience_id = %experience_id,
                "Database error fetching updated experience"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        user_id = %authed.id,
        experience_id = %experience_id,
        "Experience updated successfully"
    );

    Ok(Json(experience))
}

/// DELETE /api/profile/experience/:id - Delete an experience
pub async fn delete_experience(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(experience_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        experience_id = %experience_id,
        "Deleting experience"
    );

    // Delete the experience (only if it belongs to the user)
    let result = sqlx::query("DELETE FROM experiences WHERE id = ? AND user_id = ?")
        .bind(&experience_id)
        .bind(&authed.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %authed.id,
                experience_id = %experience_id,
                "Database error deleting experience"
            );
            ApiError::DatabaseError(e)
        })?;

    if result.rows_affected() == 0 {
        warn!(
            user_id = %authed.id,
            experience_id = %experience_id,
            "Experience not found or access denied for deletion"
        );
        return Err(ApiError::BadRequest("Experience not found".to_string()));
    }

    info!(
        user_id = %authed.id,
        experience_id = %experience_id,
        "Experience deleted successfully"
    );

    Ok(StatusCode::NO_CONTENT)
}
