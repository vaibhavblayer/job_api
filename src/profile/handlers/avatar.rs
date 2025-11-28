// src/profile/handlers/avatar.rs

use axum::{
    extract::{Extension, Json, Multipart, Path},
    http::StatusCode,
    response::IntoResponse,
};
use infer::Infer;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::fs as tokio_fs;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::super::models::{AvatarUpdateRequest, AvatarUploadResponse, MessageResponse};
use crate::auth::{AuthedUser, User};
use crate::common::{generate_raw_id, ApiError, AppState};

/// POST /api/user/avatar - Upload avatar
pub async fn upload_avatar(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<Json<AvatarUploadResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    info!(user_id = %authed.id, "Avatar upload initiated");

    // File size limit: 5MB
    const MAX_FILE_SIZE: usize = 5 * 1024 * 1024;

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("avatar") {
            let filename = field
                .file_name()
                .ok_or_else(|| ApiError::BadRequest("No filename provided".to_string()))?
                .to_string();

            let data = field
                .bytes()
                .await
                .map_err(|_| ApiError::BadRequest("Failed to read file data".to_string()))?;

            // Validate file size
            if data.len() > MAX_FILE_SIZE {
                return Err(ApiError::BadRequest(
                    "File size exceeds 5MB limit".to_string(),
                ));
            }

            // Validate file type
            if !is_valid_image_type(&data) {
                return Err(ApiError::BadRequest(
                    "Invalid image type. Only JPEG, PNG, GIF, and WebP are supported".to_string(),
                ));
            }

            // Save avatar and update user
            let avatar_url = save_avatar_file(&state, &authed.id, &data, &filename).await?;
            update_user_avatar(&state.db, &authed.id, &avatar_url).await?;

            info!(user_id = %authed.id, avatar_url = %avatar_url, "Avatar uploaded successfully");

            return Ok(Json(AvatarUploadResponse {
                avatar_url,
                message: "Avatar uploaded successfully".to_string(),
            }));
        }
    }

    Err(ApiError::BadRequest("No avatar file found".to_string()))
}

/// PUT /api/user/avatar - Update avatar URL
pub async fn update_avatar_url(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<AvatarUpdateRequest>,
) -> Result<Json<AvatarUploadResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    let avatar_url = request
        .avatar_url
        .ok_or_else(|| ApiError::BadRequest("No avatar URL provided".to_string()))?;

    info!(user_id = %authed.id, external_url = %avatar_url, "Downloading external avatar");

    // Download and store the external avatar
    let local_url = download_and_store_avatar(&state, &authed.id, &avatar_url).await?;
    update_user_avatar(&state.db, &authed.id, &local_url).await?;

    Ok(Json(AvatarUploadResponse {
        avatar_url: local_url,
        message: "Avatar updated successfully".to_string(),
    }))
}

