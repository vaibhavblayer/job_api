// src/candidates/handlers/videos.rs

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
use crate::candidates::models::*;
use crate::common::{generate_video_id, ApiError, AppState};

/// POST /api/user/videos - Upload a video
pub async fn upload_video(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    info!(user_id = %authed.id, "User uploading video");

    // Check video limit (max 2 videos per user)
    const MAX_VIDEOS: i64 = 2;
    let video_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM videos WHERE user_id = ?"
    )
    .bind(&authed.id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    if video_count >= MAX_VIDEOS {
        return Err(ApiError::BadRequest(
            format!("Video limit reached. You can upload a maximum of {} videos. Please delete an existing video before uploading a new one.", MAX_VIDEOS)
        ));
    }

    // Extract video file from multipart
    let mut video_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "video" {
            filename = field.file_name().map(|s| s.to_string());
            mime_type = field.content_type().map(|s| s.to_string());
            video_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Failed to read video data: {}", e)))?
                    .to_vec(),
            );
        }
    }

    let video_data =
        video_data.ok_or_else(|| ApiError::BadRequest("No video file provided".to_string()))?;
    let filename =
        filename.ok_or_else(|| ApiError::BadRequest("No filename provided".to_string()))?;
    let mime_type =
        mime_type.ok_or_else(|| ApiError::BadRequest("No mime type provided".to_string()))?;

    // Validate file size (max 100MB)
    const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;
    if video_data.len() > MAX_FILE_SIZE {
        return Err(ApiError::BadRequest(
            format!("Video file too large. Maximum size is 100MB.")
        ));
    }

    // Validate mime type
    const SUPPORTED_FORMATS: &[&str] = &["video/mp4", "video/quicktime", "video/x-msvideo", "video/webm"];
    if !SUPPORTED_FORMATS.contains(&mime_type.as_str()) {
        return Err(ApiError::BadRequest(
            format!("Unsupported video format. Supported formats: MP4, MOV, AVI, WebM")
        ));
    }

    // Generate unique video ID and S3 key
    let video_id = generate_video_id();
    let extension = filename.split('.').last().unwrap_or("mp4");
    let s3_key = format!("videos/user-{}/{}.{}", authed.id, video_id, extension);

    // Upload to S3
    let s3_url = state
        .aws_service
        .upload_file(video_data.clone(), &s3_key, &mime_type)
        .await
        .map_err(|e| ApiError::ProcessingError(format!("Failed to upload to S3: {}", e)))?;

    let file_size = video_data.len() as i64;

    // Store in database
    sqlx::query(
        r#"
        INSERT INTO videos (id, user_id, s3_url, filename, file_size, duration_seconds, mime_type, uploaded_at)
        VALUES (?, ?, ?, ?, ?, 0, ?, datetime('now'))
        "#,
    )
    .bind(&video_id)
    .bind(&authed.id)
    .bind(&s3_url)
    .bind(&filename)
    .bind(file_size)
    .bind(&mime_type)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(video_id = %video_id, user_id = %authed.id, "Video uploaded successfully");

    Ok((StatusCode::CREATED, Json(json!({
        "id": video_id,
        "s3_url": s3_url,
        "filename": filename,
        "file_size": file_size,
        "duration_seconds": 0,
    }))))
}

/// GET /api/user/videos - List user's videos
pub async fn list_user_videos(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    let videos = sqlx::query_as::<_, Video>(
        "SELECT * FROM videos WHERE user_id = ? ORDER BY uploaded_at DESC",
    )
    .bind(&authed.id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(Json(videos))
}

/// GET /api/user/videos/:id - Get video details
pub async fn get_video(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    let video = sqlx::query_as::<_, Video>("SELECT * FROM videos WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Video not found".to_string()))?;

    Ok(Json(video))
}

/// DELETE /api/user/videos/:id - Delete a video
pub async fn delete_video(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    // Check if video exists and belongs to user
    let video = sqlx::query_as::<_, Video>("SELECT * FROM videos WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Video not found".to_string()))?;

    // Check if video is attached to any applications
    let application_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM applications WHERE video_id = ?"
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    if application_count > 0 {
        return Err(ApiError::BadRequest(
            format!("Cannot delete video. It is attached to {} application(s). Please remove it from those applications first.", application_count)
        ));
    }

    // Extract S3 key from URL (skip for YouTube videos)
    let s3_key = if let Some(url) = &video.s3_url {
        if let Some(key) = url.split(".amazonaws.com/").nth(1) {
            key.to_string()
        } else if let Some(key) = url.split("/").last() {
            format!("videos/user-{}/{}", authed.id, key)
        } else {
            return Err(ApiError::ProcessingError("Invalid S3 URL format".to_string()));
        }
    } else {
        // YouTube video - no S3 key needed
        String::new()
    };

    // Delete from S3 (only for uploaded videos)
    if !s3_key.is_empty() {
        state
            .aws_service
            .delete_file(&s3_key)
            .await
            .map_err(|e| ApiError::ProcessingError(format!("Failed to delete from S3: {}", e)))?;
    }

    // Delete from database
    sqlx::query("DELETE FROM videos WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(video_id = %id, user_id = %authed.id, "Video deleted");

    Ok(Json(json!({ "message": "Video deleted successfully" })))
}

/// GET /api/admin/videos/:id/download - Download video (admin only)
pub async fn download_video(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can download videos".to_string(),
        ));
    }

    let video = sqlx::query_as::<_, Video>("SELECT * FROM videos WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Video not found".to_string()))?;

    // Return the S3 URL for download
    Ok(Json(json!({
        "download_url": video.s3_url,
        "filename": video.filename
    })))
}
