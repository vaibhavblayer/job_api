// src/admin/handlers/settings.rs

use axum::{extract::Extension, Json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::admin::models::{SystemSetting, TestConnectionRequest, UpdateSystemSettingsRequestV2};
use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};

/// GET /api/admin/settings - Get all system settings
pub async fn get_system_settings(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<std::collections::HashMap<String, String>>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "System settings access denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Fetching system settings"
    );

    let settings_map = state
        .settings_service
        .get_all_settings()
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "Error fetching system settings"
            );
            ApiError::InternalServer(format!("Failed to fetch settings: {}", e))
        })?;

    info!(
        admin_user_id = %authed.id,
        settings_count = settings_map.len(),
        "System settings fetched successfully"
    );

    Ok(Json(settings_map))
}

/// PUT /api/admin/settings - Update system settings
pub async fn update_system_settings(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<UpdateSystemSettingsRequestV2>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "System settings update denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        settings_count = request.settings.len(),
        "Updating system settings"
    );

    if !state.settings_service.is_encryption_available() {
        let has_encrypted = request
            .settings
            .values()
            .any(|s| s.encrypt.unwrap_or(false));
        if has_encrypted {
            warn!(
                admin_user_id = %authed.id,
                "Encryption requested but not available"
            );
            return Err(ApiError::BadRequest(
                "Encryption not configured. Set ENCRYPTION_MASTER_KEY environment variable."
                    .to_string(),
            ));
        }
    }

    let mut updated_count = 0;
    let mut errors = Vec::new();

    let sensitive_keys = vec![
        "openai_api_key",
        "aws_secret_access_key",
        "google_client_secret",
        "google_refresh_token",
        "google_access_token",
        "monitoring_sentry_dsn",
    ];

    for (key, setting_update) in request.settings.iter() {
        if !key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            errors.push(format!("Invalid setting key: {}", key));
            continue;
        }

        let should_encrypt = setting_update
            .encrypt
            .unwrap_or_else(|| sensitive_keys.contains(&key.as_str()));

        let result = state
            .settings_service
            .set_setting(key, &setting_update.value, should_encrypt, Some(&authed.id))
            .await;

        match result {
            Ok(_) => {
                updated_count += 1;
                debug!(
                    admin_user_id = %authed.id,
                    setting_key = %key,
                    encrypted = should_encrypt,
                    "System setting updated successfully"
                );
            }
            Err(e) => {
                error!(
                    error = %e,
                    setting_key = %key,
                    "Error updating system setting"
                );
                errors.push(format!("Failed to update {}: {}", key, e));
            }
        }
    }

    if !errors.is_empty() && updated_count == 0 {
        error!(
            admin_user_id = %authed.id,
            errors = ?errors,
            "All system settings updates failed"
        );
        return Err(ApiError::BadRequest(format!(
            "Failed to update settings: {}",
            errors.join(", ")
        )));
    }

    info!(
        admin_user_id = %authed.id,
        updated_count = updated_count,
        error_count = errors.len(),
        "System settings update completed"
    );

    let response = serde_json::json!({
        "message": "Settings updated successfully",
        "updated_count": updated_count,
        "errors": errors
    });

    Ok(Json(response))
}

