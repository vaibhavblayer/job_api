use super::models::{
    CreateCompanyRequest, MessageResponse, SaveUrlAsAssetRequest, UpdateCompanyRequest,
};
use super::services::CompaniesService;
use super::validators;
use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};
use axum::{
    extract::{Extension, Multipart, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::common::generate_raw_id;

// ============================================================================
// Company CRUD Handlers
// ============================================================================

/// GET /api/admin/companies - Get all companies
pub async fn get_companies(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let companies = companies_service.get_all_companies().await?;

    Ok(Json(companies))
}

/// POST /api/admin/companies - Create a new company
pub async fn create_company(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Json(request): Json<CreateCompanyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let company = companies_service.create_company(request).await?;

    Ok((StatusCode::CREATED, Json(company)))
}

/// GET /api/admin/companies/:id - Get company by ID
pub async fn get_company_by_id(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let company = companies_service.get_company_by_id(&company_id).await?;

    Ok(Json(company))
}

/// PUT /api/admin/companies/:id - Update company
pub async fn update_company(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
    Json(request): Json<UpdateCompanyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let company = companies_service
        .update_company(&company_id, request)
        .await?;

    Ok(Json(company))
}

/// DELETE /api/admin/companies/:id - Delete company
pub async fn delete_company(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    companies_service.delete_company(&company_id).await?;

    Ok(Json(MessageResponse {
        message: "Company deleted successfully".to_string(),
    }))
}

// ============================================================================
// Company Asset Handlers
// ============================================================================

/// GET /api/admin/companies/:id/assets - Get all assets for a company
pub async fn get_company_assets(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let assets = companies_service.get_company_assets(&company_id).await?;

    Ok(Json(assets))
}

/// POST /api/admin/companies/:id/assets - Upload a new asset for a company
pub async fn upload_company_asset(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    // Verify company exists
    companies_service.get_company_by_id(&company_id).await?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut asset_type: Option<String> = None;
    let mut is_default = false;

    // Parse multipart form data
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                filename = field
                    .file_name()
                    .map(|s| s.to_string())
                    .or_else(|| Some("upload".to_string()));
                content_type = field.content_type().map(|s| s.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Failed to read file: {}", e)))?
                        .to_vec(),
                );
            }
            "asset_type" => {
                let value = field.text().await.map_err(|e| {
                    ApiError::BadRequest(format!("Failed to read asset_type: {}", e))
                })?;
                asset_type = Some(value);
            }
            "is_default" => {
                let value = field.text().await.map_err(|e| {
                    ApiError::BadRequest(format!("Failed to read is_default: {}", e))
                })?;
                is_default = value == "true" || value == "1";
            }
            _ => {}
        }
    }

    let file_data =
        file_data.ok_or_else(|| ApiError::BadRequest("No file provided".to_string()))?;
    let filename =
        filename.ok_or_else(|| ApiError::BadRequest("No filename provided".to_string()))?;
    let asset_type =
        asset_type.ok_or_else(|| ApiError::BadRequest("No asset_type provided".to_string()))?;

    // Validate asset type
    validators::validate_asset_type(&asset_type).map_err(ApiError::ValidationError)?;

    // Determine content type
    let mime_type = content_type.unwrap_or_else(|| {
        infer::get(&file_data)
            .map(|t| t.mime_type().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    });

    // Validate file type (images only)
    validators::validate_image_mime_type(&mime_type).map_err(ApiError::ValidationError)?;

    // Generate unique filename
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let unique_filename = format!("{}_{}.{}", asset_type, generate_raw_id(8), extension);

    // Determine storage directory based on asset type
    let storage_dir = if asset_type == "logo" {
        &app_state.job_images_logos_dir
    } else {
        &app_state.job_images_jobs_dir
    };

    let file_path = storage_dir.join(&unique_filename);

    // Save file to disk
    tokio::fs::write(&file_path, &file_data)
        .await
        .map_err(|e| {
            error!("Failed to write file: {}", e);
            ApiError::InternalServer("Failed to save file".to_string())
        })?;

    // Generate URL (use plural form for consistency with serve route)
    let url_type = if asset_type == "logo" { "logos" } else { "jobs" };
    let url = format!("/api/job-images/{}/{}", url_type, unique_filename);

    // Create asset record in database
    let asset = companies_service
        .create_company_asset(
            &company_id,
            &asset_type,
            url,
            unique_filename,
            file_data.len() as i64,
            mime_type,
            is_default,
        )
        .await?;

    info!(
        "Uploaded company asset: {} for company {}",
        asset.id, company_id
    );

    Ok((StatusCode::CREATED, Json(asset)))
}

