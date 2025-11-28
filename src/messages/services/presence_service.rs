use crate::messages::models::PresenceStatus;
use crate::messages::services::websocket_service::ConnectionManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Tracks user presence and last seen timestamps
#[derive(Clone)]
pub struct PresenceService {
    connection_manager: ConnectionManager,
    // Map of user_id -> last_seen timestamp
    last_seen: Arc<RwLock<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
}

impl PresenceService {
    pub fn new(connection_manager: ConnectionManager) -> Self {
        Self {
            connection_manager,
            last_seen: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the current presence status for a user
    pub async fn get_status(&self, user_id: &str) -> PresenceStatus {
        if self.connection_manager.is_user_online(user_id).await {
            PresenceStatus::Online
        } else {
            PresenceStatus::Offline
        }
    }

    /// Update last seen timestamp for a user
    pub async fn update_last_seen(&self, user_id: &str) {
        let now = chrono::Utc::now();
        self.last_seen
            .write()
            .await
            .insert(user_id.to_string(), now);

        debug!(
            user_id = %user_id,
            timestamp = %now,
            "Updated last seen timestamp"
        );
    }

    /// Get last seen timestamp for a user
    pub async fn get_last_seen(&self, user_id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        self.last_seen.read().await.get(user_id).copied()
    }

    /// Mark user as online and broadcast presence
    pub async fn mark_online(&self, user_id: &str, relevant_users: Vec<String>) {
        self.update_last_seen(user_id).await;

        info!(
            user_id = %user_id,
            relevant_users_count = relevant_users.len(),
            "User marked as online"
        );

        // Broadcast online status to relevant users
        let message = crate::messages::models::WebSocketMessage::PresenceUpdate {
            user_id: user_id.to_string(),
            status: PresenceStatus::Online,
        };

        self.connection_manager
            .broadcast_to_users(&relevant_users, message)
            .await;
    }

    /// Mark user as offline and broadcast presence
    pub async fn mark_offline(&self, user_id: &str, relevant_users: Vec<String>) {
        self.update_last_seen(user_id).await;

        info!(
            user_id = %user_id,
            relevant_users_count = relevant_users.len(),
            "User marked as offline"
        );

        // Broadcast offline status to relevant users
        let message = crate::messages::models::WebSocketMessage::PresenceUpdate {
            user_id: user_id.to_string(),
            status: PresenceStatus::Offline,
        };

        self.connection_manager
            .broadcast_to_users(&relevant_users, message)
            .await;
    }

    /// Get presence status for multiple users
    pub async fn get_statuses(&self, user_ids: &[String]) -> HashMap<String, PresenceStatus> {
        let mut statuses = HashMap::new();

        for user_id in user_ids {
            let status = self.get_status(user_id).await;
            statuses.insert(user_id.clone(), status);
        }

        statuses
    }

    /// Get all online users
    pub async fn get_online_users(&self) -> Vec<String> {
        self.connection_manager.get_online_users().await
    }

    /// Check if user is online
    pub async fn is_online(&self, user_id: &str) -> bool {
        self.connection_manager.is_user_online(user_id).await
    }
}
