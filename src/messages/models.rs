use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// Core Message Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConversationMessage {
    pub id: String,
    pub user_id: String,
    pub sender: String,
    pub message: String,
    pub is_read: Option<i64>, // SQLite uses INTEGER for boolean
    pub created_at: Option<String>,
}

// CLI-compatible message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliMessage {
    pub id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub content: String,
    pub created_at: String,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliMessageListResponse {
    pub messages: Vec<CliMessage>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageAttachment {
    pub id: String,
    pub message_id: String,
    pub filename: String,
    pub original_filename: String,
    pub file_size: i64,
    pub mime_type: String,
    pub file_path: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnhancedConversationMessage {
    pub id: String,
    pub user_id: String,
    pub sender: String,
    pub message: String,
    pub attachments: Vec<MessageAttachment>,
    pub is_read: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConversationWithMetadata {
    pub id: String,
    pub participants: Vec<ConversationParticipant>,
    pub last_message: Option<EnhancedConversationMessage>,
    pub unread_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ConversationParticipant {
    pub user_id: String,
    pub name: Option<String>,
    pub email: String,
    pub avatar: Option<String>,
    pub role: String, // user, admin
}

// ============================================================================
// Request/Response Models
// ============================================================================

#[derive(Deserialize)]
pub struct ConversationInput {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub message: String,
    pub conversation_id: Option<String>,
    pub recipient_id: Option<String>,
}

#[derive(Debug)]
pub struct CreateMessageWithAttachmentsRequest {
    pub message: String,
    pub conversation_id: Option<String>,
    pub recipient_id: Option<String>,
    pub attachments: Vec<AttachmentData>,
}

#[derive(Debug)]
pub struct AttachmentData {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct MessageSearchRequest {
    pub query: String,
    pub conversation_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MessageSearchResult {
    pub messages: Vec<EnhancedConversationMessage>,
    pub total_count: i64,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

// ============================================================================
// WebSocket Message Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    // Client → Server
    SendMessage {
        content: String,
        conversation_id: Option<String>,
    },
    UploadFile {
        filename: String,
        mime_type: String,
        #[serde(with = "base64_serde")]
        data: Vec<u8>,
        conversation_id: Option<String>,
    },
    UploadFileChunk {
        upload_id: String,
        chunk_index: u32,
        total_chunks: u32,
        #[serde(with = "base64_serde")]
        data: Vec<u8>,
    },
    TypingStart {
        conversation_id: String,
    },
    TypingStop {
        conversation_id: String,
    },
    MarkRead {
        message_id: String,
    },
    Ping,

    // Server → Client
    MessageReceived {
        message: EnhancedConversationMessage,
    },
    MessageDelivered {
        message_id: String,
        delivered_at: String,
    },
    TypingIndicator {
        user_id: String,
        user_name: String,
        is_typing: bool,
        conversation_id: String,
    },
    ReadReceipt {
        message_id: String,
        read_by: String,
        read_at: String,
    },
    PresenceUpdate {
        user_id: String,
        status: PresenceStatus,
    },
    FileUploadProgress {
        upload_id: String,
        progress: f32,
    },
    FileUploadComplete {
        upload_id: String,
        file_url: String,
        attachment: MessageAttachment,
    },
    Error {
        code: String,
        message: String,
    },
    Pong,
    Connected {
        user_id: String,
    },
    MissedMessages {
        messages: Vec<EnhancedConversationMessage>,
        count: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Online,
    Offline,
    Away,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingIndicator {
    pub user_id: String,
    pub user_name: String,
    pub conversation_id: String,
    pub is_typing: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadReceipt {
    pub message_id: String,
    pub user_id: String,
    pub read_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadProgress {
    pub upload_id: String,
    pub filename: String,
    pub progress: f32,
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
}

// Helper module for base64 serialization
mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}