/// DELETE /api/admin/companies/:company_id/assets/:asset_id - Delete a company asset
pub async fn delete_company_asset(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((company_id, asset_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    // Get asset to determine file path
    let asset = companies_service.get_company_asset_by_id(&asset_id).await?;

    // Delete from database
    companies_service
        .delete_company_asset(&company_id, &asset_id)
        .await?;

    // Delete file from disk
    let storage_dir = if asset.asset_type == "logo" {
        &app_state.job_images_logos_dir
    } else {
        &app_state.job_images_jobs_dir
    };

    let file_path = storage_dir.join(&asset.filename);
    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await.ok();
    }

    Ok(Json(MessageResponse {
        message: "Asset deleted successfully".to_string(),
    }))
}

/// PATCH /api/admin/companies/:company_id/assets/:asset_id/set-default - Set asset as default
pub async fn set_default_asset(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path((company_id, asset_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    let asset = companies_service
        .set_default_asset(&company_id, &asset_id)
        .await?;

    Ok(Json(asset))
}

/// POST /api/admin/companies/:company_id/assets/save-url - Save an existing URL as a company asset
pub async fn save_url_as_company_asset(
    Extension(state): Extension<Arc<RwLock<AppState>>>,
    user: AuthedUser,
    Path(company_id): Path<String>,
    Json(request): Json<SaveUrlAsAssetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !user.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let app_state = state.read().await;
    let companies_service = CompaniesService::new(app_state.db.clone());

    // Verify company exists
    companies_service.get_company_by_id(&company_id).await?;

    // Validate asset_type
    validators::validate_asset_type(&request.asset_type).map_err(ApiError::ValidationError)?;

    // Extract filename from URL
    let filename = request
        .url
        .split('/')
        .last()
        .ok_or_else(|| ApiError::BadRequest("Invalid URL format".to_string()))?
        .to_string();

    // Determine file path to get file size
    let storage_dir = if request.asset_type == "logo" {
        &app_state.job_images_logos_dir
    } else {
        &app_state.job_images_jobs_dir
    };

    let file_path = storage_dir.join(&filename);

    // Get file size and mime type
    let (file_size, mime_type) = if file_path.exists() {
        let metadata = tokio::fs::metadata(&file_path).await.map_err(|e| {
            error!("Failed to read file metadata: {}", e);
            ApiError::InternalServer("Failed to read file metadata".to_string())
        })?;

        let file_data = tokio::fs::read(&file_path).await.map_err(|e| {
            error!("Failed to read file: {}", e);
            ApiError::InternalServer("Failed to read file".to_string())
        })?;

        let mime = infer::get(&file_data)
            .map(|t| t.mime_type().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        (metadata.len() as i64, mime)
    } else {
        // If file doesn't exist locally, use defaults
        (0, "image/jpeg".to_string())
    };

    let is_default = request.is_default.unwrap_or(false);

    // Create asset record in database
    let asset = companies_service
        .create_company_asset(
            &company_id,
            &request.asset_type,
            request.url,
            filename,
            file_size,
            mime_type,
            is_default,
        )
        .await?;

    info!(
        "Saved URL as company asset: {} for company {}",
        asset.id, company_id
    );

    Ok((StatusCode::CREATED, Json(asset)))
}
