// src/admin/handlers/files.rs

use axum::{
    extract::{Extension, Path, Query},
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::fs as tokio_fs;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::admin::models::{
    FileItem, ListFilesQuery, ListFilesResponse, MessageResponse, StorageStats,
};
use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};

/// GET /api/admin/files - List files in storage
pub async fn list_files_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<ListFilesResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let storage_type = if let Some(st) = query.storage_type {
        st
    } else {
        state
            .settings_service
            .get_setting("storage_type")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "local".to_string())
    };

    let mut files = Vec::new();

    if storage_type == "s3" || storage_type == "s3-cloudfront" {
        match state.aws_service.list_files(query.prefix.as_deref()).await {
            Ok(s3_objects) => {
                for obj in s3_objects {
                    if let Some(search_term) = &query.search {
                        if !obj.key.to_lowercase().contains(&search_term.to_lowercase()) {
                            continue;
                        }
                    }

                    let file_name = obj.key.split('/').last().unwrap_or(&obj.key).to_string();
                    let file_type = file_name.split('.').last().unwrap_or("unknown").to_string();

                    let url = state
                        .aws_service
                        .get_file_url(&obj.key, storage_type == "s3-cloudfront")
                        .await
                        .unwrap_or_else(|_| format!("/{}", obj.key));

                    files.push(FileItem {
                        name: file_name,
                        path: obj.key.clone(),
                        size: obj.size,
                        file_type,
                        uploaded_at: obj.last_modified.map(|dt| dt.to_rfc3339()),
                        uploaded_by: None,
                        url,
                    });
                }
            }
            Err(e) => {
                error!(error = %e, storage_type = %storage_type, "Failed to list S3 files, falling back to local");
                // Fall back to local storage listing instead of returning error
                // This allows the file manager to work even if S3 is misconfigured
                info!("Falling back to local file listing due to S3 error");
            }
        }
    }
    
    // Always include local files (or as fallback when S3 fails)
    if storage_type == "local" || files.is_empty() {
        let base_dirs = vec![
            ("resumes", state.resumes_dir.clone()),
            ("avatars", state.avatars_dir.clone()),
            ("logos", state.logos_dir.clone()),
            (
                "job-images",
                state
                    .job_images_jobs_dir
                    .parent()
                    .unwrap_or(&state.job_images_jobs_dir)
                    .to_path_buf(),
            ),
        ];

        for (category, dir) in base_dirs {
            if let Some(prefix) = &query.prefix {
                if !category.starts_with(prefix) {
                    continue;
                }
            }

            if let Ok(entries) = tokio_fs::read_dir(&dir).await {
                let mut entries = entries;
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(metadata) = entry.metadata().await {
                        if metadata.is_file() {
                            let file_name = entry.file_name().to_string_lossy().to_string();

                            if let Some(search_term) = &query.search {
                                if !file_name
                                    .to_lowercase()
                                    .contains(&search_term.to_lowercase())
                                {
                                    continue;
                                }
                            }

                            let file_path = entry.path();
                            let relative_path = file_path
                                .strip_prefix(&dir)
                                .unwrap_or(&file_path)
                                .to_string_lossy()
                                .to_string();

                            let file_type =
                                file_name.split('.').last().unwrap_or("unknown").to_string();

                            let url = format!("/uploads/{}/{}", category, relative_path);

                            files.push(FileItem {
                                name: file_name,
                                path: format!("{}/{}", category, relative_path),
                                size: metadata.len() as i64,
                                file_type,
                                uploaded_at: metadata.modified().ok().map(|t| {
                                    let datetime: chrono::DateTime<Utc> = t.into();
                                    datetime.to_rfc3339()
                                }),
                                uploaded_by: None,
                                url,
                            });
                        }
                    }
                }
            }
        }
    }

    let total = files.len();

    info!(
        admin_user_id = %authed.id,
        storage_type = %storage_type,
        total_files = total,
        "Listed files"
    );

    Ok(Json(ListFilesResponse { files, total }))
}

