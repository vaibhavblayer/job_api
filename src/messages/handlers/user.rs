// User messaging handlers - kept minimal for backward compatibility
// Most functionality moved to WebSocket handlers

use crate::auth::extractors::AuthedUser;
use crate::common::error::ApiError;
use crate::common::id_generator::generate_message_id;
use crate::common::state::AppState;
use crate::messages::models::{CliMessage, ConversationInput, ConversationMessage, EnhancedConversationMessage, MessageAttachment};
use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Response type that includes both CLI format and enhanced format
#[derive(serde::Serialize)]
pub struct EnhancedMessageListResponse {
    pub messages: Vec<EnhancedConversationMessage>,
    pub total: usize,
}

pub async fn list_conversations(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<EnhancedMessageListResponse>, ApiError> {
    let state = state_lock.read().await.clone();
    let messages = sqlx::query_as::<_, ConversationMessage>(
        "SELECT * FROM conversation_messages WHERE user_id = ? ORDER BY datetime(created_at) ASC",
    )
    .bind(&authed.id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Fetch attachments for each message and convert to enhanced format
    let mut enhanced_messages = Vec::new();
    for msg in messages {
        let attachments = sqlx::query_as::<_, MessageAttachment>(
            "SELECT * FROM message_attachments WHERE message_id = ?",
        )
        .bind(&msg.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        enhanced_messages.push(EnhancedConversationMessage {
            id: msg.id,
            user_id: msg.user_id,
            sender: msg.sender,
            message: msg.message,
            attachments,
            is_read: msg.is_read.unwrap_or(0) == 1,
            created_at: msg.created_at,
        });
    }

    let total = enhanced_messages.len();

    Ok(Json(EnhancedMessageListResponse {
        messages: enhanced_messages,
        total,
    }))
}

pub async fn user_send_conversation(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(input): Json<ConversationInput>,
) -> Result<Json<CliMessage>, ApiError> {
    let trimmed = input.message.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("message cannot be empty".to_string()));
    }

    let message = trimmed.to_owned();

    let state = state_lock.read().await.clone();
    let message_id_str = generate_message_id();
    let created_at = chrono::Utc::now().to_rfc3339();
    
    sqlx::query(
        "INSERT INTO conversation_messages (id, user_id, sender, message, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&message_id_str)
    .bind(&authed.id)
    .bind("user")
    .bind(&message)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %authed.id,
            message_id = %message_id_str,
            "Database error inserting conversation message"
        );
        ApiError::DatabaseError(e)
    })?;

    // Return CLI-compatible format
    Ok(Json(CliMessage {
        id: message_id_str,
        sender_id: authed.id.clone(),
        receiver_id: "admin".to_string(), // Admin receiver
        content: message,
        created_at,
        read: false,
    }))
}


/// POST /api/conversations/read - Mark all messages as read for the current user
pub async fn mark_conversation_read(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();
    
    // Mark all admin messages as read for this user
    let result = sqlx::query(
        "UPDATE conversation_messages SET is_read = 1 WHERE user_id = ? AND sender = 'admin' AND (is_read IS NULL OR is_read = 0)",
    )
    .bind(&authed.id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let messages_marked = result.rows_affected();
    
    info!(
        user_id = %authed.id,
        messages_marked = %messages_marked,
        "Marked messages as read"
    );

    Ok(Json(serde_json::json!({
        "message": "Messages marked as read",
        "messages_marked": messages_marked
    })))
}

/// GET /api/attachments/:filename - Serve attachment file
pub async fn serve_attachment(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    _authed: AuthedUser,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    
    // Sanitize filename to prevent directory traversal
    let safe_filename = filename.replace("..", "").replace("/", "").replace("\\", "");
    
    // Look for the file in the attachments directory
    let attachments_dir = state.resumes_dir.join("attachments");
    let file_path = attachments_dir.join(&safe_filename);
    
    if !file_path.exists() {
        return Err(ApiError::BadRequest("Attachment not found".to_string()));
    }
    
    // Read file content
    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|e| {
            error!(error = %e, filename = %safe_filename, "Failed to read attachment file");
            ApiError::InternalServer("Failed to read attachment".to_string())
        })?;
    
    // Determine content type from filename
    let content_type = match file_path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => "application/pdf",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    };
    
    info!(filename = %safe_filename, content_type = %content_type, "Serving attachment");
    
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CACHE_CONTROL, "public, max-age=31536000".to_string()),
        ],
        content,
    ))
}
