// src/candidates/handlers/youtube_videos.rs
//! YouTube video integration handlers

use axum::{
    extract::{Extension, Query},
    response::{Html, Redirect},
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::{
    auth::AuthedUser,
    candidates::models::{VideoSubmission, YouTubeVideoLinkRequest},
    common::{generate_token_id, generate_video_id, ApiError, AppState},
    services::youtube::{YouTubeService, YouTubeVideo},
};

#[derive(Debug, Deserialize)]
pub struct ListYouTubeVideosQuery {
    max_results: Option<u32>,
}

/// GET /api/user/youtube/videos - List user's YouTube videos
pub async fn list_youtube_videos(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(query): Query<ListYouTubeVideosQuery>,
) -> Result<Json<Vec<YouTubeVideo>>, ApiError> {
    debug!(
        user_id = %authed.id,
        "Listing YouTube videos for user"
    );

    let state = state_lock.read().await;
    
    // Get user's YouTube access token
    let access_token = get_user_youtube_token(&state.db, &authed.id).await?;
    
    let youtube_service = YouTubeService::new(state.settings_service.clone());
    let max_results = query.max_results.unwrap_or(50);
    
    match youtube_service.get_user_videos(&access_token, max_results).await {
        Ok(videos) => {
            info!(
                user_id = %authed.id,
                video_count = videos.len(),
                "Successfully fetched YouTube videos"
            );
            Ok(Json(videos))
        }
        Err(e) => {
            let error_str = e.to_string();
            error!(
                user_id = %authed.id,
                error = %error_str,
                "Failed to fetch YouTube videos"
            );
            
            // Check if token is expired/invalid
            if error_str.contains("401") || error_str.contains("invalid") || error_str.contains("expired") {
                // Try to refresh the token
                if let Ok(new_token) = refresh_user_youtube_token(&state, &authed.id).await {
                    // Retry with new token
                    match youtube_service.get_user_videos(&new_token, max_results).await {
                        Ok(videos) => {
                            info!(
                                user_id = %authed.id,
                                video_count = videos.len(),
                                "Successfully fetched YouTube videos after token refresh"
                            );
                            return Ok(Json(videos));
                        }
                        Err(e2) => {
                            error!(
                                user_id = %authed.id,
                                error = %e2,
                                "Failed to fetch YouTube videos even after token refresh"
                            );
                        }
                    }
                }
                
                // Token refresh failed, user needs to re-authorize
                return Err(ApiError::BadRequest(
                    "YouTube authorization expired. Please re-authorize YouTube access.".to_string(),
                ));
            }
            
            Err(ApiError::InternalServer(format!(
                "Failed to fetch YouTube videos: {}",
                e
            )))
        }
    }
}

/// POST /api/user/videos/youtube - Link a YouTube video to user profile
pub async fn link_youtube_video(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<YouTubeVideoLinkRequest>,
) -> Result<Json<VideoSubmission>, ApiError> {
    debug!(
        user_id = %authed.id,
        youtube_video_id = %request.youtube_video_id,
        "Linking YouTube video to user profile"
    );

    let state = state_lock.read().await;
    
    // Check if user already has maximum number of videos
    let existing_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM videos WHERE user_id = ?"
    )
    .bind(&authed.id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error counting user videos");
        ApiError::DatabaseError(e)
    })?;

    if existing_count >= 2 {
        warn!(
            user_id = %authed.id,
            existing_count = existing_count,
            "User has reached maximum video limit"
        );
        return Err(ApiError::BadRequest(
            "Maximum of 2 videos allowed per user".to_string(),
        ));
    }

    // Check if this YouTube video is already linked
    let existing_video = sqlx::query_scalar::<_, Option<String>>(
        "SELECT id FROM videos WHERE youtube_video_id = ?"
    )
    .bind(&request.youtube_video_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error checking existing YouTube video");
        ApiError::DatabaseError(e)
    })?;

    if existing_video.is_some() {
        return Err(ApiError::BadRequest(
            "This YouTube video is already linked to a profile".to_string(),
        ));
    }

    // Get YouTube video details
    let access_token = get_user_youtube_token(&state.db, &authed.id).await?;
    let youtube_service = YouTubeService::new(state.settings_service.clone());
    
    let youtube_video = youtube_service
        .get_video_details(&request.youtube_video_id, &access_token)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                youtube_video_id = %request.youtube_video_id,
                "Failed to fetch YouTube video details"
            );
            ApiError::BadRequest(format!("Invalid YouTube video: {}", e))
        })?;

    // Parse duration to seconds
    let duration_seconds = parse_duration_to_seconds(&youtube_video.duration);

    // Create video record
    let video_id = generate_video_id();
    sqlx::query(
        r#"
        INSERT INTO videos (
            id, user_id, duration_seconds, uploaded_at, video_source,
            youtube_video_id, youtube_thumbnail_url, youtube_title, youtube_description
        ) VALUES (?, ?, ?, datetime('now'), 'youtube', ?, ?, ?, ?)
        "#,
    )
    .bind(&video_id)
    .bind(&authed.id)
    .bind(duration_seconds)
    .bind(&request.youtube_video_id)
    .bind(&youtube_video.thumbnail_url)
    .bind(request.title.as_deref().unwrap_or(&youtube_video.title))
    .bind(request.description.as_deref().unwrap_or(&youtube_video.description))
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error creating YouTube video record");
        ApiError::DatabaseError(e)
    })?;

    // Fetch the created video
    let video = sqlx::query_as::<_, VideoSubmission>(
        "SELECT * FROM videos WHERE id = ?"
    )
    .bind(&video_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching created video");
        ApiError::DatabaseError(e)
    })?;

    info!(
        user_id = %authed.id,
        video_id = %video_id,
        youtube_video_id = %request.youtube_video_id,
        "Successfully linked YouTube video"
    );

    Ok(Json(video))
}