/// POST /api/admin/settings/test-connection - Test service connection
pub async fn test_service_connection(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<TestConnectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Test connection denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        service = %request.service,
        "Testing service connection"
    );

    let result = match request.service.as_str() {
        "openai" => {
            let api_key = if let Some(creds) = &request.credentials {
                creds.get("api_key").map(|s| s.to_string())
            } else {
                state
                    .settings_service
                    .get_setting("openai_api_key")
                    .await
                    .map_err(|e| {
                        ApiError::InternalServer(format!("Failed to get API key: {}", e))
                    })?
            };

            if let Some(key) = api_key {
                let base_url = if let Some(creds) = &request.credentials {
                    creds.get("base_url").map(|s| s.to_string())
                } else {
                    state
                        .settings_service
                        .get_setting("openai_base_url")
                        .await
                        .map_err(|e| {
                            ApiError::InternalServer(format!("Failed to get base URL: {}", e))
                        })?
                }
                .unwrap_or_else(|| "https://api.openai.com".to_string());

                let test_url = format!("{}/v1/models", base_url);
                let response = state
                    .http
                    .get(&test_url)
                    .header("Authorization", format!("Bearer {}", key))
                    .send()
                    .await;

                match response {
                    Ok(resp) if resp.status().is_success() => {
                        info!(admin_user_id = %authed.id, "OpenAI connection test successful");
                        serde_json::json!({
                            "success": true,
                            "message": "OpenAI connection successful",
                            "service": "openai"
                        })
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let error_text = resp
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());
                        warn!(
                            admin_user_id = %authed.id,
                            status = %status,
                            error = %error_text,
                            "OpenAI connection test failed"
                        );
                        serde_json::json!({
                            "success": false,
                            "message": format!("OpenAI connection failed: {} - {}", status, error_text),
                            "service": "openai"
                        })
                    }
                    Err(e) => {
                        error!(
                            admin_user_id = %authed.id,
                            error = %e,
                            "OpenAI connection test error"
                        );
                        serde_json::json!({
                            "success": false,
                            "message": format!("OpenAI connection error: {}", e),
                            "service": "openai"
                        })
                    }
                }
            } else {
                serde_json::json!({
                    "success": false,
                    "message": "OpenAI API key not configured",
                    "service": "openai"
                })
            }
        }
        "aws_s3" => match state.aws_service.test_s3_connection().await {
            Ok(test_result) => {
                info!(
                    admin_user_id = %authed.id,
                    success = test_result.success,
                    "AWS S3 connection test completed"
                );
                serde_json::to_value(test_result).unwrap_or_else(|_| {
                    serde_json::json!({
                        "success": false,
                        "message": "Failed to serialize test result",
                        "service": "aws_s3"
                    })
                })
            }
            Err(e) => {
                error!(
                    admin_user_id = %authed.id,
                    error = %e,
                    "AWS S3 connection test error"
                );
                serde_json::json!({
                    "success": false,
                    "message": format!("AWS S3 test error: {}", e),
                    "service": "aws_s3"
                })
            }
        },
        "aws_ses" => match state.aws_service.test_ses_connection().await {
            Ok(test_result) => {
                info!(
                    admin_user_id = %authed.id,
                    success = test_result.success,
                    "AWS SES connection test completed"
                );
                serde_json::to_value(test_result).unwrap_or_else(|_| {
                    serde_json::json!({
                        "success": false,
                        "message": "Failed to serialize test result",
                        "service": "aws_ses"
                    })
                })
            }
            Err(e) => {
                error!(
                    admin_user_id = %authed.id,
                    error = %e,
                    "AWS SES connection test error"
                );
                serde_json::json!({
                    "success": false,
                    "message": format!("AWS SES test error: {}", e),
                    "service": "aws_ses"
                })
            }
        },
        "google" | "Google Meet" => {
            match state.google_service.test_connection().await {
                Ok(test_result) => {
                    info!(
                        admin_user_id = %authed.id,
                        success = test_result.success,
                        "Google connection test completed"
                    );
                    serde_json::json!({
                        "success": test_result.success,
                        "message": test_result.message,
                        "service": "google"
                    })
                }
                Err(e) => {
                    error!(
                        admin_user_id = %authed.id,
                        error = %e,
                        "Google connection test error"
                    );
                    serde_json::json!({
                        "success": false,
                        "message": format!("Google test error: {}", e),
                        "service": "google"
                    })
                }
            }
        }
        _ => {
            warn!(
                admin_user_id = %authed.id,
                service = %request.service,
                "Unknown service for connection test"
            );
            return Err(ApiError::BadRequest(format!(
                "Unknown service: {}",
                request.service
            )));
        }
    };

    Ok(Json(result))
}

/// GET /api/admin/settings/google/auth-url - Get Google OAuth authorization URL
pub async fn get_google_auth_url(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Google auth URL request denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Generating Google OAuth authorization URL"
    );

    // Get the redirect URI - use the admin callback endpoint
    let redirect_uri = std::env::var("GOOGLE_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:8080/api/admin/settings/google/callback".to_string());

    let auth_url = state
        .google_service
        .get_authorization_url(&redirect_uri)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate Google OAuth URL");
            ApiError::InternalServer(format!("Failed to generate OAuth URL: {}", e))
        })?;

    info!(
        admin_user_id = %authed.id,
        "Google OAuth URL generated successfully"
    );

    Ok(Json(serde_json::json!({
        "auth_url": auth_url,
        "redirect_uri": redirect_uri
    })))
}

