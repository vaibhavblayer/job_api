//! Authentication handlers

use axum::extract::{Extension, Json};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::extractors::AuthedUser;
use super::models::{Claims, GoogleIdTokenPayload, User};
use crate::common::{generate_raw_id, generate_user_id, safe_email_log, ApiError, AppState};
use jsonwebtoken::{decode, DecodingKey, Validation};

/// POST /api/auth/google
/// Authenticates a user via Google OAuth ID token
///
/// # Request Body
/// ```json
/// {
///   "id_token": "<google id token>"
/// }
/// ```
///
/// # Response
/// ```json
/// {
///   "token": "<jwt token>",
///   "user": { ... }
/// }
/// ```
pub async fn google_auth(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(payload): Json<GoogleIdTokenPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("🔐 Received Google auth request");
    let state = state_lock.read().await.clone();

    // Verify token with Google's tokeninfo endpoint
    // Docs: https://developers.google.com/identity/sign-in/web/backend-auth
    let tokeninfo_url = format!(
        "https://oauth2.googleapis.com/tokeninfo?id_token={}",
        payload.id_token
    );

    debug!("Initiating Google token validation with tokeninfo endpoint");

    let resp = state.http.get(&tokeninfo_url).send().await;
    let body = match resp {
        Ok(r) => {
            let status = r.status();
            debug!(http_status = %status, "Received response from Google tokeninfo endpoint");

            if status.is_success() {
                match r.json::<serde_json::Value>().await {
                    Ok(j) => {
                        debug!("Successfully parsed Google tokeninfo JSON response");
                        j
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            "Failed to parse Google tokeninfo JSON response - malformed token"
                        );
                        return Err(ApiError::BadRequest("malformed id_token".to_string()));
                    }
                }
            } else {
                // Handle specific HTTP error codes from Google
                match status.as_u16() {
                    400 => {
                        warn!(
                            http_status = %status,
                            "Google tokeninfo returned 400 - invalid or malformed token"
                        );
                        return Err(ApiError::BadRequest(
                            "invalid or malformed id_token".to_string(),
                        ));
                    }
                    401 => {
                        warn!(
                            http_status = %status,
                            "Google tokeninfo returned 401 - expired or invalid token"
                        );
                        return Err(ApiError::Unauthorized(
                            "expired or invalid id_token".to_string(),
                        ));
                    }
                    _ => {
                        warn!(
                            http_status = %status,
                            "Google tokeninfo returned error status"
                        );
                        return Err(ApiError::BadRequest(
                            "id_token validation failed".to_string(),
                        ));
                    }
                }
            }
        }
        Err(e) => {
            error!(
                error = %e,
                endpoint = "https://oauth2.googleapis.com/tokeninfo",
                "HTTP error contacting Google tokeninfo endpoint"
            );
            return Err(ApiError::InternalServer(
                "google token validation service unavailable".to_string(),
            ));
        }
    };

    // Extract required fields: email, sub, email_verified
    debug!("Extracting user information from Google token payload");

    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let sub = body.get("sub").and_then(|v| v.as_str()).map(str::to_string);
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let picture = body
        .get("picture")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // Validate required fields are present
    if email.is_none() || sub.is_none() {
        warn!(
            has_email = email.is_some(),
            has_sub = sub.is_some(),
            "Google token missing required fields (email/sub)"
        );
        return Err(ApiError::BadRequest(
            "token missing required fields".to_string(),
        ));
    }

    // Check if email is verified (optional but recommended)
    if let Some(email_verified) = body.get("email_verified").and_then(|v| v.as_bool()) {
        if !email_verified {
            warn!("Google token contains unverified email address");
        } else {
            debug!("Google token email verification confirmed");
        }
    }

    // Check token expiration
    if let Some(exp) = body.get("exp").and_then(|v| v.as_i64()) {
        let current_time = Utc::now().timestamp();
        if exp < current_time {
            warn!(
                token_exp = exp,
                current_time = current_time,
                "Google token has expired"
            );
            return Err(ApiError::Unauthorized("token has expired".to_string()));
        }
        debug!(
            token_exp = exp,
            current_time = current_time,
            "Google token expiration validation successful"
        );
    }

    // Validate audience (client id) when configured
    if let Some(client_id) = &state.google_client_id {
        match body.get("aud").and_then(|v| v.as_str()) {
            Some(aud_val) => {
                if aud_val != client_id {
                    warn!(
                        token_audience = %aud_val,
                        expected_client_id = %client_id,
                        "Google token audience validation failed - rejecting token"
                    );
                    return Err(ApiError::Unauthorized(
                        "token audience mismatch".to_string(),
                    ));
                }
                debug!(
                    token_audience = %aud_val,
                    expected_client_id = %client_id,
                    "Google token audience validation successful"
                );
            }
            None => {
                warn!(
                    expected_client_id = %client_id,
                    "Google token missing audience field - rejecting token"
                );
                return Err(ApiError::Unauthorized("token missing audience".to_string()));
            }
        }
    }

    let email = email.unwrap();
    let sub = sub.unwrap();

    debug!(
        email = %safe_email_log(&email),
        provider = "google",
        provider_id = %sub,
        "Google token validation successful, proceeding with user lookup"
    );

    // Create or find user in DB
    let existing: Option<User> = match sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE provider = ? AND provider_id = ?",
    )
    .bind("google")
    .bind(&sub)
    .fetch_optional(&state.db)
    .await
    {
        Ok(row) => {
            if row.is_some() {
                debug!(
                    provider = "google",
                    provider_id = %sub,
                    "Found existing user in database"
                );
            } else {
                debug!(
                    provider = "google",
                    provider_id = %sub,
                    "No existing user found, will create new user"
                );
            }
            row
        }
        Err(e) => {
            error!(
                error = %e,
                provider = "google",
                provider_id = %sub,
                "Database error checking existing user during OAuth flow"
            );
            return Err(ApiError::DatabaseError(e));
        }
    };

    let user = match existing {
        Some(mut u) => {
            // For existing users, download and store avatar if we don't have one locally
            if let Some(picture_url) = &picture {
                if u.avatar
                    .as_ref()
                    .map_or(true, |avatar| avatar.starts_with("http"))
                {
                    // Only download if we don't have a local avatar or current avatar is external
                    match download_and_store_avatar(&state, &u.id, picture_url).await {
                        Ok(local_url) => {
                            u.avatar = Some(local_url.clone());
                            info!(user_id = %u.id, "Avatar downloaded and stored locally during login");
                            // Update database with local avatar URL
                            let _ = update_user_avatar(&state.db, &u.id, &local_url).await;
                        }
                        Err(e) => {
                            warn!(error = %e, user_id = %u.id, "Failed to download avatar during login, keeping existing");
                        }
                    }
                }
            }
            u
        }
        None => {
            let id = generate_user_id();
            info!(
                user_id = %id,
                email = %safe_email_log(&email),
                provider = "google",
                "Creating new user account via Google OAuth"
            );

            // Download and store avatar for new users
            let local_avatar = if let Some(picture_url) = &picture {
                match download_and_store_avatar(&state, &id, picture_url).await {
                    Ok(local_url) => {
                        info!(user_id = %id, "Avatar downloaded and stored locally during registration");
                        Some(local_url)
                    }
                    Err(e) => {
                        warn!(error = %e, user_id = %id, "Failed to download avatar during registration, will use external URL");
                        picture.clone()
                    }
                }
            } else {
                None
            };

            // insert
            if let Err(e) = sqlx::query(
                "INSERT OR IGNORE INTO users (id, email, name, avatar, provider, provider_id, avatar_updated_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now'))",
            )
            .bind(&id)
            .bind(&email)
            .bind(name.as_deref())
            .bind(local_avatar.as_deref())
            .bind("google")
            .bind(&sub)
            .execute(&state.db)
            .await
            {
                error!(
                    error = %e,
                    user_id = %id,
                    email = %safe_email_log(&email),
                    provider = "google",
                    "Database error inserting new user during OAuth flow"
                );
                return Err(ApiError::DatabaseError(e));
            }

            debug!(
                user_id = %id,
                "Successfully inserted new user, fetching user record"
            );

            // fetch back
            match sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
                .bind(&id)
                .fetch_one(&state.db)
                .await
            {
                Ok(row) => {
                    info!(
                        user_id = %id,
                        email = %safe_email_log(&email),
                        "New user account created successfully via Google OAuth"
                    );
                    row
                }
                Err(e) => {
                    error!(
                        error = %e,
                        user_id = %id,
                        "Database error fetching newly created user during OAuth flow"
                    );
                    return Err(ApiError::DatabaseError(e));
                }
            }
        }
    };

    // Update profile status to pending - this is not critical for OAuth flow
    // so we log errors but don't fail the authentication
    if let Err(e) = update_profile_status(&state.db, &user.id, "pending", None).await {
        error!(error = %e, user_id = %user.id, "Failed to update profile status to pending");
    }

    // create JWT
    let exp = (Utc::now() + Duration::hours(24)).timestamp() as usize;
    let claims = Claims {
        sub: user.id.clone(),
        exp,
    };
    let token = match encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!(
                error = %e,
                user_id = %user.id,
                "JWT encoding error during authentication"
            );
            return Err(ApiError::InternalServer("jwt error".to_string()));
        }
    };

    info!(
        user_id = %user.id,
        email = %safe_email_log(&user.email),
        provider = "google",
        "User authentication successful via Google OAuth"
    );

    // Check if user is admin
    let is_admin = state.admin_emails.contains(&user.email);

    let resp = serde_json::json!({
        "token": token,
        "user": {
            "id": user.id,
            "email": user.email,
            "name": user.name,
            "avatar": user.avatar,
            "is_admin": is_admin,
        },
    });

    Ok(Json(resp))
}

