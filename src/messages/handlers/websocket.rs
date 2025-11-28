use crate::auth::extractors::AuthedUser;
use crate::common::error::ApiError;
use crate::common::id_generator::{generate_connection_id, generate_raw_id};
use crate::common::state::AppState;
use crate::messages::models::{EnhancedConversationMessage, WebSocketMessage};
use crate::messages::services::{ConnectionManager, MessageService, PresenceService};
use crate::messages::validators;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, WebSocketUpgrade,
    },
    response::IntoResponse,
    Extension,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// WebSocket upgrade handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    // Extract JWT token from query params or headers
    let token = params
        .get("token")
        .ok_or_else(|| ApiError::Unauthorized("Missing authentication token".to_string()))?;

    // Validate JWT and extract user info
    let state = state_lock.read().await.clone();
    let claims = crate::auth::handlers::validate_jwt(token)?;

    // Fetch user from database
    let user = sqlx::query_as::<_, crate::auth::models::User>("SELECT * FROM users WHERE id = ?")
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::Unauthorized("User not found".to_string()))?;

    // Check if user is admin based on admin_emails list
    let user_email_lower = user.email.to_lowercase();
    let is_admin = state.admin_emails.contains(&user_email_lower);

    let authed_user = AuthedUser {
        id: user.id.clone(),
        email: user.email.clone(),
        is_admin,
    };

    info!(
        user_id = %authed_user.id,
        email = %authed_user.email,
        "WebSocket connection authenticated"
    );

    // Upgrade the connection
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, authed_user, state_lock)))
}

/// Handle WebSocket connection
async fn handle_socket(
    socket: WebSocket,
    authed_user: AuthedUser,
    state_lock: Arc<RwLock<AppState>>,
) {
    let connection_id = generate_connection_id();
    let user_id = authed_user.id.clone();

    info!(
        user_id = %user_id,
        connection_id = %connection_id,
        "WebSocket connection established"
    );

    let state = state_lock.read().await.clone();
    let connection_manager = state.connection_manager.clone();
    let presence_service = PresenceService::new(connection_manager.clone());
    let message_service = MessageService::new(state.db.clone());

    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending messages to this connection
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Register the connection
    connection_manager
        .register(user_id.clone(), connection_id.clone(), tx.clone())
        .await;

    // Send connected message
    let connected_msg = WebSocketMessage::Connected {
        user_id: user_id.clone(),
    };
    if let Ok(json) = serde_json::to_string(&connected_msg) {
        let _ = sender.send(Message::Text(json)).await;
    }

    // Get missed messages (messages sent while user was offline)
    let last_disconnect = presence_service.get_last_seen(&user_id).await;
    if let Some(since) = last_disconnect {
        match message_service.get_messages_since(&user_id, since).await {
            Ok(missed) if !missed.is_empty() => {
                let missed_msg = WebSocketMessage::MissedMessages {
                    count: missed.len(),
                    messages: missed,
                };
                if let Ok(json) = serde_json::to_string(&missed_msg) {
                    let _ = sender.send(Message::Text(json)).await;
                }
            }
            _ => {}
        }
    }

    // Mark user as online
    // In a real app, you'd get relevant users from conversations
    // Get relevant users (those with active conversations)
    let relevant_users = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT CASE 
            WHEN sender = ? THEN user_id 
            ELSE sender 
        END
        FROM conversation_messages 
        WHERE sender = ? OR user_id = ?
        "#,
    )
    .bind(&user_id)
    .bind(&user_id)
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    presence_service.mark_online(&user_id, relevant_users).await;

    // Spawn task to send messages from the channel to the WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to receive messages from the WebSocket
    let user_id_clone = user_id.clone();
    let connection_id_clone = connection_id.clone();
    let connection_manager_clone = connection_manager.clone();
    let state_lock_clone = state_lock.clone();

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Err(e) = handle_message(
                msg,
                &user_id_clone,
                &connection_id_clone,
                &authed_user,
                &connection_manager_clone,
                &state_lock_clone,
            )
            .await
            {
                error!(
                    user_id = %user_id_clone,
                    connection_id = %connection_id_clone,
                    error = %e,
                    "Error handling WebSocket message"
                );

                // Send error message to client
                let error_msg = WebSocketMessage::Error {
                    code: "MESSAGE_ERROR".to_string(),
                    message: e.to_string(),
                };
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    let _ = connection_manager_clone
                        .send_to_connection(&connection_id_clone, error_msg)
                        .await;
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        }
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }

    // Cleanup: unregister connection and mark user as offline
    connection_manager.unregister(&connection_id).await;

    // Only mark as offline if user has no other connections
    if connection_manager.get_user_connection_count(&user_id).await == 0 {
        // Get relevant users (those with active conversations)
        let relevant_users = sqlx::query_scalar::<_, String>(
            r#"
            SELECT DISTINCT CASE 
                WHEN sender = ? THEN user_id 
                ELSE sender 
            END
            FROM conversation_messages 
            WHERE sender = ? OR user_id = ?
            "#,
        )
        .bind(&user_id)
        .bind(&user_id)
        .bind(&user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        presence_service
            .mark_offline(&user_id, relevant_users)
            .await;
    }

    info!(
        user_id = %user_id,
        connection_id = %connection_id,
        "WebSocket connection closed"
    );
}

