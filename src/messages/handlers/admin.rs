// Admin messaging handlers - kept minimal for backward compatibility
// Most functionality moved to WebSocket handlers

use crate::auth::extractors::AuthedUser;
use crate::common::error::ApiError;
use crate::common::id_generator::generate_message_id;
use crate::common::state::AppState;
use crate::messages::models::{ConversationInput, ConversationMessage, EnhancedConversationMessage, MessageAttachment};
use axum::{extract::Path, Extension, Json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::error;

pub async fn admin_list_conversations(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<EnhancedConversationMessage>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();
    let messages = sqlx::query_as::<_, ConversationMessage>(
        "SELECT * FROM conversation_messages WHERE user_id = ? ORDER BY datetime(created_at) ASC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Fetch attachments for each message
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

    Ok(Json(enhanced_messages))
}

pub async fn admin_send_conversation(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(user_id): Path<String>,
    Json(input): Json<ConversationInput>,
) -> Result<Json<ConversationMessage>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let trimmed = input.message.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("message cannot be empty".to_string()));
    }

    let message = trimmed.to_owned();

    let state = state_lock.read().await.clone();
    let message_id = generate_message_id();
    sqlx::query(
        "INSERT INTO conversation_messages (id, user_id, sender, message) VALUES (?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(&user_id)
    .bind("admin")
    .bind(&message)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            target_user_id = %user_id,
            admin_user_id = %authed.id,
            message_id = %message_id,
            "Database error inserting admin conversation message"
        );
        ApiError::DatabaseError(e)
    })?;

    let msg = sqlx::query_as::<_, ConversationMessage>(
        "SELECT * FROM conversation_messages WHERE id = ?",
    )
    .bind(&message_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(Json(msg))
}

pub async fn admin_get_all_conversations(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();

    let conversations = sqlx::query(
        r#"
        SELECT
            cm.user_id,
            u.name as user_name,
            u.email as user_email,
            u.avatar as user_avatar,
            cm.message as last_message,
            cm.created_at as last_message_at,
            (SELECT COUNT(*) FROM conversation_messages
             WHERE user_id = cm.user_id
             AND sender = 'user'
             AND is_read = 0) as unread_count
        FROM conversation_messages cm
        INNER JOIN users u ON cm.user_id = u.id
        WHERE cm.id IN (
            SELECT id FROM conversation_messages cm2
            WHERE cm2.user_id = cm.user_id
            ORDER BY datetime(cm2.created_at) DESC
            LIMIT 1
        )
        ORDER BY datetime(cm.created_at) DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let mut result = Vec::new();
    for row in conversations {
        use sqlx::Row;
        let user_id: String = row.try_get_unchecked("user_id").unwrap_or_default();
        let user_name: String = row.try_get_unchecked("user_name").unwrap_or_default();
        let user_email: String = row.try_get_unchecked("user_email").unwrap_or_default();
        let user_avatar: Option<String> = row.try_get_unchecked("user_avatar").ok();
        let last_message: String = row.try_get_unchecked("last_message").unwrap_or_default();
        let last_message_at: String = row.try_get_unchecked("last_message_at").unwrap_or_default();
        let unread_count: i64 = row.try_get_unchecked("unread_count").unwrap_or(0);

        result.push(serde_json::json!({
            "user_id": user_id,
            "user_name": user_name,
            "user_email": user_email,
            "user_avatar": user_avatar,
            "last_message": last_message,
            "last_message_at": last_message_at,
            "unread_count": unread_count
        }));
    }

    Ok(Json(result))
}

/// POST /api/admin/conversations/:user_id/read - Mark all messages in a conversation as read
pub async fn admin_mark_conversation_read(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("admin privileges required".to_string()));
    }

    let state = state_lock.read().await.clone();
    
    // Mark all messages from this user as read (messages sent by user, not admin)
    let result = sqlx::query(
        "UPDATE conversation_messages SET is_read = 1 WHERE user_id = ? AND sender = 'user' AND is_read = 0",
    )
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    let rows_affected = result.rows_affected();

    tracing::info!(
        admin_user_id = %authed.id,
        conversation_user_id = %user_id,
        messages_marked = rows_affected,
        "Conversation marked as read by admin"
    );

    Ok(Json(serde_json::json!({
        "message": "Conversation marked as read",
        "messages_marked": rows_affected
    })))
}