/// GET /api/me
/// Returns the current authenticated user's information
///
/// # Response
/// ```json
/// {
///   "user": { ... },
///   "is_admin": true
/// }
/// ```
#[axum::debug_handler]
pub async fn me_handler(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();
    
    // In dev mode, return the dev user directly without database lookup
    if state.dev_mode.is_enabled() {
        let dev_user = state.dev_mode.create_dev_user();
        let resp = serde_json::json!({
            "user": dev_user,
            "is_admin": authed.is_admin
        });
        return Ok(Json(resp));
    }
    
    // Production mode: fetch user from database
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&authed.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    let resp = serde_json::json!({
        "user": user,
        "is_admin": authed.is_admin
    });
    Ok(Json(resp))
}

/// POST /api/auth/logout
/// Logout endpoint - since we're using JWT tokens, logout is handled client-side
/// This endpoint just returns success to confirm the logout request
///
/// # Response
/// ```json
/// {
///   "message": "Logout successful"
/// }
/// ```
pub async fn logout_handler(_authed: AuthedUser) -> Result<Json<serde_json::Value>, ApiError> {
    info!("User logout successful");
    let resp = serde_json::json!({
        "message": "Logout successful"
    });
    Ok(Json(resp))
}

// ---- Helper Functions ----

/// Validate a JWT token and return the claims
/// This is used by the WebSocket handler for authentication
pub fn validate_jwt(token: &str) -> Result<Claims, ApiError> {
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "default_secret_key_change_in_production".to_string());

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| {
        warn!(error = %e, "JWT validation failed");
        ApiError::Unauthorized("Invalid token".to_string())
    })?;

    Ok(token_data.claims)
}

