// src/candidates/handlers/files.rs
//! File serving for candidate resumes and assets

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::common::{ApiError, AppState};

/// GET /uploads/resumes/*path - Serve resume files (PDFs and derived images)
pub async fn serve_resume_file(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path(path): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    let file_path = state.resumes_dir.join(&path);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("File not found".to_string()));
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read file".to_string()))?;

    let content_type = if path.ends_with(".pdf") {
        "application/pdf"
    } else if path.ends_with(".png") {
        "image/png"
    } else {
        "application/octet-stream"
    };

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        content,
    ))
}