/// DELETE /api/admin/files/:path - Delete a file
pub async fn delete_file_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(file_path): Path<String>,
) -> Result<Json<MessageResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    if storage_type == "s3" || storage_type == "s3-cloudfront" {
        match state.aws_service.delete_file(&file_path).await {
            Ok(_) => {
                info!(
                    admin_user_id = %authed.id,
                    file_path = %file_path,
                    "File deleted from S3"
                );
                Ok(Json(MessageResponse {
                    message: "File deleted successfully".to_string(),
                }))
            }
            Err(e) => {
                error!(
                    admin_user_id = %authed.id,
                    file_path = %file_path,
                    error = %e,
                    "Failed to delete file from S3"
                );
                Err(ApiError::InternalServer(format!(
                    "Failed to delete file: {}",
                    e
                )))
            }
        }
    } else {
        let base_dirs = vec![
            ("resumes", state.resumes_dir.clone()),
            ("avatars", state.avatars_dir.clone()),
            ("logos", state.logos_dir.clone()),
            (
                "job-images",
                state
                    .job_images_jobs_dir
                    .parent()
                    .unwrap_or(&state.job_images_jobs_dir)
                    .to_path_buf(),
            ),
        ];

        let mut deleted = false;
        for (category, dir) in base_dirs {
            if file_path.starts_with(category) {
                let relative_path = file_path
                    .strip_prefix(&format!("{}/", category))
                    .unwrap_or(&file_path);
                let full_path = dir.join(relative_path);

                if full_path.exists() {
                    match tokio_fs::remove_file(&full_path).await {
                        Ok(_) => {
                            info!(
                                admin_user_id = %authed.id,
                                file_path = %file_path,
                                "File deleted from local storage"
                            );
                            deleted = true;
                            break;
                        }
                        Err(e) => {
                            error!(
                                admin_user_id = %authed.id,
                                file_path = %file_path,
                                error = %e,
                                "Failed to delete file from local storage"
                            );
                            return Err(ApiError::InternalServer(format!(
                                "Failed to delete file: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        if deleted {
            Ok(Json(MessageResponse {
                message: "File deleted successfully".to_string(),
            }))
        } else {
            Err(ApiError::BadRequest("File not found".to_string()))
        }
    }
}

/// GET /api/admin/files/stats - Get storage statistics
pub async fn get_storage_stats_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<StorageStats>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let mut total_size: i64 = 0;
    let mut file_count: i64 = 0;

    if storage_type == "s3" || storage_type == "s3-cloudfront" {
        match state.aws_service.list_files(None).await {
            Ok(objects) => {
                file_count = objects.len() as i64;
                total_size = objects.iter().map(|obj| obj.size).sum();
            }
            Err(e) => {
                error!(error = %e, "Failed to get S3 stats");
                return Err(ApiError::InternalServer(format!(
                    "Failed to get S3 stats: {}",
                    e
                )));
            }
        }
    } else {
        let base_dirs = vec![
            state.resumes_dir.clone(),
            state.avatars_dir.clone(),
            state.logos_dir.clone(),
            state
                .job_images_jobs_dir
                .parent()
                .unwrap_or(&state.job_images_jobs_dir)
                .to_path_buf(),
        ];

        for dir in base_dirs {
            if let Ok(entries) = tokio_fs::read_dir(&dir).await {
                let mut entries = entries;
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(metadata) = entry.metadata().await {
                        if metadata.is_file() {
                            total_size += metadata.len() as i64;
                            file_count += 1;
                        }
                    }
                }
            }
        }
    }

    info!(
        admin_user_id = %authed.id,
        storage_type = %storage_type,
        total_size = total_size,
        file_count = file_count,
        "Retrieved storage statistics"
    );

    Ok(Json(StorageStats {
        total_size,
        file_count,
        storage_type,
        quota: None,
    }))
}

/// POST /api/admin/files/delete-bulk - Delete multiple files
pub async fn delete_files_bulk_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<DeleteFilesBulkRequest>,
) -> Result<Json<DeleteFilesBulkResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let mut deleted_count = 0;
    let mut errors: Vec<String> = Vec::new();

    for file_path in &request.paths {
        if storage_type == "s3" || storage_type == "s3-cloudfront" {
            match state.aws_service.delete_file(file_path).await {
                Ok(_) => {
                    deleted_count += 1;
                    info!(
                        admin_user_id = %authed.id,
                        file_path = %file_path,
                        "File deleted from S3"
                    );
                }
                Err(e) => {
                    error!(
                        admin_user_id = %authed.id,
                        file_path = %file_path,
                        error = %e,
                        "Failed to delete file from S3"
                    );
                    errors.push(format!("Failed to delete {}: {}", file_path, e));
                }
            }
        } else {
            // Local storage
            let base_dirs = vec![
                ("resumes", state.resumes_dir.clone()),
                ("avatars", state.avatars_dir.clone()),
                ("logos", state.logos_dir.clone()),
                (
                    "job-images",
                    state
                        .job_images_jobs_dir
                        .parent()
                        .unwrap_or(&state.job_images_jobs_dir)
                        .to_path_buf(),
                ),
            ];

            let mut deleted = false;
            for (category, dir) in &base_dirs {
                if file_path.starts_with(*category) {
                    let relative_path = file_path
                        .strip_prefix(&format!("{}/", category))
                        .unwrap_or(file_path);
                    let full_path = dir.join(relative_path);

                    if full_path.exists() {
                        match tokio_fs::remove_file(&full_path).await {
                            Ok(_) => {
                                deleted_count += 1;
                                deleted = true;
                                info!(
                                    admin_user_id = %authed.id,
                                    file_path = %file_path,
                                    "File deleted from local storage"
                                );
                                break;
                            }
                            Err(e) => {
                                error!(
                                    admin_user_id = %authed.id,
                                    file_path = %file_path,
                                    error = %e,
                                    "Failed to delete file from local storage"
                                );
                                errors.push(format!("Failed to delete {}: {}", file_path, e));
                                break;
                            }
                        }
                    }
                }
            }

            if !deleted && errors.iter().all(|e| !e.contains(file_path)) {
                errors.push(format!("File not found: {}", file_path));
            }
        }
    }

    info!(
        admin_user_id = %authed.id,
        deleted_count = deleted_count,
        error_count = errors.len(),
        "Bulk file deletion completed"
    );

    Ok(Json(DeleteFilesBulkResponse {
        message: format!("Deleted {} files", deleted_count),
        deleted_count,
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct DeleteFilesBulkRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DeleteFilesBulkResponse {
    pub message: String,
    pub deleted_count: i32,
}