async fn download_and_store_avatar(
    state: &AppState,
    user_id: &str,
    external_url: &str,
) -> Result<String, ApiError> {
    use infer::Infer;
    use tokio::fs as tokio_fs;

    info!(user_id = %user_id, external_url = %external_url, "Downloading avatar from external URL");

    // Download image from external URL
    let response = state.http.get(external_url).send().await.map_err(|e| {
        error!(error = %e, external_url = %external_url, "Failed to download avatar");
        ApiError::InternalServer("Failed to download avatar".to_string())
    })?;

    if !response.status().is_success() {
        return Err(ApiError::BadRequest(
            "Failed to download avatar from URL".to_string(),
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read avatar data".to_string()))?;

    // Validate file type
    let infer = Infer::new();
    let (is_valid, content_type) = if let Some(info) = infer.get(&bytes) {
        let mime = info.mime_type();
        (
            matches!(mime, "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/webp"),
            mime.to_string(),
        )
    } else {
        (false, "application/octet-stream".to_string())
    };

    if !is_valid {
        return Err(ApiError::BadRequest(
            "Downloaded file is not a valid image".to_string(),
        ));
    }

    // Generate filename
    let extension = get_extension_from_url(external_url).unwrap_or("jpg");
    let filename = format!("avatar_{}_{}.{}", user_id, generate_raw_id(8), extension);

    // Check storage type setting
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let avatar_url = if storage_type.starts_with("s3") {
        // Upload to S3
        let s3_key = format!("avatars/{}", filename);
        match state
            .aws_service
            .upload_file(bytes.to_vec(), &s3_key, &content_type)
            .await
        {
            Ok(url) => {
                info!(user_id = %user_id, s3_key = %s3_key, "Avatar uploaded to S3 successfully");
                url
            }
            Err(e) => {
                warn!(error = %e, user_id = %user_id, "Failed to upload avatar to S3, falling back to local storage");
                // Fall back to local storage
                let file_path = state.avatars_dir.join(&filename);
                tokio_fs::write(&file_path, &bytes).await.map_err(|e| {
                    error!(error = %e, file_path = %file_path.display(), "Failed to save avatar file locally");
                    ApiError::InternalServer("Failed to save avatar file".to_string())
                })?;
                format!("/api/avatars/{}", filename)
            }
        }
    } else {
        // Save to local storage
        let file_path = state.avatars_dir.join(&filename);
        tokio_fs::write(&file_path, &bytes).await.map_err(|e| {
            error!(error = %e, file_path = %file_path.display(), "Failed to save avatar file");
            ApiError::InternalServer("Failed to save avatar file".to_string())
        })?;
        format!("/api/avatars/{}", filename)
    };

    info!(user_id = %user_id, filename = %filename, storage_type = %storage_type, "Avatar file saved successfully");

    Ok(avatar_url)
}

async fn update_user_avatar(
    pool: &SqlitePool,
    user_id: &str,
    avatar_url: &str,
) -> Result<(), ApiError> {
    let filename = avatar_url.replace("/api/avatars/", "");

    sqlx::query(
        "UPDATE users SET avatar = ?, avatar_filename = ?, avatar_updated_at = datetime('now') WHERE id = ?"
    )
    .bind(avatar_url)
    .bind(&filename)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(())
}

async fn update_profile_status(
    pool: &SqlitePool,
    user_id: &str,
    status: &str,
    resume_id: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO profiles (user_id, resume_status, last_resume_id)
        VALUES (?, ?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            resume_status = excluded.resume_status,
            last_resume_id = excluded.last_resume_id,
            updated_at = datetime('now')
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(resume_id)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(
            error = %e,
            user_id = %user_id,
            status = %status,
            resume_id = ?resume_id,
            "Database error updating profile status"
        );
        ApiError::DatabaseError(e)
    })?;

    Ok(())
}

fn get_extension_from_url(url: &str) -> Option<&str> {
    url.split('?')
        .next()? // Remove query parameters
        .split('.')
        .last()
        .filter(|ext| matches!(*ext, "jpg" | "jpeg" | "png" | "gif" | "webp"))
}

/// GET /auth/google - Start Google OAuth flow
/// Redirects user to Google's authorization page
pub async fn google_oauth_start(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
) -> Result<axum::response::Redirect, ApiError> {
    let state = state_lock.read().await;
    
    // Get the redirect URI from environment or use default
    let redirect_uri = std::env::var("GOOGLE_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:8080/auth/google/callback".to_string());
    
    info!("Starting Google OAuth flow with redirect_uri: {}", redirect_uri);
    
    let auth_url = state
        .google_service
        .get_authorization_url(&redirect_uri)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate Google OAuth URL");
            ApiError::InternalServer(format!("Failed to generate OAuth URL: {}", e))
        })?;
    
    info!("Redirecting to Google OAuth: {}", auth_url);
    Ok(axum::response::Redirect::to(&auth_url))
}