/// GET /api/auth/youtube - Start YouTube OAuth flow for user
/// Accepts token as query parameter since this is a redirect endpoint
pub async fn youtube_oauth_start(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Redirect, ApiError> {
    let state = state_lock.read().await;
    
    // Get user_id from token in query parameter
    let token = params.get("token").ok_or_else(|| {
        ApiError::Unauthorized("Authentication required. Please provide a token.".to_string())
    })?;
    
    // Validate the JWT token
    let claims = crate::auth::handlers::validate_jwt(token)?;
    let user_id = claims.sub;
    
    info!(user_id = %user_id, "Starting YouTube OAuth flow for user");
    
    // Get Google client credentials from settings or env
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| ApiError::InternalServer("Google OAuth not configured".to_string()))?;
    
    // Get backend URL from env for redirect
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let redirect_uri = format!("{}/api/auth/youtube/callback", backend_url);
    
    // YouTube readonly scope
    let scopes = vec![
        "https://www.googleapis.com/auth/youtube.readonly",
    ];
    let scope_param = scopes.join(" ");
    
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&scope_param)
    );
    
    // Store user_id in a temporary state parameter for the callback
    let auth_url_with_state = format!("{}&state={}", auth_url, user_id);
    
    info!(user_id = %user_id, "Redirecting to Google OAuth for YouTube");
    Ok(Redirect::to(&auth_url_with_state))
}

