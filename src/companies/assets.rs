// src/companies/assets.rs
//! Company asset management (logos, images)

use axum::{
    extract::{Extension, Multipart, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::AuthedUser;
use crate::common::{generate_raw_id, ApiError, AppState};

/// POST /api/admin/logo/upload - Upload company logo (admin only)
pub async fn upload_logo(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("logo") {
            let data = field
                .bytes()
                .await
                .map_err(|_| ApiError::BadRequest("Invalid file".to_string()))?;

            if !is_valid_image_type(&data) {
                return Err(ApiError::BadRequest("Invalid image type".to_string()));
            }

            let (filename, s3_url) = save_logo_file(&state, &data).await?;
            
            // Use S3 URL if available, otherwise use local API path
            let logo_url = s3_url.unwrap_or_else(|| format!("/api/logos/{}", filename));

            update_logo_setting(&state.db, &logo_url).await?;

            return Ok((
                StatusCode::OK,
                Json(json!({
                    "logo_url": logo_url,
                    "message": "Logo uploaded successfully"
                })),
            ));
        }
    }

    Err(ApiError::BadRequest("No logo file provided".to_string()))
}

/// GET /api/logos/:filename - Serve logo files
pub async fn serve_logo(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    let file_path = state.logos_dir.join(&filename);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("Logo not found".to_string()));
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read logo".to_string()))?;

    let content_type = get_content_type_from_extension(&filename);

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        content,
    ))
}

