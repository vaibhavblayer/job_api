#[cfg(test)]
mod tests {
    use crate::messages::models::*;
    use crate::messages::services::*;
    use crate::messages::validators;

    #[test]
    fn test_validate_message_content() {
        // Valid message
        assert!(validators::validate_message_content("Hello, world!").is_ok());

        // Empty message
        assert!(validators::validate_message_content("").is_err());
        assert!(validators::validate_message_content("   ").is_err());

        // Too long message
        let long_message = "a".repeat(10001);
        assert!(validators::validate_message_content(&long_message).is_err());
    }

    #[test]
    fn test_validate_attachment() {
        // Valid attachment
        assert!(validators::validate_attachment("test.pdf", "application/pdf", 1024).is_ok());

        // Empty filename
        assert!(validators::validate_attachment("", "application/pdf", 1024).is_err());

        // File too large
        assert!(
            validators::validate_attachment("test.pdf", "application/pdf", 11 * 1024 * 1024)
                .is_err()
        );

        // Empty file
        assert!(validators::validate_attachment("test.pdf", "application/pdf", 0).is_err());

        // Invalid file type
        assert!(
            validators::validate_attachment("test.exe", "application/x-msdownload", 1024).is_err()
        );
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(validators::sanitize_filename("test.pdf"), "test.pdf");
        assert_eq!(
            validators::sanitize_filename("../../../etc/passwd"),
            "etcpasswd"
        );
        assert_eq!(
            validators::sanitize_filename("test file.pdf"),
            "testfile.pdf"
        );
        assert_eq!(
            validators::sanitize_filename("test@#$%file.pdf"),
            "testfile.pdf"
        );
    }

    #[test]
    fn test_websocket_message_serialization() {
        // Test SendMessage serialization
        let msg = WebSocketMessage::SendMessage {
            content: "Hello".to_string(),
            conversation_id: Some("conv123".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("send_message"));
        assert!(json.contains("Hello"));

        // Test Ping/Pong
        let ping = WebSocketMessage::Ping;
        let ping_json = serde_json::to_string(&ping).unwrap();
        assert!(ping_json.contains("ping"));

        let pong = WebSocketMessage::Pong;
        let pong_json = serde_json::to_string(&pong).unwrap();
        assert!(pong_json.contains("pong"));
    }

    #[test]
    fn test_presence_status() {
        let online = PresenceStatus::Online;
        let json = serde_json::to_string(&online).unwrap();
        assert_eq!(json, "\"online\"");

        let offline = PresenceStatus::Offline;
        let json = serde_json::to_string(&offline).unwrap();
        assert_eq!(json, "\"offline\"");
    }

    #[tokio::test]
    async fn test_connection_manager() {
        let manager = ConnectionManager::new();

        // Initially no connections
        assert_eq!(manager.get_total_connections().await, 0);
        assert!(!manager.is_user_online("user1").await);

        // Register a connection
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        manager
            .register("user1".to_string(), "conn1".to_string(), tx)
            .await;

        // Check connection exists
        assert_eq!(manager.get_total_connections().await, 1);
        assert!(manager.is_user_online("user1").await);
        assert_eq!(manager.get_user_connection_count("user1").await, 1);

        // Unregister connection
        manager.unregister("conn1").await;

        // Check connection removed
        assert_eq!(manager.get_total_connections().await, 0);
        assert!(!manager.is_user_online("user1").await);
    }

    #[tokio::test]
    async fn test_connection_manager_multiple_connections() {
        let manager = ConnectionManager::new();

        // Register multiple connections for same user
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();

        manager
            .register("user1".to_string(), "conn1".to_string(), tx1)
            .await;
        manager
            .register("user1".to_string(), "conn2".to_string(), tx2)
            .await;

        // Check both connections exist
        assert_eq!(manager.get_user_connection_count("user1").await, 2);
        assert_eq!(manager.get_total_connections().await, 2);

        // Unregister one connection
        manager.unregister("conn1").await;

        // User should still be online with one connection
        assert!(manager.is_user_online("user1").await);
        assert_eq!(manager.get_user_connection_count("user1").await, 1);

        // Unregister second connection
        manager.unregister("conn2").await;

        // User should now be offline
        assert!(!manager.is_user_online("user1").await);
    }

    #[tokio::test]
    async fn test_presence_service() {
        let manager = ConnectionManager::new();
        let presence = PresenceService::new(manager.clone());

        // Initially offline
        assert_eq!(presence.get_status("user1").await, PresenceStatus::Offline);

        // Register connection
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        manager
            .register("user1".to_string(), "conn1".to_string(), tx)
            .await;

        // Should be online
        assert_eq!(presence.get_status("user1").await, PresenceStatus::Online);
        assert!(presence.is_online("user1").await);

        // Update last seen
        presence.update_last_seen("user1").await;
        let last_seen = presence.get_last_seen("user1").await;
        assert!(last_seen.is_some());

        // Unregister connection
        manager.unregister("conn1").await;

        // Should be offline
        assert_eq!(presence.get_status("user1").await, PresenceStatus::Offline);
        assert!(!presence.is_online("user1").await);
    }
}
