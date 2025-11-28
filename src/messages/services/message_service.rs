use crate::common::error::ApiError;
use crate::common::id_generator::{generate_attachment_id, generate_message_id};
use crate::messages::models::{
    AttachmentData, ConversationMessage, EnhancedConversationMessage, MessageAttachment,
};
use sqlx::SqlitePool;
use tracing::{error, info};

pub struct MessageService {
    db: SqlitePool,
}

impl MessageService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Create a new message
    pub async fn create_message(
        &self,
        user_id: &str,
        sender: &str,
        content: &str,
    ) -> Result<ConversationMessage, ApiError> {
        let message_id = generate_message_id();

        sqlx::query(
            "INSERT INTO conversation_messages (id, user_id, sender, message, is_read, created_at) VALUES (?, ?, ?, ?, 0, datetime('now'))"
        )
        .bind(&message_id)
        .bind(user_id)
        .bind(sender)
        .bind(content)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                user_id = %user_id,
                message_id = %message_id,
                "Database error creating message"
            );
            ApiError::DatabaseError(e)
        })?;

        let message = sqlx::query_as::<_, ConversationMessage>(
            "SELECT * FROM conversation_messages WHERE id = ?",
        )
        .bind(&message_id)
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        info!(
            user_id = %user_id,
            message_id = %message_id,
            sender = %sender,
            "Message created successfully"
        );

        Ok(message)
    }

    /// Get messages for a user
    pub async fn get_user_messages(
        &self,
        user_id: &str,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<EnhancedConversationMessage>, ApiError> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let messages = sqlx::query_as::<_, ConversationMessage>(
            "SELECT * FROM conversation_messages WHERE user_id = ? ORDER BY datetime(created_at) ASC LIMIT ? OFFSET ?"
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        let mut enhanced_messages = Vec::new();
        for msg in messages {
            let attachments = self.get_message_attachments(&msg.id).await?;
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

        Ok(enhanced_messages)
    }

    /// Get attachments for a message
    pub async fn get_message_attachments(
        &self,
        message_id: &str,
    ) -> Result<Vec<MessageAttachment>, ApiError> {
        let attachments = sqlx::query_as::<_, MessageAttachment>(
            "SELECT * FROM message_attachments WHERE message_id = ?",
        )
        .bind(message_id)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(attachments)
    }

    /// Mark a message as read
    pub async fn mark_message_read(&self, message_id: &str, user_id: &str) -> Result<(), ApiError> {
        let result = sqlx::query(
            "UPDATE conversation_messages SET is_read = 1 WHERE id = ? AND user_id = ?",
        )
        .bind(message_id)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(ApiError::BadRequest("Message not found".to_string()));
        }

        info!(
            message_id = %message_id,
            user_id = %user_id,
            "Message marked as read"
        );

        Ok(())
    }

    /// Mark all messages in a conversation as read
    pub async fn mark_conversation_read(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            "UPDATE conversation_messages SET is_read = 1 WHERE user_id = ? AND is_read = 0",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        let rows_affected = result.rows_affected();

        info!(
            user_id = %user_id,
            messages_marked = rows_affected,
            "Conversation marked as read"
        );

        Ok(rows_affected)
    }

    /// Search messages
    pub async fn search_messages(
        &self,
        query: &str,
        user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<EnhancedConversationMessage>, i64), ApiError> {
        let mut sql = "SELECT * FROM conversation_messages WHERE message LIKE ?".to_string();
        let mut count_sql =
            "SELECT COUNT(*) FROM conversation_messages WHERE message LIKE ?".to_string();

        let search_pattern = format!("%{}%", query);

        let messages = if let Some(uid) = user_id {
            sql.push_str(" AND user_id = ?");
            count_sql.push_str(" AND user_id = ?");
            sql.push_str(" ORDER BY datetime(created_at) DESC LIMIT ? OFFSET ?");

            sqlx::query_as::<_, ConversationMessage>(&sql)
                .bind(&search_pattern)
                .bind(uid)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db)
                .await
        } else {
            sql.push_str(" ORDER BY datetime(created_at) DESC LIMIT ? OFFSET ?");

            sqlx::query_as::<_, ConversationMessage>(&sql)
                .bind(&search_pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db)
                .await
        }
        .map_err(ApiError::DatabaseError)?;

        let total_count = if let Some(uid) = user_id {
            sqlx::query_scalar::<_, i64>(&count_sql)
                .bind(&search_pattern)
                .bind(uid)
                .fetch_one(&self.db)
                .await
        } else {
            sqlx::query_scalar::<_, i64>(&count_sql)
                .bind(&search_pattern)
                .fetch_one(&self.db)
                .await
        }
        .unwrap_or(0);

        let mut enhanced_messages = Vec::new();
        for msg in messages {
            let attachments = self.get_message_attachments(&msg.id).await?;
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

        Ok((enhanced_messages, total_count))
    }

    /// Save attachment metadata
    pub async fn save_attachment(
        &self,
        message_id: &str,
        attachment_data: &AttachmentData,
        stored_filename: &str,
        file_path: &str,
    ) -> Result<MessageAttachment, ApiError> {
        let attachment_id = generate_attachment_id();

        sqlx::query(
            r#"
            INSERT INTO message_attachments (id, message_id, filename, original_filename, file_size, mime_type, file_path, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#
        )
        .bind(&attachment_id)
        .bind(message_id)
        .bind(stored_filename)
        .bind(&attachment_data.filename)
        .bind(attachment_data.data.len() as i64)
        .bind(&attachment_data.content_type)
        .bind(file_path)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                attachment_id = %attachment_id,
                message_id = %message_id,
                "Database error saving attachment metadata"
            );
            ApiError::DatabaseError(e)
        })?;

        let attachment = MessageAttachment {
            id: attachment_id,
            message_id: message_id.to_string(),
            filename: stored_filename.to_string(),
            original_filename: attachment_data.filename.clone(),
            file_size: attachment_data.data.len() as i64,
            mime_type: attachment_data.content_type.clone(),
            file_path: file_path.to_string(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        info!(
            attachment_id = %attachment.id,
            message_id = %message_id,
            filename = %attachment.original_filename,
            "Attachment saved successfully"
        );

        Ok(attachment)
    }

    /// Get unread message count for a user
    pub async fn get_unread_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_messages WHERE user_id = ? AND is_read = 0",
        )
        .bind(user_id)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        Ok(count)
    }

    /// Get messages created after a specific timestamp (for missed messages)
    pub async fn get_messages_since(
        &self,
        user_id: &str,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<EnhancedConversationMessage>, ApiError> {
        let since_str = since.to_rfc3339();

        let messages = sqlx::query_as::<_, ConversationMessage>(
            "SELECT * FROM conversation_messages WHERE user_id = ? AND datetime(created_at) > datetime(?) ORDER BY datetime(created_at) ASC"
        )
        .bind(user_id)
        .bind(&since_str)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        let mut enhanced_messages = Vec::new();
        for msg in messages {
            let attachments = self.get_message_attachments(&msg.id).await?;
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

        Ok(enhanced_messages)
    }
}