/// GET /api/admin/logos - List all uploaded logos (admin only)
pub async fn list_logos(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Get the active logo URL from settings (only check 'company_logo')
    let active_logo_url: Option<String> = sqlx::query_scalar(
        "SELECT value FROM system_settings WHERE key = 'company_logo'"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let mut logos = Vec::new();
    
    // Check storage type
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    // If using S3, list from S3
    if storage_type.starts_with("s3") {
        match state.aws_service.list_files(Some("logos/")).await {
            Ok(s3_objects) => {
                for obj in s3_objects {
                    let filename = obj.key.strip_prefix("logos/").unwrap_or(&obj.key).to_string();
                    if filename.is_empty() {
                        continue;
                    }
                    
                    // Get the full URL for this file
                    let logo_url = match state.aws_service.get_file_url(&obj.key, true).await {
                        Ok(url) => url,
                        Err(_) => format!("/api/logos/{}", filename),
                    };
                    
                    let is_active = active_logo_url.as_ref().map(|url| url.contains(&filename)).unwrap_or(false);
                    let uploaded_at = obj.last_modified.map(|dt| dt.timestamp() as u64).unwrap_or(0);

                    logos.push(json!({
                        "filename": filename,
                        "url": logo_url,
                        "size": obj.size,
                        "is_active": is_active,
                        "uploaded_at": uploaded_at
                    }));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list S3 logos, falling back to local");
            }
        }
    }
    
    // Also list local logos (or as fallback)
    if let Ok(mut entries) = tokio::fs::read_dir(&state.logos_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(filename) = entry.file_name().to_str() {
                // Skip if already in list from S3
                if logos.iter().any(|l| l.get("filename").and_then(|f| f.as_str()) == Some(filename)) {
                    continue;
                }
                
                let file_path = entry.path();
                let metadata = tokio::fs::metadata(&file_path).await.ok();
                
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let uploaded_at = metadata
                    .as_ref()
                    .and_then(|m| m.created().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let logo_url = format!("/api/logos/{}", filename);
                let is_active = active_logo_url.as_ref().map(|url| url.contains(filename)).unwrap_or(false);

                logos.push(json!({
                    "filename": filename,
                    "url": logo_url,
                    "size": size,
                    "is_active": is_active,
                    "uploaded_at": uploaded_at
                }));
            }
        }
    }

    // Sort by uploaded_at descending (newest first)
    logos.sort_by(|a, b| {
        let a_time = a.get("uploaded_at").and_then(|v| v.as_u64()).unwrap_or(0);
        let b_time = b.get("uploaded_at").and_then(|v| v.as_u64()).unwrap_or(0);
        b_time.cmp(&a_time)
    });

    Ok(Json(logos))
}

/// POST /api/admin/logo/activate - Activate a logo
pub async fn activate_logo(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Accept either 'filename' or 'logo_url'
    let logo_url = if let Some(filename) = payload.get("filename").and_then(|v| v.as_str()) {
        format!("/api/logos/{}", filename)
    } else if let Some(url) = payload.get("logo_url").and_then(|v| v.as_str()) {
        url.to_string()
    } else {
        return Err(ApiError::BadRequest("filename or logo_url required".to_string()));
    };

    update_logo_setting(&state.db, &logo_url).await?;

    Ok(Json(json!({
        "message": "Logo activated successfully"
    })))
}

/// DELETE /api/admin/logo/:filename - Delete a specific logo file (admin only)
pub async fn delete_logo_file(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    
    // Check storage type
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let mut deleted = false;

    // Try to delete from S3 first if using S3 storage
    if storage_type.starts_with("s3") {
        let s3_key = format!("logos/{}", filename);
        match state.aws_service.delete_file(&s3_key).await {
            Ok(_) => {
                tracing::info!(s3_key = %s3_key, "Logo deleted from S3");
                deleted = true;
            }
            Err(e) => {
                tracing::warn!(error = %e, s3_key = %s3_key, "Failed to delete logo from S3, trying local");
            }
        }
    }

    // Also try to delete from local storage
    let file_path = state.logos_dir.join(&filename);
    if file_path.exists() {
        match tokio::fs::remove_file(&file_path).await {
            Ok(_) => {
                tracing::info!(path = ?file_path, "Logo deleted from local storage");
                deleted = true;
            }
            Err(e) => {
                tracing::warn!(error = %e, path = ?file_path, "Failed to delete logo from local storage");
            }
        }
    }

    if !deleted {
        return Err(ApiError::BadRequest("Logo not found in storage".to_string()));
    }

    Ok(Json(json!({
        "message": "Logo deleted successfully"
    })))
}

// Helper functions

async fn save_logo_file(state: &AppState, data: &[u8]) -> Result<(String, Option<String>), ApiError> {
    use tracing::{info, warn};
    
    let filename = format!("{}.png", generate_raw_id(8));

    // Check storage type setting
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    if storage_type.starts_with("s3") {
        // Upload to S3
        let s3_key = format!("logos/{}", filename);
        match state
            .aws_service
            .upload_file(data.to_vec(), &s3_key, "image/png")
            .await
        {
            Ok(s3_url) => {
                info!(s3_key = %s3_key, s3_url = %s3_url, "Logo uploaded to S3 successfully");
                // Return filename and S3 URL
                return Ok((filename, Some(s3_url)));
            }
            Err(e) => {
                warn!(error = %e, "Failed to upload logo to S3, falling back to local storage");
                // Fall through to local storage
            }
        }
    }

    // Save to local storage
    let file_path = state.logos_dir.join(&filename);
    tokio::fs::write(&file_path, data)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to save logo".to_string()))?;

    Ok((filename, None))
}

async fn update_logo_setting(db: &sqlx::SqlitePool, logo_url: &str) -> Result<(), ApiError> {
    // Use only 'company_logo' key for simplicity
    sqlx::query(
        "INSERT INTO system_settings (key, value) VALUES ('company_logo', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(logo_url)
    .execute(db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(())
}

fn is_valid_image_type(data: &[u8]) -> bool {
    let infer = infer::Infer::new();
    if let Some(info) = infer.get(data) {
        matches!(
            info.mime_type(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        )
    } else {
        false
    }
}

fn get_content_type_from_extension(filename: &str) -> &'static str {
    match filename.split('.').last() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}