/// DELETE /api/user/avatar - Remove avatar
pub async fn remove_avatar(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<MessageResponse>, ApiError> {
    let state = state_lock.read().await.clone();

    // Get current avatar info
    if let Ok(Some(user)) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
    {
        // Delete file if it's a local avatar
        if let Some(avatar_url) = &user.avatar {
            if avatar_url.starts_with("/api/avatars/") {
                let filename = avatar_url.replace("/api/avatars/", "");
                let file_path = state.avatars_dir.join(&filename);
                if file_path.exists() {
                    let _ = tokio_fs::remove_file(&file_path).await;
                }
            }
        }
    }

    // Clear avatar from database
    sqlx::query("UPDATE users SET avatar = NULL, avatar_filename = NULL, avatar_updated_at = datetime('now') WHERE id = ?")
        .bind(&authed.id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(user_id = %authed.id, "Avatar removed successfully");

    Ok(Json(MessageResponse {
        message: "Avatar removed successfully".to_string(),
    }))
}

/// GET /api/avatars/:filename - Serve avatar files
pub async fn serve_avatar(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await.clone();

    // Sanitize filename to prevent path traversal
    let safe_filename = sanitize_filename(&filename);
    let file_path = state.avatars_dir.join(&safe_filename);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("Avatar not found".to_string()));
    }

    let file_content = tokio_fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read avatar file".to_string()))?;

    let content_type = get_content_type_from_extension(&safe_filename);

    Ok((
        StatusCode::OK,
        [
            ("Content-Type", content_type),
            ("Cache-Control", "public, max-age=31536000"), // 1 year cache
        ],
        file_content,
    ))
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn download_and_store_avatar(
    state: &AppState,
    user_id: &str,
    external_url: &str,
) -> Result<String, ApiError> {
    info!(user_id = %user_id, external_url = %external_url, "Downloading avatar from external URL");

    // Download image from external URL
    let response = state.http.get(external_url).send().await.map_err(|e| {
        error!(error = %e, external_url = %external_url, "Failed to download avatar");
        ApiError::InternalServer("Failed to download avatar".to_string())
    })?;

    if !response.status().is_success() {
        return Err(ApiError::BadRequest(
            "Failed to download avatar from URL".to_string(),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read avatar data".to_string()))?;

    // Validate file type
    if !is_valid_image_type(&bytes) {
        return Err(ApiError::BadRequest(
            "Downloaded file is not a valid image".to_string(),
        ));
    }

    // Generate filename and save
    let extension = get_extension_from_url(external_url).unwrap_or("jpg");
    let filename = format!("avatar_{}_{}.{}", user_id, generate_raw_id(8), extension);

    save_avatar_file(state, user_id, &bytes, &filename).await
}

async fn save_avatar_file(
    state: &AppState,
    user_id: &str,
    data: &[u8],
    original_filename: &str,
) -> Result<String, ApiError> {
    // Generate safe filename
    let extension = get_extension_from_filename(original_filename).unwrap_or("jpg");
    let filename = format!("avatar_{}_{}.{}", user_id, generate_raw_id(8), extension);
    let file_path = state.avatars_dir.join(&filename);

    // Save file
    tokio_fs::write(&file_path, data).await.map_err(|e| {
        error!(error = %e, file_path = %file_path.display(), "Failed to save avatar file");
        ApiError::InternalServer("Failed to save avatar file".to_string())
    })?;

    // Generate public URL - use relative URL (frontend will prepend API base)
    let avatar_url = format!("/api/avatars/{}", filename);

    info!(user_id = %user_id, filename = %filename, "Avatar file saved successfully");

    Ok(avatar_url)
}

async fn update_user_avatar(
    pool: &SqlitePool,
    user_id: &str,
    avatar_url: &str,
) -> Result<(), ApiError> {
    let filename = avatar_url.replace("/api/avatars/", "");

    sqlx::query(
        "UPDATE users SET avatar = ?, avatar_filename = ?, avatar_updated_at = datetime('now') WHERE id = ?"
    )
    .bind(avatar_url)
    .bind(&filename)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(())
}

fn is_valid_image_type(data: &[u8]) -> bool {
    let infer = Infer::new();
    if let Some(info) = infer.get(data) {
        matches!(
            info.mime_type(),
            "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/webp"
        )
    } else {
        false
    }
}

fn get_content_type_from_extension(filename: &str) -> &'static str {
    match filename.split('.').last() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "image/jpeg",
    }
}

fn get_extension_from_url(url: &str) -> Option<&str> {
    url.split('?')
        .next()? // Remove query parameters
        .split('.')
        .last()
        .filter(|ext| matches!(*ext, "jpg" | "jpeg" | "png" | "gif" | "webp"))
}

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    filename
        .split('.')
        .last()
        .filter(|ext| matches!(*ext, "jpg" | "jpeg" | "png" | "gif" | "webp"))
}

fn sanitize_filename(filename: &str) -> String {
    // Remove path traversal sequences and directory separators
    let cleaned = filename
        .replace("..", "")
        .replace("/", "")
        .replace("\\", "")
        .replace("\0", ""); // Remove null bytes

    // Whitelist safe characters: alphanumeric, dots, hyphens, underscores
    let sanitized: String = cleaned
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();

    // Limit filename length to prevent buffer overflow attacks
    let max_length = 255;
    let truncated = if sanitized.len() > max_length {
        sanitized.chars().take(max_length).collect()
    } else {
        sanitized
    };

    // Ensure we don't end up with an empty filename
    if truncated.is_empty() {
        "sanitized_file".to_string()
    } else {
        truncated
    }
}
