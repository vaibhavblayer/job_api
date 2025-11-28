// src/admin/handlers/theme.rs

use axum::{extract::Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};

/// Theme mode: light, dark, or system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Light
    }
}

/// Theme settings stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub customizations: serde_json::Value,
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Light,
            customizations: serde_json::json!({}),
            updated_at: None,
            updated_by: None,
        }
    }
}

/// Request to update theme settings
#[derive(Debug, Deserialize)]
pub struct UpdateThemeRequest {
    pub mode: Option<ThemeMode>,
    pub customizations: Option<serde_json::Value>,
}

/// GET /api/settings/theme - Get theme settings (public endpoint)
pub async fn get_theme_settings(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
) -> Result<Json<ThemeSettings>, ApiError> {
    let state = state_lock.read().await.clone();

    info!("Fetching theme settings");

    // Get theme mode
    let mode_str: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM system_settings WHERE key = 'theme_mode'"
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching theme mode");
        ApiError::DatabaseError(e)
    })?;

    let mode = match mode_str {
        Some((value,)) => match value.as_str() {
            "dark" => ThemeMode::Dark,
            "system" => ThemeMode::System,
            _ => ThemeMode::Light,
        },
        None => ThemeMode::Light,
    };

    // Get theme customizations
    let customizations_row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT value, updated_at, updated_by FROM system_settings WHERE key = 'theme_customizations'"
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching theme customizations");
        ApiError::DatabaseError(e)
    })?;

    let (customizations, updated_at, updated_by) = match customizations_row {
        Some((value, updated_at, updated_by)) => {
            let parsed = serde_json::from_str(&value).unwrap_or(serde_json::json!({}));
            (parsed, updated_at, updated_by)
        }
        None => (serde_json::json!({}), None, None),
    };

    let settings = ThemeSettings {
        mode,
        customizations,
        updated_at,
        updated_by,
    };

    info!("Theme settings fetched successfully");

    Ok(Json(settings))
}

/// PUT /api/admin/settings/theme - Update theme settings (admin only)
pub async fn update_theme_settings(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<UpdateThemeRequest>,
) -> Result<Json<ThemeSettings>, ApiError> {
    let state = state_lock.read().await.clone();

    if !authed.is_admin {
        warn!(
            user_id = %authed.id,
            "Theme settings update denied: admin privileges required"
        );
        return Err(ApiError::Forbidden("Admin privileges required".to_string()));
    }

    info!(
        admin_user_id = %authed.id,
        "Updating theme settings"
    );

    // Update theme mode if provided
    if let Some(mode) = &request.mode {
        let mode_str = match mode {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::System => "system",
        };

        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, encrypted, updated_at, updated_by)
            VALUES ('theme_mode', ?, 0, datetime('now'), ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by
            "#
        )
        .bind(mode_str)
        .bind(&authed.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error updating theme mode");
            ApiError::DatabaseError(e)
        })?;

        info!(
            admin_user_id = %authed.id,
            mode = %mode_str,
            "Theme mode updated"
        );
    }

    // Update customizations if provided
    if let Some(customizations) = &request.customizations {
        let customizations_str = serde_json::to_string(customizations)
            .map_err(|e| ApiError::BadRequest(format!("Invalid customizations JSON: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, encrypted, updated_at, updated_by)
            VALUES ('theme_customizations', ?, 0, datetime('now'), ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by
            "#
        )
        .bind(&customizations_str)
        .bind(&authed.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error updating theme customizations");
            ApiError::DatabaseError(e)
        })?;

        info!(
            admin_user_id = %authed.id,
            "Theme customizations updated"
        );
    }

    // Fetch and return updated settings
    get_theme_settings(Extension(state_lock)).await
}