/// GET /api/auth/youtube/callback - Handle YouTube OAuth callback
pub async fn youtube_oauth_callback(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Html<String>, ApiError> {
    let state = state_lock.read().await;
    
    // Check for error from Google
    if let Some(error) = params.get("error") {
        error!(oauth_error = %error, "YouTube OAuth returned error");
        let frontend_url = std::env::var("FRONTEND_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        return Ok(Html(format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head><title>YouTube Authorization Failed</title></head>
            <body>
                <h1>❌ Authorization Failed</h1>
                <p>Error: {}</p>
                <script>
                    setTimeout(function() {{
                        window.location.href = '{}/dashboard/videos';
                    }}, 3000);
                </script>
            </body>
            </html>
            "#,
            error, frontend_url
        )));
    }
    
    // Get authorization code and user_id from state
    let code = params.get("code").ok_or_else(|| {
        error!("No authorization code in YouTube OAuth callback");
        ApiError::BadRequest("No authorization code provided".to_string())
    })?;
    
    let user_id = params.get("state").ok_or_else(|| {
        error!("No state (user_id) in YouTube OAuth callback");
        ApiError::BadRequest("Invalid OAuth state".to_string())
    })?;
    
    info!(user_id = %user_id, "Received YouTube OAuth callback");
    
    // Exchange code for tokens
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| ApiError::InternalServer("Google OAuth not configured".to_string()))?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| ApiError::InternalServer("Google OAuth not configured".to_string()))?;
    
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let redirect_uri = format!("{}/api/auth/youtube/callback", backend_url);
    
    let client = reqwest::Client::new();
    let token_response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to exchange YouTube OAuth code");
            ApiError::InternalServer("Failed to exchange authorization code".to_string())
        })?;
    
    if !token_response.status().is_success() {
        let error_text = token_response.text().await.unwrap_or_default();
        error!(error = %error_text, "YouTube token exchange failed");
        return Err(ApiError::InternalServer(format!("Token exchange failed: {}", error_text)));
    }
    
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: i64,
    }
    
    let tokens: TokenResponse = token_response.json().await.map_err(|e| {
        error!(error = %e, "Failed to parse YouTube token response");
        ApiError::InternalServer("Failed to parse token response".to_string())
    })?;
    
    // Calculate expiration time
    let expires_at = Utc::now() + Duration::seconds(tokens.expires_in);
    
    // Store tokens in database
    let token_id = generate_token_id();
    sqlx::query(
        r#"
        INSERT INTO user_oauth_tokens (id, user_id, provider, access_token, refresh_token, token_expires_at, scopes, updated_at)
        VALUES (?, ?, 'youtube', ?, ?, ?, 'youtube.readonly', datetime('now'))
        ON CONFLICT(user_id, provider) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = COALESCE(excluded.refresh_token, user_oauth_tokens.refresh_token),
            token_expires_at = excluded.token_expires_at,
            updated_at = datetime('now')
        "#,
    )
    .bind(&token_id)
    .bind(user_id)
    .bind(&tokens.access_token)
    .bind(&tokens.refresh_token)
    .bind(expires_at.to_rfc3339())
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to store YouTube tokens");
        ApiError::DatabaseError(e)
    })?;
    
    info!(user_id = %user_id, "Successfully stored YouTube OAuth tokens");
    
    let frontend_url = std::env::var("FRONTEND_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    
    // Redirect back to the videos page
    Ok(Html(format!(r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>YouTube Authorization Successful</title>
            <style>
                body {{ font-family: Arial, sans-serif; text-align: center; padding: 50px; }}
                .success {{ color: #4CAF50; }}
            </style>
        </head>
        <body>
            <h1 class="success">✅ YouTube Authorization Successful!</h1>
            <p>You can now access your YouTube videos.</p>
            <p>Redirecting...</p>
            <script>
                setTimeout(function() {{
                    window.location.href = '{}/dashboard/videos';
                }}, 2000);
            </script>
        </body>
        </html>
    "#, frontend_url)))
}

/// Helper function to get user's YouTube access token
async fn get_user_youtube_token(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<String, ApiError> {
    #[derive(sqlx::FromRow)]
    struct UserToken {
        access_token: String,
        token_expires_at: Option<String>,
    }
    
    let token = sqlx::query_as::<_, UserToken>(
        "SELECT access_token, token_expires_at FROM user_oauth_tokens WHERE user_id = ? AND provider = 'youtube'"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, user_id = %user_id, "Database error fetching YouTube token");
        ApiError::DatabaseError(e)
    })?;
    
    match token {
        Some(t) => {
            // Check if token is expired
            if let Some(expires_at) = t.token_expires_at {
                if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&expires_at) {
                    if exp < Utc::now() {
                        warn!(user_id = %user_id, "YouTube token expired");
                        return Err(ApiError::BadRequest(
                            "YouTube authorization expired. Please re-authorize.".to_string(),
                        ));
                    }
                }
            }
            Ok(t.access_token)
        }
        None => {
            debug!(user_id = %user_id, "No YouTube token found for user");
            Err(ApiError::BadRequest(
                "YouTube integration not configured. Please authorize YouTube access first.".to_string(),
            ))
        }
    }
}

/// Helper function to refresh user's YouTube token
async fn refresh_user_youtube_token(
    state: &AppState,
    user_id: &str,
) -> Result<String, ApiError> {
    #[derive(sqlx::FromRow)]
    struct RefreshToken {
        refresh_token: Option<String>,
    }
    
    let token = sqlx::query_as::<_, RefreshToken>(
        "SELECT refresh_token FROM user_oauth_tokens WHERE user_id = ? AND provider = 'youtube'"
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::DatabaseError(e))?;
    
    let refresh_token = token
        .and_then(|t| t.refresh_token)
        .ok_or_else(|| ApiError::BadRequest("No refresh token available".to_string()))?;
    
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .map_err(|_| ApiError::InternalServer("Google OAuth not configured".to_string()))?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .map_err(|_| ApiError::InternalServer("Google OAuth not configured".to_string()))?;
    
    let client = reqwest::Client::new();
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| ApiError::InternalServer(format!("Token refresh failed: {}", e)))?;
    
    if !response.status().is_success() {
        return Err(ApiError::BadRequest("Token refresh failed".to_string()));
    }
    
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        expires_in: i64,
    }
    
    let tokens: TokenResponse = response.json().await
        .map_err(|e| ApiError::InternalServer(format!("Failed to parse token response: {}", e)))?;
    
    let expires_at = Utc::now() + Duration::seconds(tokens.expires_in);
    
    // Update token in database
    sqlx::query(
        "UPDATE user_oauth_tokens SET access_token = ?, token_expires_at = ?, updated_at = datetime('now') WHERE user_id = ? AND provider = 'youtube'"
    )
    .bind(&tokens.access_token)
    .bind(expires_at.to_rfc3339())
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::DatabaseError(e))?;
    
    info!(user_id = %user_id, "Successfully refreshed YouTube token");
    Ok(tokens.access_token)
}

/// Parse duration string to seconds
fn parse_duration_to_seconds(duration: &str) -> i32 {
    // Parse formats like "5:30" or "1:02:10"
    let parts: Vec<&str> = duration.split(':').collect();
    
    match parts.len() {
        1 => parts[0].parse().unwrap_or(0),
        2 => {
            let minutes: i32 = parts[0].parse().unwrap_or(0);
            let seconds: i32 = parts[1].parse().unwrap_or(0);
            minutes * 60 + seconds
        }
        3 => {
            let hours: i32 = parts[0].parse().unwrap_or(0);
            let minutes: i32 = parts[1].parse().unwrap_or(0);
            let seconds: i32 = parts[2].parse().unwrap_or(0);
            hours * 3600 + minutes * 60 + seconds
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_to_seconds() {
        assert_eq!(parse_duration_to_seconds("5:30"), 330);
        assert_eq!(parse_duration_to_seconds("1:02:10"), 3730);
        assert_eq!(parse_duration_to_seconds("45"), 45);
        assert_eq!(parse_duration_to_seconds("0:45"), 45);
    }
}
