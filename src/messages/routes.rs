use crate::messages::handlers;
use axum::{
    routing::{get, post},
    Router,
};

pub fn messages_routes() -> Router {
    Router::new()
        // WebSocket route
        .route(
            "/ws/conversations",
            get(handlers::websocket::websocket_handler),
        )
        // REST API routes (backward compatibility)
        .route(
            "/api/conversations",
            get(handlers::user::list_conversations).post(handlers::user::user_send_conversation),
        )
        // Alias for /api/messages (CLI compatibility)
        .route(
            "/api/messages",
            get(handlers::user::list_conversations).post(handlers::user::user_send_conversation),
        )
        .route(
            "/api/admin/conversations/:user_id",
            get(handlers::admin::admin_list_conversations)
                .post(handlers::admin::admin_send_conversation),
        )
        .route(
            "/api/admin/conversations",
            get(handlers::admin::admin_get_all_conversations),
        )
        // Mark conversation as read (admin)
        .route(
            "/api/admin/conversations/:user_id/read",
            post(handlers::admin::admin_mark_conversation_read),
        )
        // Mark conversation as read (user)
        .route(
            "/api/conversations/read",
            post(handlers::user::mark_conversation_read),
        )
        // Attachment serving route
        .route(
            "/api/attachments/:filename",
            get(handlers::user::serve_attachment),
        )
}
