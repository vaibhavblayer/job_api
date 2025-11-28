// src/jobs/handlers/images.rs
//! Job image management

use axum::{
    extract::{Extension, Multipart, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::auth::AuthedUser;
use crate::common::{generate_raw_id, ApiError, AppState};

/// POST /api/admin/jobs/upload-image - Upload job image or company logo (admin only)
pub async fn upload_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let mut image_type = String::new();
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        match field.name() {
            Some("type") => {
                image_type = field.text().await.unwrap_or_default();
            }
            Some("image") => {
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| ApiError::BadRequest("Invalid file".to_string()))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let data = file_data.ok_or_else(|| ApiError::BadRequest("No image provided".to_string()))?;

    if !is_valid_image_type(&data) {
        return Err(ApiError::BadRequest("Invalid image type".to_string()));
    }

    let filename = format!("{}.png", generate_raw_id(8));
    let dir = if image_type == "logo" {
        &state.job_images_logos_dir
    } else {
        &state.job_images_jobs_dir
    };

    let file_path = dir.join(&filename);
    tokio::fs::write(&file_path, &data)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to save image".to_string()))?;

    let url = format!("/api/job-images/{}/{}", image_type, filename);

    info!(
        admin_id = %authed.id,
        image_type = %image_type,
        filename = %filename,
        "Job image uploaded"
    );

    Ok((
        StatusCode::OK,
        Json(json!({
            "url": url,
            "message": "Image uploaded successfully"
        })),
    ))
}

/// GET /api/job-images/:type/:filename - Serve job images
pub async fn serve_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path((img_type, filename)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    let dir = if img_type == "logos" {
        &state.job_images_logos_dir
    } else {
        &state.job_images_jobs_dir
    };

    let file_path = dir.join(&filename);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("Image not found".to_string()));
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read image".to_string()))?;

    let content_type = get_content_type_from_extension(&filename);

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        content,
    ))
}

/// DELETE /api/admin/jobs/images/:filename - Delete a job image (admin only)
pub async fn delete_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Try both directories
    let logo_path = state.job_images_logos_dir.join(&filename);
    let job_path = state.job_images_jobs_dir.join(&filename);

    if logo_path.exists() {
        tokio::fs::remove_file(&logo_path)
            .await
            .map_err(|_| ApiError::InternalServer("Failed to delete image".to_string()))?;
    } else if job_path.exists() {
        tokio::fs::remove_file(&job_path)
            .await
            .map_err(|_| ApiError::InternalServer("Failed to delete image".to_string()))?;
    } else {
        return Err(ApiError::BadRequest("Image not found".to_string()));
    }

    info!(
        admin_id = %authed.id,
        filename = %filename,
        "Job image deleted"
    );

    Ok(Json(json!({
        "message": "Image deleted successfully"
    })))
}

// Helper functions

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