/// Handle individual WebSocket messages
async fn handle_message(
    msg: Message,
    user_id: &str,
    connection_id: &str,
    authed_user: &AuthedUser,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    match msg {
        Message::Text(text) => {
            debug!(
                user_id = %user_id,
                connection_id = %connection_id,
                "Received text message"
            );

            let ws_msg: WebSocketMessage = serde_json::from_str(&text)
                .map_err(|e| ApiError::BadRequest(format!("Invalid message format: {}", e)))?;

            handle_websocket_message(
                ws_msg,
                user_id,
                connection_id,
                authed_user,
                connection_manager,
                state_lock,
            )
            .await?;
        }
        Message::Binary(_) => {
            warn!(
                user_id = %user_id,
                connection_id = %connection_id,
                "Received unsupported binary message"
            );
            return Err(ApiError::BadRequest(
                "Binary messages not supported".to_string(),
            ));
        }
        Message::Ping(_) => {
            debug!(
                user_id = %user_id,
                connection_id = %connection_id,
                "Received ping"
            );
            connection_manager.update_heartbeat(connection_id).await;
        }
        Message::Pong(_) => {
            debug!(
                user_id = %user_id,
                connection_id = %connection_id,
                "Received pong"
            );
            connection_manager.update_heartbeat(connection_id).await;
        }
        Message::Close(_) => {
            info!(
                user_id = %user_id,
                connection_id = %connection_id,
                "Received close message"
            );
        }
    }

    Ok(())
}

/// Handle parsed WebSocket messages
async fn handle_websocket_message(
    msg: WebSocketMessage,
    user_id: &str,
    connection_id: &str,
    authed_user: &AuthedUser,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    match msg {
        WebSocketMessage::SendMessage {
            content,
            conversation_id,
        } => {
            handle_send_message(
                content,
                conversation_id,
                user_id,
                authed_user,
                connection_manager,
                state_lock,
            )
            .await?;
        }
        WebSocketMessage::TypingStart { conversation_id } => {
            handle_typing_indicator(
                user_id,
                &conversation_id,
                true,
                connection_manager,
                state_lock,
            )
            .await?;
        }
        WebSocketMessage::TypingStop { conversation_id } => {
            handle_typing_indicator(
                user_id,
                &conversation_id,
                false,
                connection_manager,
                state_lock,
            )
            .await?;
        }
        WebSocketMessage::MarkRead { message_id } => {
            handle_mark_read(&message_id, user_id, connection_manager, state_lock).await?;
        }
        WebSocketMessage::Ping => {
            connection_manager.update_heartbeat(connection_id).await;
            connection_manager
                .send_to_connection(connection_id, WebSocketMessage::Pong)
                .await
                .map_err(|e| ApiError::InternalServer(e))?;
        }
        WebSocketMessage::UploadFile {
            filename,
            mime_type,
            data,
            conversation_id,
        } => {
            handle_file_upload(
                filename,
                mime_type,
                data,
                conversation_id,
                user_id,
                authed_user,
                connection_manager,
                state_lock,
            )
            .await?;
        }
        _ => {
            warn!(
                user_id = %user_id,
                message_type = ?msg,
                "Received unsupported message type from client"
            );
        }
    }

    Ok(())
}