/// GET /auth/google/callback - Handle OAuth callback from Google
/// Exchanges authorization code for tokens and stores refresh token
pub async fn google_oauth_callback(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Html<String>, ApiError> {
    let state = state_lock.read().await;
    
    // Check for error from Google
    if let Some(error) = params.get("error") {
        error!(oauth_error = %error, "Google OAuth returned error");
        return Ok(axum::response::Html(format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>OAuth Error</title>
                <style>
                    body {{ font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; }}
                    .error {{ background: #fee; border: 1px solid #fcc; padding: 20px; border-radius: 8px; }}
                    h1 {{ color: #c00; }}
                </style>
            </head>
            <body>
                <div class="error">
                    <h1>❌ Authorization Failed</h1>
                    <p>Error: {}</p>
                    <p><a href="/auth/google">Try again</a></p>
                </div>
            </body>
            </html>
            "#,
            error
        )));
    }
    
    // Get authorization code
    let code = params.get("code").ok_or_else(|| {
        error!("No authorization code in OAuth callback");
        ApiError::BadRequest("No authorization code provided".to_string())
    })?;
    
    info!("Received OAuth callback with authorization code");
    
    // Get the redirect URI (must match the one used in authorization)
    let redirect_uri = std::env::var("GOOGLE_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:8080/auth/google/callback".to_string());
    
    // Exchange code for tokens
    let token_response = state
        .google_service
        .exchange_code(code, &redirect_uri)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to exchange authorization code for tokens");
            ApiError::InternalServer(format!("Failed to exchange code: {}", e))
        })?;
    
    info!("Successfully exchanged code for tokens, refresh_token present: {}", token_response.refresh_token.is_some());
    
    // Get connected account email
    let connected_account = state
        .settings_service
        .get_setting("google_connected_account")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "Unknown".to_string());
    
    Ok(axum::response::Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>OAuth Success</title>
            <style>
                body {{ 
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                    max-width: 600px; 
                    margin: 50px auto; 
                    padding: 20px;
                    background: #f5f5f5;
                }}
                .success {{ 
                    background: white;
                    border: 2px solid #4CAF50; 
                    padding: 30px; 
                    border-radius: 12px;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
                }}
                h1 {{ 
                    color: #4CAF50;
                    margin-top: 0;
                }}
                .info {{
                    background: #e8f5e9;
                    padding: 15px;
                    border-radius: 8px;
                    margin: 20px 0;
                }}
                .scopes {{
                    background: #f5f5f5;
                    padding: 15px;
                    border-radius: 8px;
                    margin: 20px 0;
                }}
                .scopes ul {{
                    margin: 10px 0;
                    padding-left: 20px;
                }}
                .scopes li {{
                    margin: 5px 0;
                    color: #666;
                }}
                a {{
                    display: inline-block;
                    background: #667eea;
                    color: white;
                    padding: 12px 24px;
                    text-decoration: none;
                    border-radius: 6px;
                    margin-top: 20px;
                }}
                a:hover {{
                    background: #5568d3;
                }}
            </style>
        </head>
        <body>
            <div class="success">
                <h1>✅ Authorization Successful!</h1>
                <div class="info">
                    <p><strong>Connected Account:</strong> {}</p>
                    <p><strong>Status:</strong> Refresh token saved to database</p>
                </div>
                <div class="scopes">
                    <p><strong>Granted Permissions:</strong></p>
                    <ul>
                        <li>✅ Basic profile information</li>
                        <li>✅ Email address</li>
                        <li>✅ Google Calendar access</li>
                        <li>✅ Create Google Meet links</li>
                        <li>✅ YouTube readonly access</li>
                    </ul>
                </div>
                <p>You can now:</p>
                <ul>
                    <li>Schedule interviews with Google Meet links</li>
                    <li>Create calendar events automatically</li>
                    <li>Access YouTube videos</li>
                </ul>
                <a href="{}/admin/settings">Go to Admin Settings</a>
            </div>
        </body>
        </html>
        "#,
        connected_account,
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
    )))
}
