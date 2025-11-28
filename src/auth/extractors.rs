//! Authentication extractors for Axum

use async_trait::async_trait;
use axum::{
    extract::{Extension, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use super::models::{Claims, User};
use crate::common::{safe_email_log, ApiError, AppState};

/// Authenticated user extractor
///
/// This extractor validates JWT tokens and loads user information from the database.
/// It also checks if the user has admin privileges based on the admin_emails list.
#[derive(Debug)]
pub struct AuthedUser {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthedUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract the Extension containing the AppState
        let Extension(state_lock): Extension<Arc<RwLock<AppState>>> =
            Extension::from_request_parts(parts, state)
                .await
                .map_err(|_| ApiError::InternalServer("missing app state".to_string()))?;

        let app_state = state_lock.read().await.clone();

        // DEV MODE: Bypass authentication completely
        if app_state.dev_mode.is_enabled() {
            let dev_user = app_state.dev_mode.create_dev_user();
            let is_admin = app_state.dev_mode.user_is_admin || 
                          app_state.admin_emails.contains(&dev_user.email.to_lowercase());
            
            debug!(
                user_id = %dev_user.id,
                email = %safe_email_log(&dev_user.email),
                is_admin = is_admin,
                "DEV MODE: Authentication bypassed"
            );
            
            return Ok(AuthedUser {
                id: dev_user.id,
                email: dev_user.email,
                is_admin,
            });
        }

        // Extract Bearer token from Authorization header
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let token = match token {
            Some(t) => t,
            None => {
                warn!("Authentication failed: missing Authorization header");
                return Err(ApiError::Unauthorized("missing auth".into()));
            }
        };

        // Handle "Bearer <token>" format or raw token
        let bare_token = if let Some(rest) = token.strip_prefix("Bearer ") {
            rest.to_string()
        } else {
            token
        };

        // Validate JWT token
        let decoded = match decode::<Claims>(
            &bare_token,
            &DecodingKey::from_secret(app_state.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        ) {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "JWT token validation failed");
                return Err(ApiError::Unauthorized("invalid token".into()));
            }
        };

        let user_id = decoded.claims.sub;

        // Look up user in database
        let user: Option<User> = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_optional(&app_state.db)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    user_id = %user_id,
                    "Database error during user lookup in authentication"
                );
                ApiError::DatabaseError(e)
            })?;

        match user {
            Some(u) => {
                let user_email_lower = u.email.to_lowercase();
                let is_admin = app_state.admin_emails.contains(&user_email_lower);
                debug!(
                    user_id = %u.id,
                    email = %safe_email_log(&u.email),
                    email_lower = %user_email_lower,
                    admin_emails = ?app_state.admin_emails,
                    is_admin = is_admin,
                    "User authentication successful via extractor"
                );
                Ok(AuthedUser {
                    id: u.id,
                    email: u.email,
                    is_admin,
                })
            }
            None => {
                warn!(user_id = %user_id, "Authentication failed: user not found in database");
                Err(ApiError::Unauthorized("user not found".into()))
            }
        }
    }
}