/// Handle sending a message
async fn handle_send_message(
    content: String,
    conversation_id: Option<String>,
    user_id: &str,
    authed_user: &AuthedUser,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    // Validate message content
    validators::validate_message_content(&content)?;

    let state = state_lock.read().await.clone();
    let message_service = MessageService::new(state.db.clone());

    // Determine target user ID
    let target_user_id = if authed_user.is_admin {
        conversation_id.ok_or_else(|| {
            ApiError::BadRequest("conversation_id required for admin messages".to_string())
        })?
    } else {
        user_id.to_string()
    };

    let sender = if authed_user.is_admin {
        "admin"
    } else {
        "user"
    };

    // Create message
    let message = message_service
        .create_message(&target_user_id, sender, &content)
        .await?;

    // Convert to enhanced message
    let enhanced_message = EnhancedConversationMessage {
        id: message.id.clone(),
        user_id: message.user_id.clone(),
        sender: message.sender.clone(),
        message: message.message.clone(),
        attachments: vec![],
        is_read: false,
        created_at: message.created_at.clone(),
    };

    // Send to recipient
    let recipient_msg = WebSocketMessage::MessageReceived {
        message: enhanced_message.clone(),
    };

    // Send to target user's connections
    let _ = connection_manager
        .send_to_user(&target_user_id, recipient_msg)
        .await;

    // Send delivery confirmation to sender
    let delivery_msg = WebSocketMessage::MessageDelivered {
        message_id: message.id.clone(),
        delivered_at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = connection_manager.send_to_user(user_id, delivery_msg).await;

    info!(
        user_id = %user_id,
        message_id = %message.id,
        target_user_id = %target_user_id,
        "Message sent via WebSocket"
    );

    Ok(())
}

/// Handle typing indicator
async fn handle_typing_indicator(
    user_id: &str,
    conversation_id: &str,
    is_typing: bool,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    let state = state_lock.read().await.clone();

    // Get user name
    let user = sqlx::query_as::<_, crate::auth::models::User>("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("User not found".to_string()))?;

    let user_name = user.name.unwrap_or_else(|| "Unknown".to_string());

    // Broadcast typing indicator to conversation participants
    let typing_msg = WebSocketMessage::TypingIndicator {
        user_id: user_id.to_string(),
        user_name,
        is_typing,
        conversation_id: conversation_id.to_string(),
    };

    // In a real app, you'd get conversation participants from the database
    // For now, just broadcast to the conversation_id (which is the other user's ID)
    let _ = connection_manager
        .send_to_user(conversation_id, typing_msg)
        .await;

    debug!(
        user_id = %user_id,
        conversation_id = %conversation_id,
        is_typing = is_typing,
        "Typing indicator sent"
    );

    Ok(())
}

/// Handle mark message as read
async fn handle_mark_read(
    message_id: &str,
    user_id: &str,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    let state = state_lock.read().await.clone();
    let message_service = MessageService::new(state.db.clone());

    message_service
        .mark_message_read(message_id, user_id)
        .await?;

    // Broadcast read receipt
    let read_receipt = WebSocketMessage::ReadReceipt {
        message_id: message_id.to_string(),
        read_by: user_id.to_string(),
        read_at: chrono::Utc::now().to_rfc3339(),
    };

    // Send to all user's connections
    let _ = connection_manager.send_to_user(user_id, read_receipt).await;

    info!(
        user_id = %user_id,
        message_id = %message_id,
        "Message marked as read via WebSocket"
    );

    Ok(())
}

/// Handle file upload
async fn handle_file_upload(
    filename: String,
    mime_type: String,
    data: Vec<u8>,
    conversation_id: Option<String>,
    user_id: &str,
    authed_user: &AuthedUser,
    connection_manager: &ConnectionManager,
    state_lock: &Arc<RwLock<AppState>>,
) -> Result<(), ApiError> {
    // Validate file
    validators::validate_attachment(&filename, &mime_type, data.len())?;
    validators::validate_file_content(&data, &mime_type)?;

    let state = state_lock.read().await.clone();
    let message_service = MessageService::new(state.db.clone());

    // Determine target user ID
    let target_user_id = if authed_user.is_admin {
        conversation_id.ok_or_else(|| {
            ApiError::BadRequest("conversation_id required for admin messages".to_string())
        })?
    } else {
        user_id.to_string()
    };

    let sender = if authed_user.is_admin {
        "admin"
    } else {
        "user"
    };

    // Create message for the attachment
    let message = message_service
        .create_message(&target_user_id, sender, &format!("Sent file: {}", filename))
        .await?;

    // Save file
    let upload_id = generate_raw_id(8);
    let safe_filename = validators::sanitize_filename(&filename);
    let stored_filename = format!("{}_{}", upload_id, safe_filename);

    let attachments_dir = state.resumes_dir.join("attachments");
    tokio::fs::create_dir_all(&attachments_dir)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create attachments directory");
            ApiError::InternalServer("Failed to create attachments directory".to_string())
        })?;

    let file_path = attachments_dir.join(&stored_filename);
    tokio::fs::write(&file_path, &data).await.map_err(|e| {
        error!(error = %e, file_path = %file_path.display(), "Failed to save attachment file");
        ApiError::AttachmentError("Failed to save attachment".to_string())
    })?;

    // Save attachment metadata
    let attachment_data = crate::messages::models::AttachmentData {
        filename: filename.clone(),
        content_type: mime_type,
        data,
    };

    let attachment = message_service
        .save_attachment(
            &message.id,
            &attachment_data,
            &stored_filename,
            &format!("attachments/{}", stored_filename),
        )
        .await?;

    // Send upload complete message
    let complete_msg = WebSocketMessage::FileUploadComplete {
        upload_id: upload_id.clone(),
        file_url: format!("/api/attachments/{}", stored_filename),
        attachment: attachment.clone(),
    };

    let _ = connection_manager.send_to_user(user_id, complete_msg).await;

    // Send message received to recipient
    let enhanced_message = EnhancedConversationMessage {
        id: message.id.clone(),
        user_id: message.user_id.clone(),
        sender: message.sender.clone(),
        message: message.message.clone(),
        attachments: vec![attachment],
        is_read: false,
        created_at: message.created_at.clone(),
    };

    let recipient_msg = WebSocketMessage::MessageReceived {
        message: enhanced_message,
    };

    let _ = connection_manager
        .send_to_user(&target_user_id, recipient_msg)
        .await;

    info!(
        user_id = %user_id,
        upload_id = %upload_id,
        filename = %filename,
        "File uploaded via WebSocket"
    );

    Ok(())
}
