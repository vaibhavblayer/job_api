// src/jobs/handlers/content_versions.rs
//! Handlers for job content version management (Inline AI Editor)

use axum::{
    extract::{Extension, Path},
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};
use crate::jobs::models::{
    ActivateVersionResponse, ContentVersionsResponse, DeleteVersionResponse,
    GenerateContentRequest, GenerateContentResponse,
};
use crate::jobs::services::ContentVersionsService;

// ============================================================================
// Get Content Versions
// ============================================================================

/// GET /api/admin/jobs/:job_id/content/:component_type/versions
/// Get all versions for a job component
pub async fn get_content_versions(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((job_id, component_type)): Path<(String, String)>,
) -> Result<Json<ContentVersionsResponse>, ApiError> {
    // Admin check
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let service = ContentVersionsService::new(
        app_state.db.clone(),
        app_state.openai_service.clone(),
    );

    let response = service.get_versions(&job_id, &component_type).await?;

    Ok(Json(response))
}

// ============================================================================
// Generate Content
// ============================================================================

/// POST /api/admin/jobs/:job_id/content/:component_type/generate
/// Generate new content using AI
pub async fn generate_content(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((job_id, component_type)): Path<(String, String)>,
    Json(request): Json<GenerateContentRequest>,
) -> Result<Json<GenerateContentResponse>, ApiError> {
    // Admin check
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    info!(
        job_id = %job_id,
        component_type = %component_type,
        user_id = %user.id,
        "Generating content with AI"
    );

    let app_state = state.read().await;
    let service = ContentVersionsService::new(
        app_state.db.clone(),
        app_state.openai_service.clone(),
    );

    let version = service
        .generate_content(
            &job_id,
            &component_type,
            request.prompt,
            request.tone,
            &user.id,
        )
        .await?;

    Ok(Json(GenerateContentResponse { version }))
}

// ============================================================================
// Activate Version
// ============================================================================

/// POST /api/admin/jobs/:job_id/content/:component_type/versions/:version_id/activate
/// Activate a specific version
pub async fn activate_version(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((job_id, component_type, version_id)): Path<(String, String, String)>,
) -> Result<Json<ActivateVersionResponse>, ApiError> {
    // Admin check
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    info!(
        job_id = %job_id,
        component_type = %component_type,
        version_id = %version_id,
        user_id = %user.id,
        "Activating content version"
    );

    let app_state = state.read().await;
    let service = ContentVersionsService::new(
        app_state.db.clone(),
        app_state.openai_service.clone(),
    );

    let version = service
        .activate_version(&job_id, &component_type, &version_id)
        .await?;

    Ok(Json(ActivateVersionResponse {
        success: true,
        version,
    }))
}

// ============================================================================
// Delete Version
// ============================================================================

/// DELETE /api/admin/jobs/:job_id/content/:component_type/versions/:version_id
/// Delete a specific version (cannot delete active version)
pub async fn delete_version(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((job_id, component_type, version_id)): Path<(String, String, String)>,
) -> Result<Json<DeleteVersionResponse>, ApiError> {
    // Admin check
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    info!(
        job_id = %job_id,
        component_type = %component_type,
        version_id = %version_id,
        user_id = %user.id,
        "Deleting content version"
    );

    let app_state = state.read().await;
    let service = ContentVersionsService::new(
        app_state.db.clone(),
        app_state.openai_service.clone(),
    );

    service
        .delete_version(&job_id, &component_type, &version_id)
        .await?;

    Ok(Json(DeleteVersionResponse { success: true }))
}
