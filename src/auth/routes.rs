//! Authentication routes

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers;

/// Creates and returns the authentication router
///
/// # Routes
/// - `POST /api/auth/google` - Google OAuth authentication
/// - `POST /api/auth/logout` - Logout (client-side token removal)
/// - `GET /api/me` - Get current user information
pub fn auth_routes() -> Router {
    Router::new()
        .route("/api/auth/google", post(handlers::google_auth))
        .route("/auth/google", get(handlers::google_oauth_start))
        .route("/auth/google/callback", get(handlers::google_oauth_callback))
        .route("/api/auth/logout", post(handlers::logout_handler))
        .route("/api/me", get(handlers::me_handler))
}
