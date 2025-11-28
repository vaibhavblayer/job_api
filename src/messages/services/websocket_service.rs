use crate::messages::models::{PresenceStatus, WebSocketMessage};
use axum::extract::ws::Message;
use futures_util::stream::SplitSink;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

pub type WsSender = SplitSink<axum::extract::ws::WebSocket, Message>;

/// Connection information for a WebSocket client
#[derive(Debug, Clone)]
pub struct Connection {
    pub user_id: String,
    pub connection_id: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// Manages active WebSocket connections
#[derive(Clone)]
pub struct ConnectionManager {
    // Map of user_id -> list of connection_ids
    user_connections: Arc<RwLock<HashMap<String, Vec<String>>>>,
    // Map of connection_id -> sender channel
    connections: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>>,
    // Map of connection_id -> Connection info
    connection_info: Arc<RwLock<HashMap<String, Connection>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            user_connections: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_info: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new WebSocket connection
    pub async fn register(
        &self,
        user_id: String,
        connection_id: String,
        sender: mpsc::UnboundedSender<Message>,
    ) {
        let now = chrono::Utc::now();

        // Store connection sender
        self.connections
            .write()
            .await
            .insert(connection_id.clone(), sender);

        // Store connection info
        let connection = Connection {
            user_id: user_id.clone(),
            connection_id: connection_id.clone(),
            connected_at: now,
            last_heartbeat: now,
        };
        self.connection_info
            .write()
            .await
            .insert(connection_id.clone(), connection);

        // Add to user connections
        let mut user_conns = self.user_connections.write().await;
        user_conns
            .entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(connection_id.clone());

        info!(
            user_id = %user_id,
            connection_id = %connection_id,
            "WebSocket connection registered"
        );
    }

    /// Unregister a WebSocket connection
    pub async fn unregister(&self, connection_id: &str) {
        // Get connection info before removing
        let conn_info = self.connection_info.write().await.remove(connection_id);

        if let Some(info) = conn_info {
            // Remove from connections
            self.connections.write().await.remove(connection_id);

            // Remove from user connections
            let mut user_conns = self.user_connections.write().await;
            if let Some(conns) = user_conns.get_mut(&info.user_id) {
                conns.retain(|id| id != connection_id);
                if conns.is_empty() {
                    user_conns.remove(&info.user_id);
                }
            }

            info!(
                user_id = %info.user_id,
                connection_id = %connection_id,
                "WebSocket connection unregistered"
            );
        }
    }

    /// Update heartbeat timestamp for a connection
    pub async fn update_heartbeat(&self, connection_id: &str) {
        if let Some(conn) = self.connection_info.write().await.get_mut(connection_id) {
            conn.last_heartbeat = chrono::Utc::now();
            debug!(connection_id = %connection_id, "Heartbeat updated");
        }
    }

    /// Get all connection IDs for a user
    pub async fn get_user_connections(&self, user_id: &str) -> Vec<String> {
        self.user_connections
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a user is online (has any active connections)
    pub async fn is_user_online(&self, user_id: &str) -> bool {
        self.user_connections
            .read()
            .await
            .get(user_id)
            .map(|conns| !conns.is_empty())
            .unwrap_or(false)
    }

    /// Get all online user IDs
    pub async fn get_online_users(&self) -> Vec<String> {
        self.user_connections.read().await.keys().cloned().collect()
    }

    /// Send a message to a specific connection
    pub async fn send_to_connection(
        &self,
        connection_id: &str,
        message: WebSocketMessage,
    ) -> Result<(), String> {
        let json = serde_json::to_string(&message)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;

        let connections = self.connections.read().await;
        if let Some(sender) = connections.get(connection_id) {
            sender
                .send(Message::Text(json))
                .map_err(|e| format!("Failed to send message: {}", e))?;

            debug!(
                connection_id = %connection_id,
                message_type = ?message,
                "Message sent to connection"
            );
            Ok(())
        } else {
            Err(format!("Connection {} not found", connection_id))
        }
    }

    /// Send a message to all connections of a user
    pub async fn send_to_user(
        &self,
        user_id: &str,
        message: WebSocketMessage,
    ) -> Result<usize, String> {
        let connection_ids = self.get_user_connections(user_id).await;
        let mut sent_count = 0;

        for conn_id in connection_ids {
            if self
                .send_to_connection(&conn_id, message.clone())
                .await
                .is_ok()
            {
                sent_count += 1;
            }
        }

        if sent_count > 0 {
            debug!(
                user_id = %user_id,
                sent_count = sent_count,
                "Message sent to user connections"
            );
            Ok(sent_count)
        } else {
            Err(format!("No active connections for user {}", user_id))
        }
    }

    /// Broadcast a message to multiple users
    pub async fn broadcast_to_users(&self, user_ids: &[String], message: WebSocketMessage) {
        for user_id in user_ids {
            if let Err(e) = self.send_to_user(user_id, message.clone()).await {
                warn!(
                    user_id = %user_id,
                    error = %e,
                    "Failed to send message to user"
                );
            }
        }
    }

    /// Remove stale connections (no heartbeat for more than 60 seconds)
    pub async fn cleanup_stale_connections(&self) {
        let now = chrono::Utc::now();
        let timeout = chrono::Duration::seconds(60);

        let stale_connections: Vec<String> = self
            .connection_info
            .read()
            .await
            .iter()
            .filter(|(_, conn)| now.signed_duration_since(conn.last_heartbeat) > timeout)
            .map(|(id, _)| id.clone())
            .collect();

        for conn_id in stale_connections {
            warn!(connection_id = %conn_id, "Removing stale connection");
            self.unregister(&conn_id).await;
        }
    }

    /// Get connection count for a user
    pub async fn get_user_connection_count(&self, user_id: &str) -> usize {
        self.user_connections
            .read()
            .await
            .get(user_id)
            .map(|conns| conns.len())
            .unwrap_or(0)
    }

    /// Get total connection count
    pub async fn get_total_connections(&self) -> usize {
        self.connections.read().await.len()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket service for managing WebSocket operations
pub struct WebSocketService {
    connection_manager: ConnectionManager,
}

impl WebSocketService {
    pub fn new(connection_manager: ConnectionManager) -> Self {
        Self { connection_manager }
    }

    pub fn connection_manager(&self) -> &ConnectionManager {
        &self.connection_manager
    }

    /// Start background task for cleaning up stale connections
    pub fn start_cleanup_task(connection_manager: ConnectionManager) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                connection_manager.cleanup_stale_connections().await;
            }
        });
    }

    /// Handle ping message and respond with pong
    pub async fn handle_ping(&self, connection_id: &str) -> Result<(), String> {
        self.connection_manager
            .update_heartbeat(connection_id)
            .await;
        self.connection_manager
            .send_to_connection(connection_id, WebSocketMessage::Pong)
            .await
    }

    /// Broadcast presence update to relevant users
    pub async fn broadcast_presence(
        &self,
        user_id: &str,
        status: PresenceStatus,
        relevant_users: Vec<String>,
    ) {
        let message = WebSocketMessage::PresenceUpdate {
            user_id: user_id.to_string(),
            status,
        };

        self.connection_manager
            .broadcast_to_users(&relevant_users, message)
            .await;
    }

    /// Get missed messages for a user (messages sent while offline)
    pub async fn get_missed_messages(
        &self,
        _user_id: &str,
        _since: chrono::DateTime<chrono::Utc>,
    ) -> Vec<crate::messages::models::EnhancedConversationMessage> {
        // This would query the database for messages created after the 'since' timestamp
        // For now, return empty vec - will be implemented in message_service
        Vec::new()
    }
}
