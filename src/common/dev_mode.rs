// src/common/dev_mode.rs
//! Development mode configuration and utilities
//! Allows bypassing authentication for testing purposes

use chrono::Utc;
use std::env;
use uuid::Uuid;

use crate::auth::models::User;

#[derive(Debug, Clone)]
pub struct DevModeConfig {
    pub enabled: bool,
    pub user_email: String,
    pub user_name: String,
    pub user_is_admin: bool,
}

impl DevModeConfig {
    pub fn from_env() -> Self {
        let enabled = env::var("DEV_MODE")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        let user_email = env::var("DEV_USER_EMAIL").unwrap_or_else(|_| "dev@test.com".to_string());

        let user_name = env::var("DEV_USER_NAME").unwrap_or_else(|_| "Dev User".to_string());

        let user_is_admin = env::var("DEV_USER_IS_ADMIN")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        Self {
            enabled,
            user_email,
            user_name,
            user_is_admin,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Create a dev user for testing
    /// Uses a fixed UUID to ensure consistency across requests
    pub fn create_dev_user(&self) -> User {
        // Use a fixed UUID for dev mode to ensure consistency
        let user_id = "00000000-0000-0000-0000-000000000001".to_string();

        User {
            id: user_id.clone(),
            email: self.user_email.clone(),
            name: Some(self.user_name.clone()),
            avatar: None,
            provider: Some("dev".to_string()),
            provider_id: Some(user_id),
            created_at: Some(Utc::now().to_rfc3339()),
        }
    }
}

/// Print dev mode status on startup
pub fn print_dev_mode_status(config: &DevModeConfig) {
    if config.enabled {
        println!("⚠️  🔓 DEV MODE ENABLED 🔓 ⚠️");
        println!("   Authentication bypassed for testing");
        println!("   Dev User: {} ({})", config.user_name, config.user_email);
        println!(
            "   Admin: {}",
            if config.user_is_admin { "Yes" } else { "No" }
        );
        println!("   ⚠️  DO NOT USE IN PRODUCTION ⚠️");
        println!();
    } else {
        println!("🔒 Production mode - Authentication required");
    }
}

/// CLI argument parsing for dev mode
pub fn parse_dev_mode_args() -> Option<bool> {
    let args: Vec<String> = env::args().collect();

    for arg in &args {
        match arg.as_str() {
            "--dev" | "--dev-mode" => return Some(true),
            "--no-dev" | "--prod" | "--production" => return Some(false),
            _ => {}
        }
    }

    None
}

/// Override dev mode from CLI args
pub fn apply_cli_override(mut config: DevModeConfig) -> DevModeConfig {
    if let Some(cli_dev_mode) = parse_dev_mode_args() {
        println!("🔧 CLI override: DEV_MODE = {}", cli_dev_mode);
        config.enabled = cli_dev_mode;
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_mode_config_defaults() {
        // Save original env vars
        let original_dev_mode = env::var("DEV_MODE").ok();

        // Unset for test
        env::remove_var("DEV_MODE");

        let config = DevModeConfig::from_env();
        assert!(!config.enabled, "Dev mode should be disabled by default");

        // Restore
        if let Some(val) = original_dev_mode {
            env::set_var("DEV_MODE", val);
        }
    }

    // Note: Testing parse_dev_mode_args is tricky because it reads directly from env::args()
    // which we can't easily mock in a unit test without external crates or complex setup.
    // However, the logic is simple enough that manual verification or integration tests are better.
}
