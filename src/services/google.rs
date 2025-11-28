// src/services/google.rs
use crate::services::settings::{SettingsError, SettingsService};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum GoogleError {
    #[error("Google OAuth not configured")]
    NotConfigured,

    #[error("OAuth flow failed: {0}")]
    OAuthFailed(String),

    #[error("Calendar API error: {0}")]
    CalendarError(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Settings error: {0}")]
    SettingsError(#[from] SettingsError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("HTTP request failed: {0}")]
    RequestFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleMeetConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: Option<String>,
    pub access_token: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub connected_account: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEvent {
    pub summary: String,
    pub description: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub attendees: Vec<String>,
    pub create_meet_link: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEventResponse {
    pub id: String,
    pub html_link: String,
    pub hangout_link: Option<String>, // Google Meet link
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub token_type: String,
    pub scope: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub service: String,
}

// Google Calendar API request/response types
#[derive(Debug, Serialize)]
struct CalendarEventRequest {
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    start: EventDateTime,
    end: EventDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    attendees: Option<Vec<Attendee>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conference_data: Option<ConferenceData>,
}

#[derive(Debug, Serialize)]
struct EventDateTime {
    #[serde(rename = "dateTime")]
    date_time: String,
    #[serde(rename = "timeZone")]
    time_zone: String,
}

#[derive(Debug, Serialize)]
struct Attendee {
    email: String,
}

#[derive(Debug, Serialize)]
struct ConferenceData {
    #[serde(rename = "createRequest")]
    create_request: ConferenceCreateRequest,
}

#[derive(Debug, Serialize)]
struct ConferenceCreateRequest {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "conferenceSolutionKey")]
    conference_solution_key: ConferenceSolutionKey,
}

#[derive(Debug, Serialize)]
struct ConferenceSolutionKey {
    #[serde(rename = "type")]
    solution_type: String,
}

#[derive(Debug, Deserialize)]
struct CalendarEventApiResponse {
    id: String,
    #[serde(rename = "htmlLink")]
    html_link: String,
    #[serde(rename = "hangoutLink")]
    hangout_link: Option<String>, // Deprecated but still returned
    #[serde(rename = "conferenceData")]
    conference_data: Option<ConferenceDataResponse>,
}

#[derive(Debug, Deserialize)]
struct ConferenceDataResponse {
    #[serde(rename = "conferenceId")]
    conference_id: Option<String>,
    #[serde(rename = "entryPoints")]
    entry_points: Option<Vec<EntryPoint>>,
}

#[derive(Debug, Deserialize)]
struct EntryPoint {
    #[serde(rename = "entryPointType")]
    entry_point_type: String,
    uri: Option<String>,
    label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleService {
    settings_service: Arc<SettingsService>,
    client: Client,
}

impl GoogleService {
    pub fn new(settings_service: Arc<SettingsService>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            settings_service,
            client,
        }
    }

    /// Sync Google OAuth credentials from environment variables to database
    /// This is called on startup to ensure env vars are available in the settings UI
    pub async fn sync_env_to_settings(&self) -> Result<(), GoogleError> {
        use std::env;

        // Sync GOOGLE_CLIENT_ID
        if let Ok(client_id) = env::var("GOOGLE_CLIENT_ID") {
            if let Err(e) = self
                .settings_service
                .set_setting("google_client_id", &client_id, false, None)
                .await
            {
                warn!("Failed to sync GOOGLE_CLIENT_ID to database: {}", e);
            } else {
                info!("Synced GOOGLE_CLIENT_ID to database");
            }
        }

        // Sync GOOGLE_CLIENT_SECRET (encrypted)
        if let Ok(client_secret) = env::var("GOOGLE_CLIENT_SECRET") {
            if let Err(e) = self
                .settings_service
                .set_setting("google_client_secret", &client_secret, true, None)
                .await
            {
                warn!("Failed to sync GOOGLE_CLIENT_SECRET to database: {}", e);
            } else {
                info!("Synced GOOGLE_CLIENT_SECRET to database (encrypted)");
            }
        }

        Ok(())
    }

    /// Get Google Meet configuration from settings
    pub async fn get_config(&self) -> Result<GoogleMeetConfig, GoogleError> {
        let keys = [
            "google_client_id",
            "google_client_secret",
            "google_refresh_token",
            "google_access_token",
            "google_token_expires_at",
            "google_connected_account",
        ];

        let settings = self.settings_service.get_settings(&keys).await?;

        let client_id = settings
            .get("google_client_id")
            .and_then(|v| v.clone())
            .ok_or(GoogleError::NotConfigured)?;

        let client_secret = settings
            .get("google_client_secret")
            .and_then(|v| v.clone())
            .ok_or(GoogleError::NotConfigured)?;

        let refresh_token = settings.get("google_refresh_token").and_then(|v| v.clone());

        let access_token = settings.get("google_access_token").and_then(|v| v.clone());

        let token_expires_at = settings
            .get("google_token_expires_at")
            .and_then(|v| v.clone())
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let connected_account = settings
            .get("google_connected_account")
            .and_then(|v| v.clone());

        Ok(GoogleMeetConfig {
            client_id,
            client_secret,
            refresh_token,
            access_token,
            token_expires_at,
            connected_account,
        })
    }

    /// Get authorization URL for OAuth flow
    pub async fn get_authorization_url(&self, redirect_uri: &str) -> Result<String, GoogleError> {
        let config = self.get_config().await?;

        // Scopes required for all features:
        // - openid, email, profile: Basic user info
        // - calendar.events: Create and manage calendar events
        // - meetings.space.created: Create Google Meet spaces using Meet API v2
        // - youtube.readonly: Access YouTube data
        let scopes = vec![
            "openid",
            "email",
            "profile",
            "https://www.googleapis.com/auth/calendar.events",
            "https://www.googleapis.com/auth/meetings.space.created",
            "https://www.googleapis.com/auth/youtube.readonly",
        ];

        let scope_param = scopes.join(" ");

        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            urlencoding::encode(&config.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scope_param)
        );

        debug!("Generated Google OAuth authorization URL with scopes: {}", scope_param);
        Ok(auth_url)
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, GoogleError> {
        let config = self.get_config().await?;

        let params = [
            ("code", code),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ];

        debug!("Exchanging authorization code for tokens");

        let response = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Token exchange failed");
            return Err(GoogleError::OAuthFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let token_response = response
            .json::<TokenResponse>()
            .await
            .map_err(|e| GoogleError::SerializationError(e.to_string()))?;

        // Store tokens in settings
        self.settings_service
            .set_setting(
                "google_access_token",
                &token_response.access_token,
                true,
                Some("system"),
            )
            .await?;

        if let Some(ref refresh_token) = token_response.refresh_token {
            self.settings_service
                .set_setting("google_refresh_token", refresh_token, true, None)
                .await?;
        }

        // Calculate and store expiration time
        let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);
        self.settings_service
            .set_setting(
                "google_token_expires_at",
                &expires_at.to_rfc3339(),
                false,
                None,
            )
            .await?;

        // Get user email and store as connected account
        if let Ok(email) = self.get_user_email(&token_response.access_token).await {
            self.settings_service
                .set_setting("google_connected_account", &email, false, None)
                .await?;
        }

        info!("Successfully exchanged authorization code for tokens");
        Ok(token_response)
    }

    /// Get user email from access token
    async fn get_user_email(&self, access_token: &str) -> Result<String, GoogleError> {
        let response = self
            .client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(GoogleError::RequestFailed(
                "Failed to get user info".to_string(),
            ));
        }

        #[derive(Deserialize)]
        struct UserInfo {
            email: String,
        }

        let user_info = response
            .json::<UserInfo>()
            .await
            .map_err(|e| GoogleError::SerializationError(e.to_string()))?;

        Ok(user_info.email)
    }

    /// Refresh access token using refresh token
    pub async fn refresh_access_token(&self) -> Result<String, GoogleError> {
        let config = self.get_config().await?;

        let refresh_token = config
            .refresh_token
            .ok_or_else(|| GoogleError::InvalidConfig("No refresh token available".to_string()))?;

        let params = [
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        debug!(
            client_id = %config.client_id,
            has_refresh_token = true,
            "Refreshing access token with Google OAuth"
        );

        let response = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send token refresh request");
                GoogleError::RequestFailed(e.to_string())
            })?;

        let status = response.status();
        debug!(status = %status, "Received token refresh response");

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                status = %status,
                error = %error_text,
                "Token refresh failed - check your refresh token and credentials"
            );
            return Err(GoogleError::OAuthFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let token_response = response
            .json::<TokenResponse>()
            .await
            .map_err(|e| GoogleError::SerializationError(e.to_string()))?;

        // Store new access token
        self.settings_service
            .set_setting(
                "google_access_token",
                &token_response.access_token,
                true,
                None,
            )
            .await?;

        // Update expiration time
        let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);
        self.settings_service
            .set_setting(
                "google_token_expires_at",
                &expires_at.to_rfc3339(),
                false,
                None,
            )
            .await?;

        info!("Successfully refreshed access token");
        Ok(token_response.access_token)
    }

    /// Get valid access token (refreshing if necessary)
    async fn get_valid_access_token(&self) -> Result<String, GoogleError> {
        let config = self.get_config().await?;

        // Check if token exists and is not expired
        if let Some(access_token) = config.access_token {
            if let Some(expires_at) = config.token_expires_at {
                // Refresh if token expires in less than 5 minutes
                if expires_at > Utc::now() + chrono::Duration::minutes(5) {
                    debug!("Using existing access token");
                    return Ok(access_token);
                }
            }
        }

        // Token is expired or doesn't exist, refresh it
        warn!("Access token expired or missing, refreshing");
        self.refresh_access_token().await
    }

    /// Create a Google Meet space directly using Meet API v2
    /// This is more reliable than using Calendar API's conferenceData
    async fn create_meet_space(&self) -> Result<String, GoogleError> {
        let access_token = self.get_valid_access_token().await?;

        debug!("Creating Google Meet space using Meet API v2");

        // Create an empty Meet space
        let response = self
            .client
            .post("https://meet.googleapis.com/v2/spaces")
            .bearer_auth(&access_token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send Meet space creation request");
                GoogleError::RequestFailed(e.to_string())
            })?;

        let status = response.status();
        debug!(status = %status, "Received Meet space creation response");

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                status = %status,
                error = %error_text,
                "Meet space creation failed - check if Meet API is enabled in Google Cloud Console"
            );
            return Err(GoogleError::CalendarError(format!(
                "Meet API HTTP {}: {}",
                status, error_text
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;
        
        debug!(response_body = %response_text, "Meet API response body");

        #[derive(Deserialize)]
        struct MeetSpace {
            #[serde(rename = "meetingUri")]
            meeting_uri: String,
        }

        let meet_space: MeetSpace = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!(error = %e, response = %response_text, "Failed to parse Meet space response");
                GoogleError::SerializationError(e.to_string())
            })?;

        info!(meet_link = %meet_space.meeting_uri, "Google Meet space created successfully");
        Ok(meet_space.meeting_uri)
    }

    /// Create a calendar event with optional Google Meet link
    pub async fn create_calendar_event(
        &self,
        event: CalendarEvent,
    ) -> Result<CalendarEventResponse, GoogleError> {
        let access_token = self.get_valid_access_token().await?;

        // If Meet link is requested, try to create it using Meet API v2 first
        let meet_link = if event.create_meet_link {
            match self.create_meet_space().await {
                Ok(link) => {
                    info!("Created Meet link using Meet API v2: {}", link);
                    Some(link)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to create Meet space using Meet API v2, will try Calendar API fallback");
                    None
                }
            }
        } else {
            None
        };

        // Build event request
        let event_request = CalendarEventRequest {
            summary: event.summary,
            description: event.description,
            start: EventDateTime {
                date_time: event.start.to_rfc3339(),
                time_zone: "Asia/Kolkata".to_string(), // Indian Standard Time
            },
            end: EventDateTime {
                date_time: event.end.to_rfc3339(),
                time_zone: "Asia/Kolkata".to_string(), // Indian Standard Time
            },
            attendees: if event.attendees.is_empty() {
                None
            } else {
                Some(
                    event
                        .attendees
                        .iter()
                        .map(|email| Attendee {
                            email: email.clone(),
                        })
                        .collect(),
                )
            },
            conference_data: if event.create_meet_link && meet_link.is_none() {
                // Only use Calendar API conferenceData if Meet API failed
                Some(ConferenceData {
                    create_request: ConferenceCreateRequest {
                        request_id: uuid::Uuid::new_v4().to_string(),
                        conference_solution_key: ConferenceSolutionKey {
                            solution_type: "hangoutsMeet".to_string(),
                        },
                    },
                })
            } else {
                None
            },
        };

        debug!(
            summary = %event_request.summary,
            create_meet_link = event.create_meet_link,
            has_meet_link_from_api = meet_link.is_some(),
            start_time = %event.start,
            end_time = %event.end,
            attendee_count = event.attendees.len(),
            "Creating calendar event"
        );

        // Serialize request for logging
        if let Ok(request_json) = serde_json::to_string_pretty(&event_request) {
            debug!(request_body = %request_json, "Calendar API request body");
        }

        // Make API request
        let url = "https://www.googleapis.com/calendar/v3/calendars/primary/events";
        let mut query_params = vec![
            ("sendUpdates", "all"), // Send email invitations to all attendees
        ];

        // Add conferenceDataVersion parameter if creating Meet link via Calendar API
        if event.create_meet_link && meet_link.is_none() {
            query_params.push(("conferenceDataVersion", "1"));
            debug!("Added conferenceDataVersion=1 query parameter for Calendar API fallback");
        }

        let mut request = self
            .client
            .post(url)
            .bearer_auth(&access_token)
            .query(&query_params)
            .json(&event_request);

        debug!(
            send_updates = "all",
            "Calendar event will send email invitations to all attendees"
        );

        let response = request
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send calendar event creation request");
                GoogleError::RequestFailed(e.to_string())
            })?;

        let status = response.status();
        debug!(status = %status, "Received calendar event creation response");

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                status = %status,
                error = %error_text,
                "Calendar event creation failed - check OAuth scopes and permissions"
            );
            return Err(GoogleError::CalendarError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        // Get response text for logging before parsing
        let response_text = response
            .text()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;
        
        debug!(response_body = %response_text, "Calendar API response body");

        let api_response: CalendarEventApiResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!(error = %e, response = %response_text, "Failed to parse calendar event response");
                GoogleError::SerializationError(e.to_string())
            })?;

        // Use Meet link from Meet API if available, otherwise extract from Calendar API response
        let final_meet_link = meet_link.or_else(|| {
            if let Some(conf_data) = &api_response.conference_data {
                if let Some(entry_points) = &conf_data.entry_points {
                    // Find the video entry point (Google Meet link)
                    entry_points
                        .iter()
                        .find(|ep| ep.entry_point_type == "video")
                        .and_then(|ep| ep.uri.clone())
                } else {
                    None
                }
            } else {
                // Fallback to deprecated hangoutLink if conferenceData not available
                api_response.hangout_link.clone()
            }
        });

        let result = CalendarEventResponse {
            id: api_response.id,
            html_link: api_response.html_link,
            hangout_link: final_meet_link,
        };

        info!(
            event_id = %result.id,
            has_meet_link = result.hangout_link.is_some(),
            meet_link = ?result.hangout_link,
            "Calendar event created successfully"
        );

        Ok(result)
    }

    /// Test Google Calendar API connection
    pub async fn test_connection(&self) -> Result<TestResult, GoogleError> {
        match self.get_valid_access_token().await {
            Ok(access_token) => {
                // Try to list calendars to verify access
                let response = self
                    .client
                    .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
                    .bearer_auth(&access_token)
                    .send()
                    .await
                    .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

                if response.status().is_success() {
                    info!("Google Calendar connection test successful");
                    Ok(TestResult {
                        success: true,
                        message: "Successfully connected to Google Calendar API".to_string(),
                        service: "google_calendar".to_string(),
                    })
                } else {
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    warn!(error = %error_text, "Google Calendar connection test failed");
                    Ok(TestResult {
                        success: false,
                        message: format!("Failed to access Google Calendar: {}", error_text),
                        service: "google_calendar".to_string(),
                    })
                }
            }
            Err(e) => {
                warn!(error = %e, "Google Calendar connection test failed");
                Ok(TestResult {
                    success: false,
                    message: format!("Failed to get access token: {}", e),
                    service: "google_calendar".to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::SettingsService;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE system_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                encrypted INTEGER DEFAULT 0,
                description TEXT,
                updated_at TEXT DEFAULT (datetime('now')),
                updated_by TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_get_config_not_configured() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));
        let google_service = GoogleService::new(settings_service);

        let result = google_service.get_config().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GoogleError::NotConfigured));
    }

    #[tokio::test]
    async fn test_get_authorization_url() {
        let pool = setup_test_db().await;
        let settings_service = Arc::new(SettingsService::new(pool));

        // Set Google credentials (don't encrypt in tests - encryption not configured)
        settings_service
            .set_setting("google_client_id", "test_client_id", false, Some("admin"))
            .await
            .unwrap();
        settings_service
            .set_setting("google_client_secret", "test_secret", false, Some("admin")) // Changed to false
            .await
            .unwrap();

        let google_service = GoogleService::new(settings_service);
        let auth_url = google_service
            .get_authorization_url("http://localhost:3000/callback")
            .await
            .unwrap();

        assert!(auth_url.contains("accounts.google.com/o/oauth2/v2/auth"));
        assert!(auth_url.contains("client_id=test_client_id"));
        assert!(auth_url.contains("redirect_uri=http"));
        assert!(auth_url.contains("scope="));
    }
}
