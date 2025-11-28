// src/services/video.rs
use crate::candidates::models::{VideoMetadata, VideoSubmission, VideoUploadResponse};
use crate::services::aws::{AWSError, AWSService};
use sqlx::SqlitePool;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info};

use crate::common::generate_video_id;

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("Video file too large: {0} bytes (max 100MB)")]
    FileTooLarge(i64),

    #[error("Video duration invalid: {0} seconds (must be between 5-15 minutes)")]
    InvalidDuration(i32),

    #[error("Unsupported video format: {0}")]
    UnsupportedFormat(String),

    #[error("Video not found")]
    NotFound,

    #[error("AWS error: {0}")]
    AWSError(#[from] AWSError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Invalid video data: {0}")]
    InvalidData(String),
}

const MAX_FILE_SIZE: i64 = 100 * 1024 * 1024; // 100MB
const MIN_DURATION_SECONDS: i32 = 5 * 60; // 5 minutes
const MAX_DURATION_SECONDS: i32 = 15 * 60; // 15 minutes
const SUPPORTED_FORMATS: &[&str] = &[
    "video/mp4",
    "video/quicktime",
    "video/x-msvideo",
    "video/webm",
];

#[derive(Debug)]
pub struct VideoService {
    db: SqlitePool,
    aws_service: Arc<AWSService>,
}

impl VideoService {
    pub fn new(db: SqlitePool, aws_service: Arc<AWSService>) -> Self {
        Self { db, aws_service }
    }

    /// Validate video file before upload
    pub fn validate_video(
        &self,
        file_size: i64,
        duration_seconds: i32,
        mime_type: &str,
    ) -> Result<(), VideoError> {
        // Check file size
        if file_size > MAX_FILE_SIZE {
            return Err(VideoError::FileTooLarge(file_size));
        }

        // Check duration
        if duration_seconds < MIN_DURATION_SECONDS || duration_seconds > MAX_DURATION_SECONDS {
            return Err(VideoError::InvalidDuration(duration_seconds));
        }

        // Check format
        if !SUPPORTED_FORMATS.contains(&mime_type) {
            return Err(VideoError::UnsupportedFormat(mime_type.to_string()));
        }

        Ok(())
    }

    /// Upload video to S3 and store metadata in database
    pub async fn upload_video(
        &self,
        application_id: &str,
        file_data: Vec<u8>,
        filename: &str,
        mime_type: &str,
        duration_seconds: i32,
    ) -> Result<VideoUploadResponse, VideoError> {
        let file_size = file_data.len() as i64;

        // Validate video
        self.validate_video(file_size, duration_seconds, mime_type)?;

        // Generate unique filename for S3
        let video_id = generate_video_id();
        let extension = filename.split('.').last().unwrap_or("mp4");
        let s3_key = format!("videos/{}/{}.{}", application_id, video_id, extension);

        // Upload to S3
        info!(
            application_id = %application_id,
            filename = %filename,
            file_size = %file_size,
            "Uploading video to S3"
        );

        let s3_url = self
            .aws_service
            .upload_file(file_data, &s3_key, mime_type)
            .await?;

        // Store metadata in database
        sqlx::query(
            r#"
            INSERT INTO video_submissions (id, application_id, s3_url, filename, file_size, duration_seconds, mime_type)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&video_id)
        .bind(application_id)
        .bind(&s3_url)
        .bind(filename)
        .bind(file_size)
        .bind(duration_seconds)
        .bind(mime_type)
        .execute(&self.db)
        .await?;

        info!(
            video_id = %video_id,
            application_id = %application_id,
            "Video uploaded successfully"
        );

        Ok(VideoUploadResponse {
            id: video_id,
            s3_url,
            filename: filename.to_string(),
            file_size,
            duration_seconds,
        })
    }

    /// Get video submission by application ID
    pub async fn get_video_by_application(
        &self,
        application_id: &str,
    ) -> Result<Option<VideoSubmission>, VideoError> {
        let video = sqlx::query_as::<_, VideoSubmission>(
            r#"
            SELECT id, application_id, s3_url, filename, file_size, duration_seconds, mime_type, uploaded_at
            FROM video_submissions
            WHERE application_id = ?
            "#,
        )
        .bind(application_id)
        .fetch_optional(&self.db)
        .await?;

        Ok(video)
    }

    /// Get video submission by ID
    pub async fn get_video_by_id(&self, video_id: &str) -> Result<VideoSubmission, VideoError> {
        let video = sqlx::query_as::<_, VideoSubmission>(
            r#"
            SELECT id, application_id, s3_url, filename, file_size, duration_seconds, mime_type, uploaded_at
            FROM video_submissions
            WHERE id = ?
            "#,
        )
        .bind(video_id)
        .fetch_optional(&self.db)
        .await?
        .ok_or(VideoError::NotFound)?;

        Ok(video)
    }

    /// Generate signed URL for secure video streaming
    pub async fn generate_signed_url(
        &self,
        video_id: &str,
        _expiration_seconds: u64,
    ) -> Result<String, VideoError> {
        let video = self.get_video_by_id(video_id).await?;

        // For now, return the standard URL
        // In production, you would generate a presigned URL with expiration
        debug!(
            video_id = %video_id,
            "Generated signed URL (using standard URL for now)"
        );

        Ok(video.s3_url.unwrap_or_default())
    }

    /// Delete video submission
    pub async fn delete_video(&self, video_id: &str) -> Result<(), VideoError> {
        let video = self.get_video_by_id(video_id).await?;

        // Extract S3 key from URL (only for uploaded videos)
        if let Some(url) = &video.s3_url {
            let s3_key = url
                .split(".amazonaws.com/")
                .nth(1)
                .ok_or_else(|| VideoError::InvalidData("Invalid S3 URL format".to_string()))?;

            // Delete from S3
            self.aws_service.delete_file(s3_key).await?;
        }

        // Delete from database
        sqlx::query("DELETE FROM video_submissions WHERE id = ?")
            .bind(video_id)
            .execute(&self.db)
            .await?;

        info!(video_id = %video_id, "Video deleted successfully");

        Ok(())
    }

    /// Delete video by application ID
    pub async fn delete_video_by_application(
        &self,
        application_id: &str,
    ) -> Result<(), VideoError> {
        if let Some(video) = self.get_video_by_application(application_id).await? {
            self.delete_video(&video.id).await?;
        }
        Ok(())
    }

    /// Extract video metadata (simplified version)
    pub fn extract_metadata(
        &self,
        file_size: i64,
        duration_seconds: i32,
        mime_type: &str,
    ) -> VideoMetadata {
        let format = mime_type.split('/').nth(1).unwrap_or("unknown").to_string();

        VideoMetadata {
            duration_seconds,
            file_size,
            format,
            resolution: None, // Would require video processing library to extract
        }
    }

    /// Get video download URL (for admin)
    pub async fn get_download_url(&self, video_id: &str) -> Result<String, VideoError> {
        let video = self.get_video_by_id(video_id).await?;
        Ok(video.s3_url.unwrap_or_default())
    }
}