/// GET /api/admin/settings/google/callback - Handle Google OAuth callback
pub async fn google_oauth_callback(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Html<String>, ApiError> {
    let state = state_lock.read().await.clone();

    // Check for error from Google
    if let Some(error) = params.get("error") {
        error!(oauth_error = %error, "Google OAuth returned error");
        return Ok(axum::response::Html(format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Authorization Failed</title>
                <style>
                    body {{ font-family: system-ui, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f3f4f6; }}
                    .container {{ text-align: center; padding: 2rem; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
                    h1 {{ color: #dc2626; }}
                </style>
            </head>
            <body>
                <div class="container">
                    <h1>❌ Authorization Failed</h1>
                    <p>Error: {}</p>
                    <p>You can close this window and try again.</p>
                    <script>
                        window.opener && window.opener.postMessage({{ type: 'GOOGLE_OAUTH_ERROR', error: '{}' }}, '*');
                    </script>
                </div>
            </body>
            </html>
            "#,
            error, error
        )));
    }

    // Get authorization code
    let code = params.get("code").ok_or_else(|| {
        error!("No authorization code in OAuth callback");
        ApiError::BadRequest("No authorization code provided".to_string())
    })?;

    info!("Received Google OAuth callback with authorization code");

    // Get the redirect URI (must match the one used in authorization)
    let redirect_uri = std::env::var("GOOGLE_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:8080/api/admin/settings/google/callback".to_string());

    // Exchange code for tokens
    match state.google_service.exchange_code(code, &redirect_uri).await {
        Ok(_token_response) => {
            info!("Google OAuth tokens exchanged and stored successfully");
            
            // Get the connected account email
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
                    <title>Authorization Successful</title>
                    <style>
                        body {{ font-family: system-ui, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f3f4f6; }}
                        .container {{ text-align: center; padding: 2rem; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); max-width: 400px; }}
                        h1 {{ color: #16a34a; }}
                        .email {{ color: #4b5563; font-weight: 500; }}
                        .close-msg {{ color: #6b7280; margin-top: 1rem; }}
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1>✅ Authorization Successful!</h1>
                        <p>Google account connected:</p>
                        <p class="email">{}</p>
                        <p class="close-msg">This window will close automatically...</p>
                        <script>
                            window.opener && window.opener.postMessage({{ 
                                type: 'GOOGLE_OAUTH_SUCCESS', 
                                email: '{}' 
                            }}, '*');
                            setTimeout(() => window.close(), 2000);
                        </script>
                    </div>
                </body>
                </html>
                "#,
                connected_account, connected_account
            )))
        }
        Err(e) => {
            error!(error = %e, "Failed to exchange Google OAuth code for tokens");
            Ok(axum::response::Html(format!(
                r#"
                <!DOCTYPE html>
                <html>
                <head>
                    <title>Authorization Failed</title>
                    <style>
                        body {{ font-family: system-ui, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #f3f4f6; }}
                        .container {{ text-align: center; padding: 2rem; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
                        h1 {{ color: #dc2626; }}
                        .error {{ color: #6b7280; font-size: 0.875rem; }}
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1>❌ Authorization Failed</h1>
                        <p>Failed to complete authorization.</p>
                        <p class="error">{}</p>
                        <p>You can close this window and try again.</p>
                        <script>
                            window.opener && window.opener.postMessage({{ type: 'GOOGLE_OAUTH_ERROR', error: '{}' }}, '*');
                        </script>
                    </div>
                </body>
                </html>
                "#,
                e, e
            )))
        }
    }
}

/// GET /api/admin/settings/google/status - Get Google OAuth connection status
pub async fn get_google_connection_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    let connected_account = state
        .settings_service
        .get_setting("google_connected_account")
        .await
        .ok()
        .flatten();

    let has_refresh_token = state
        .settings_service
        .get_setting("google_refresh_token")
        .await
        .ok()
        .flatten()
        .is_some();

    // Test the connection if we have tokens
    let connection_valid = if has_refresh_token {
        match state.google_service.test_connection().await {
            Ok(result) => result.success,
            Err(_) => false,
        }
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "connected": connected_account.is_some() && has_refresh_token,
        "connected_account": connected_account,
        "connection_valid": connection_valid
    })))
}

/// POST /api/admin/settings/google/disconnect - Disconnect Google account
pub async fn disconnect_google_account(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Disconnecting Google account"
    );

    // Clear Google OAuth tokens
    let keys_to_clear = [
        "google_access_token",
        "google_refresh_token",
        "google_token_expires_at",
        "google_connected_account",
    ];

    for key in keys_to_clear {
        let _ = state
            .settings_service
            .delete_setting(key)
            .await;
    }

    info!(
        admin_user_id = %authed.id,
        "Google account disconnected successfully"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Google account disconnected successfully"
    })))
}

/// GET /api/settings/public - Get public system settings (no auth required)
pub async fn get_public_system_settings(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await.clone();

    info!("Fetching public system settings");

    let public_keys = vec![
        "application_name",
        "tagline",
        "company_description",
        "company_logo",
        "address",
        "phone",
        "email",
        "social_linkedin",
        "social_x",
        "social_instagram",
        "social_facebook",
    ];

    let mut settings_map = serde_json::Map::new();

    for key in public_keys {
        let setting: Option<SystemSetting> =
            sqlx::query_as::<_, SystemSetting>("SELECT * FROM system_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| {
                    error!(
                        error = %e,
                        setting_key = %key,
                        "Database error fetching public system setting"
                    );
                    ApiError::DatabaseError(e)
                })?;

        if let Some(s) = setting {
            settings_map.insert(
                key.to_string(),
                serde_json::json!({
                    "value": s.value,
                    "updated_at": s.updated_at
                }),
            );
        }
    }

    info!(
        settings_count = settings_map.len(),
        "Public system settings fetched successfully"
    );

    Ok(Json(serde_json::json!(settings_map)))
}
