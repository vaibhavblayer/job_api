// src/profile/handlers/education.rs

use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::super::models::{CreateEducationRequest, Education, UpdateEducationRequest};
use super::super::validators::EducationValidator;
use crate::auth::AuthedUser;
use crate::common::{generate_education_id, ApiError, AppState, Validator};

/// GET /api/profile/education - Get all education records for the authenticated user
pub async fn get_education(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<Education>>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(user_id = %authed.id, "Fetching user education records");

    let education = sqlx::query_as::<_, Education>(
        "SELECT * FROM education WHERE user_id = ? ORDER BY start_date DESC",
    )
    .bind(&authed.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            "Database error fetching user education records"
        );
        ApiError::DatabaseError(e)
    })?;

    debug!(
        user_id = %authed.id,
        education_count = education.len(),
        "Successfully fetched user education records"
    );

    Ok(Json(education))
}

/// POST /api/profile/education - Create a new education record for the authenticated user
pub async fn create_education(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateEducationRequest>,
) -> Result<Json<Education>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        institution = %request.institution,
        degree = %request.degree,
        "Creating new education record"
    );

    // Validate the request
    let validator = EducationValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            user_id = %authed.id,
            errors = ?validation_result.errors,
            "Education creation validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    let education_id = generate_education_id();

    // Insert the new education record
    sqlx::query(
        r#"
        INSERT INTO education (id, user_id, institution, degree, field_of_study, start_date, end_date, description, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        "#
    )
    .bind(&education_id)
    .bind(&authed.id)
    .bind(&request.institution)
    .bind(&request.degree)
    .bind(request.field_of_study.as_deref())
    .bind(&request.start_date)
    .bind(request.end_date.as_deref())
    .bind(request.description.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            education_id = %education_id,
            "Database error creating education record"
        );
        ApiError::DatabaseError(e)
    })?;

    // Fetch the created education record
    let education = sqlx::query_as::<_, Education>("SELECT * FROM education WHERE id = ?")
        .bind(&education_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                education_id = %education_id,
                "Database error fetching created education record"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        user_id = %authed.id,
        education_id = %education_id,
        institution = %request.institution,
        "Education record created successfully"
    );

    Ok(Json(education))
}

/// PUT /api/profile/education/:id - Update an existing education record
pub async fn update_education(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(education_id): Path<String>,
    Json(request): Json<UpdateEducationRequest>,
) -> Result<Json<Education>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        education_id = %education_id,
        "Updating education record"
    );

    // Validate the request
    let validator = EducationValidator;
    let validation_result = validator.validate(&request);
    if !validation_result.is_valid {
        warn!(
            user_id = %authed.id,
            education_id = %education_id,
            errors = ?validation_result.errors,
            "Education update validation failed"
        );
        return Err(ApiError::from(validation_result));
    }

    // Check if the education record exists and belongs to the user
    let existing_education =
        sqlx::query_as::<_, Education>("SELECT * FROM education WHERE id = ? AND user_id = ?")
            .bind(&education_id)
            .bind(&authed.id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    user_id = %authed.id,
                    education_id = %education_id,
                    "Database error checking education record ownership"
                );
                ApiError::DatabaseError(e)
            })?;

    if existing_education.is_none() {
        warn!(
            user_id = %authed.id,
            education_id = %education_id,
            "Education record not found or access denied"
        );
        return Err(ApiError::BadRequest(
            "Education record not found".to_string(),
        ));
    }

    // Update the education record
    let result = sqlx::query(
        r#"
        UPDATE education 
        SET institution = COALESCE(?, institution),
            degree = COALESCE(?, degree),
            field_of_study = COALESCE(?, field_of_study),
            start_date = COALESCE(?, start_date),
            end_date = COALESCE(?, end_date),
            description = COALESCE(?, description),
            updated_at = datetime('now')
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(request.institution.as_deref())
    .bind(request.degree.as_deref())
    .bind(request.field_of_study.as_deref())
    .bind(request.start_date.as_deref())
    .bind(request.end_date.as_deref())
    .bind(request.description.as_deref())
    .bind(&education_id)
    .bind(&authed.id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            education_id = %education_id,
            "Database error updating education record"
        );
        ApiError::DatabaseError(e)
    })?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest(
            "Education record not found".to_string(),
        ));
    }

    // Fetch the updated education record
    let education = sqlx::query_as::<_, Education>("SELECT * FROM education WHERE id = ?")
        .bind(&education_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                education_id = %education_id,
                "Database error fetching updated education record"
            );
            ApiError::DatabaseError(e)
        })?;

    info!(
        user_id = %authed.id,
        education_id = %education_id,
        "Education record updated successfully"
    );

    Ok(Json(education))
}

/// DELETE /api/profile/education/:id - Delete an education record
pub async fn delete_education(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(education_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let state = state_lock.read().await.clone();

    info!(
        user_id = %authed.id,
        education_id = %education_id,
        "Deleting education record"
    );

    // Delete the education record (only if it belongs to the user)
    let result = sqlx::query("DELETE FROM education WHERE id = ? AND user_id = ?")
        .bind(&education_id)
        .bind(&authed.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %authed.id,
                education_id = %education_id,
                "Database error deleting education record"
            );
            ApiError::DatabaseError(e)
        })?;

    if result.rows_affected() == 0 {
        warn!(
            user_id = %authed.id,
            education_id = %education_id,
            "Education record not found or access denied for deletion"
        );
        return Err(ApiError::BadRequest(
            "Education record not found".to_string(),
        ));
    }

    info!(
        user_id = %authed.id,
        education_id = %education_id,
        "Education record deleted successfully"
    );

    Ok(StatusCode::NO_CONTENT)
}
