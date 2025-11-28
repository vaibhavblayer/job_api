// src/jobs/handlers/templates.rs
//! Job template handlers

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};
use crate::jobs::models::{CreateAITemplateRequest, CreateJobTemplateRequest, UpdateJobTemplateRequest};
use crate::services::job_templates::JobTemplatesService;

#[derive(Debug, Deserialize)]
pub struct TemplateQueryParams {
    #[serde(rename = "type")]
    pub template_type: Option<String>,
    pub company_id: Option<String>,
}

/// GET /api/admin/job-templates - Get all templates or filter by type/company
pub async fn get_templates(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(params): Query<TemplateQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let templates = if let Some(company_id) = params.company_id {
        // If company_id is provided, get system templates + that company's templates
        service.get_available_templates(Some(&company_id)).await?
    } else if let Some(template_type) = params.template_type {
        // If type is provided, filter by type
        service.get_templates_by_type(&template_type).await?
    } else {
        // Otherwise get all templates
        service.get_all_templates().await?
    };

    Ok(Json(json!({
        "systemTemplates": templates.iter().filter(|t| t.template_type == "system").cloned().collect::<Vec<_>>(),
        "companyTemplates": templates.iter().filter(|t| t.template_type == "custom").cloned().collect::<Vec<_>>(),
        "aiTemplates": templates.iter().filter(|t| t.template_type == "ai").cloned().collect::<Vec<_>>(),
    })))
}

/// GET /api/admin/job-templates/available - Get available templates for a company
pub async fn get_available_templates(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(params): Query<TemplateQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let templates = service
        .get_available_templates(params.company_id.as_deref())
        .await?;

    Ok(Json(json!({
        "systemTemplates": templates.iter().filter(|t| t.template_type == "system").cloned().collect::<Vec<_>>(),
        "companyTemplates": templates.iter().filter(|t| t.template_type == "custom").cloned().collect::<Vec<_>>(),
        "aiTemplates": templates.iter().filter(|t| t.template_type == "ai").cloned().collect::<Vec<_>>(),
    })))
}

/// GET /api/admin/job-templates/:id - Get template by ID
pub async fn get_template_by_id(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(template_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let template = service.get_template_by_id(&template_id).await?;

    Ok(Json(template))
}

/// POST /api/admin/job-templates - Create a new custom template
pub async fn create_template(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateJobTemplateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let template = service.create_template(request, &authed.id).await?;

    info!(
        template_id = %template.id,
        template_name = %template.name,
        user_id = %authed.id,
        "Created job template"
    );

    Ok((StatusCode::CREATED, Json(template)))
}

/// PUT /api/admin/job-templates/:id - Update a custom template
pub async fn update_template(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(template_id): Path<String>,
    Json(request): Json<UpdateJobTemplateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let template = service
        .update_template(&template_id, request, &authed.id)
        .await?;

    info!(
        template_id = %template.id,
        user_id = %authed.id,
        "Updated job template"
    );

    Ok(Json(template))
}

/// DELETE /api/admin/job-templates/:id - Delete a custom template
pub async fn delete_template(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(template_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    service.delete_template(&template_id, &authed.id).await?;

    info!(
        template_id = %template_id,
        user_id = %authed.id,
        "Deleted job template"
    );

    Ok(Json(json!({
        "message": "Template deleted successfully"
    })))
}

// ============================================================================
// AI Template Handlers
// ============================================================================

/// POST /api/admin/job-templates/ai - Create a new AI template
pub async fn create_ai_template(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<CreateAITemplateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let template = service.create_ai_template(request, &authed.id).await?;

    info!(
        template_id = %template.id,
        template_name = %template.name,
        user_id = %authed.id,
        "Created AI job template"
    );

    Ok((StatusCode::CREATED, Json(template)))
}

/// GET /api/admin/job-templates/:id/ai-context - Get AI template context
pub async fn get_ai_template_context(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(template_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let context = service.get_ai_template_context(&template_id).await?;

    Ok(Json(context))
}

// ============================================================================
// Job Composer Template Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ComposerTemplateQueryParams {
    pub company_id: String,
}

/// GET /api/admin/job-templates/composer - Get templates for Job Composer (company-only, excludes system)
pub async fn get_job_composer_templates(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(params): Query<ComposerTemplateQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let service = JobTemplatesService::new(state.db.clone());

    let templates = service.get_job_composer_templates(&params.company_id).await?;

    // Group templates by type for frontend convenience
    let ai_templates: Vec<_> = templates.iter().filter(|t| t.template_type == "ai").cloned().collect();
    let custom_templates: Vec<_> = templates.iter().filter(|t| t.template_type == "custom").cloned().collect();

    Ok(Json(json!({
        "aiTemplates": ai_templates,
        "customTemplates": custom_templates,
        "total": templates.len()
    })))
}
