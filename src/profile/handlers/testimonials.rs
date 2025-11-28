// src/profile/handlers/testimonials.rs

use axum::{
    extract::{Extension, Json, Path},
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use super::super::models::{
    CreateTestimonialRequest, Testimonial, TestimonialWithUser, UpdateTestimonialRequest,
};
use crate::auth::{AuthedUser, User};
use crate::common::{generate_testimonial_id, ApiError, AppState};

/// GET /api/testimonials - Get approved and featured testimonials (public)
pub async fn get_public_testimonials(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
) -> Result<Json<Vec<TestimonialWithUser>>, ApiError> {
    let state = state_lock.read().await.clone();

    let testimonials: Vec<Testimonial> = sqlx::query_as(
        "SELECT * FROM testimonials WHERE approved = 1 AND featured = 1 ORDER BY created_at DESC LIMIT 10"
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let mut result = Vec::new();
    for testimonial in testimonials {
        let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(&testimonial.user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        if let Some(user) = user {
            result.push(TestimonialWithUser {
                id: testimonial.id,
                user_id: testimonial.user_id,
                user_name: user.name.unwrap_or_else(|| user.email.clone()),
                user_email: user.email,
                user_avatar: user.avatar,
                content: testimonial.content,
                rating: testimonial.rating,
                position: testimonial.position,
                company: testimonial.company,
                featured: testimonial.featured != 0,
                approved: testimonial.approved != 0,
                created_at: testimonial.created_at,
                updated_at: testimonial.updated_at,
            });
        }
    }

    Ok(Json(result))
}

/// POST /api/testimonials - Create a new testimonial (authenticated users)
pub async fn create_testimonial(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(req): Json<CreateTestimonialRequest>,
) -> Result<Json<Testimonial>, ApiError> {
    let state = state_lock.read().await.clone();
    let now = chrono::Utc::now().to_rfc3339();
    let id = generate_testimonial_id();

    let testimonial = Testimonial {
        id: id.clone(),
        user_id: authed.id.clone(),
        content: req.content,
        rating: req.rating,
        position: None,
        company: None,
        featured: 0,
        approved: 0, // Requires admin approval
        created_at: Some(now.clone()),
        updated_at: Some(now),
    };

    sqlx::query(
        r#"
        INSERT INTO testimonials (id, user_id, content, rating, featured, approved, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&testimonial.id)
    .bind(&testimonial.user_id)
    .bind(&testimonial.content)
    .bind(testimonial.rating)
    .bind(testimonial.featured)
    .bind(testimonial.approved)
    .bind(&testimonial.created_at)
    .bind(&testimonial.updated_at)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(testimonial_id = %id, user_id = %authed.id, "Testimonial created");

    Ok(Json(testimonial))
}

/// PUT /api/testimonials/:id - Update testimonial (owner or admin)
pub async fn update_testimonial(
    Path(id): Path<String>,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(req): Json<UpdateTestimonialRequest>,
) -> Result<Json<Testimonial>, ApiError> {
    let state = state_lock.read().await.clone();

    let existing: Option<Testimonial> = sqlx::query_as("SELECT * FROM testimonials WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let existing =
        existing.ok_or_else(|| ApiError::BadRequest("testimonial not found".to_string()))?;

    // Check authorization
    if !authed.is_admin && existing.user_id != authed.id {
        return Err(ApiError::Forbidden("access denied".to_string()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut updates = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(content) = req.content {
        updates.push("content = ?");
        values.push(content);
    }

    if let Some(rating) = req.rating {
        updates.push("rating = ?");
        values.push(rating.to_string());
    }

    if let Some(position) = req.position {
        updates.push("position = ?");
        values.push(position);
    }

    if let Some(company) = req.company {
        updates.push("company = ?");
        values.push(company);
    }

    // Only admins can update featured/approved status
    if authed.is_admin {
        if let Some(featured) = req.featured {
            updates.push("featured = ?");
            values.push(if featured { "1" } else { "0" }.to_string());
        }

        if let Some(approved) = req.approved {
            updates.push("approved = ?");
            values.push(if approved { "1" } else { "0" }.to_string());
        }
    }

    if !updates.is_empty() {
        updates.push("updated_at = ?");
        values.push(now.clone());
        values.push(id.clone());

        let query = format!(
            "UPDATE testimonials SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query);
        for value in values {
            q = q.bind(value);
        }
        q.execute(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;
    }

    let updated: Testimonial = sqlx::query_as("SELECT * FROM testimonials WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(testimonial_id = %id, "Testimonial updated");

    Ok(Json(updated))
}

/// DELETE /api/testimonials/:id - Delete testimonial (owner or admin)
pub async fn delete_testimonial(
    Path(id): Path<String>,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await.clone();

    let existing: Option<Testimonial> = sqlx::query_as("SELECT * FROM testimonials WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let existing =
        existing.ok_or_else(|| ApiError::BadRequest("testimonial not found".to_string()))?;

    // Check authorization
    if !authed.is_admin && existing.user_id != authed.id {
        return Err(ApiError::Forbidden("access denied".to_string()));
    }

    sqlx::query("DELETE FROM testimonials WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(testimonial_id = %id, "Testimonial deleted");

    Ok(Json(serde_json::json!({"message": "testimonial deleted"})))
}

/// GET /api/admin/testimonials - Get all testimonials (admin only)
pub async fn get_all_testimonials(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<TestimonialWithUser>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin access required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let testimonials: Vec<Testimonial> =
        sqlx::query_as("SELECT * FROM testimonials ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

    let mut result = Vec::new();
    for testimonial in testimonials {
        let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(&testimonial.user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        if let Some(user) = user {
            result.push(TestimonialWithUser {
                id: testimonial.id,
                user_id: testimonial.user_id,
                user_name: user.name.unwrap_or_else(|| user.email.clone()),
                user_email: user.email,
                user_avatar: user.avatar,
                content: testimonial.content,
                rating: testimonial.rating,
                position: testimonial.position,
                company: testimonial.company,
                featured: testimonial.featured != 0,
                approved: testimonial.approved != 0,
                created_at: testimonial.created_at,
                updated_at: testimonial.updated_at,
            });
        }
    }

    Ok(Json(result))
}

/// POST /api/admin/testimonials/:id/approve - Approve/unapprove testimonial (admin only)
pub async fn approve_testimonial(
    Path(id): Path<String>,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin access required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let approved = payload
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE testimonials SET approved = ?, updated_at = ? WHERE id = ?")
        .bind(approved)
        .bind(&now)
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(testimonial_id = %id, approved = approved, "Testimonial approval status updated");

    Ok(Json(
        serde_json::json!({"message": "testimonial approval updated", "approved": approved}),
    ))
}

/// POST /api/admin/testimonials/:id/feature - Toggle featured status (admin only)
pub async fn toggle_feature_testimonial(
    Path(id): Path<String>,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin access required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let featured = payload
        .get("featured")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE testimonials SET featured = ?, updated_at = ? WHERE id = ?")
        .bind(featured)
        .bind(&now)
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(testimonial_id = %id, featured = featured, "Testimonial featured status updated");

    Ok(Json(
        serde_json::json!({"message": "testimonial featured status updated", "featured": featured}),
    ))
}

/// GET /api/testimonials/my - Get current user's testimonials
pub async fn get_my_testimonials(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<Testimonial>>, ApiError> {
    let state = state_lock.read().await.clone();

    let testimonials: Vec<Testimonial> =
        sqlx::query_as("SELECT * FROM testimonials WHERE user_id = ? ORDER BY created_at DESC")
            .bind(&authed.id)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

    Ok(Json(testimonials))
}

/// GET /api/admin/candidates/:id/testimonials - Get testimonials for a specific candidate (admin only)
pub async fn get_candidate_testimonials(
    Path(id): Path<String>,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<Testimonial>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin access required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let testimonials: Vec<Testimonial> =
        sqlx::query_as("SELECT * FROM testimonials WHERE user_id = ? ORDER BY created_at DESC")
            .bind(&id)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

    Ok(Json(testimonials))
}
