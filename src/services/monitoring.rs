// Monitoring Service with Sentry integration
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub sentry_dsn: Option<String>,
    pub log_level: String,
    pub log_retention_days: i32,
    pub enable_error_tracking: bool,
    pub enable_performance_monitoring: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            sentry_dsn: None,
            log_level: "info".to_string(),
            log_retention_days: 30,
            enable_error_tracking: true,
            enable_performance_monitoring: false,
        }
    }
}

pub struct MonitoringService {
    pool: SqlitePool,
    config: Arc<tokio::sync::RwLock<MonitoringConfig>>,
    sentry_guard: Option<sentry::ClientInitGuard>,
}

impl MonitoringService {
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let config = Arc::new(tokio::sync::RwLock::new(MonitoringConfig::default()));

        let mut service = Self {
            pool,
            config,
            sentry_guard: None,
        };

        // Load configuration from database
        if let Err(e) = service.load_config().await {
            warn!("Failed to load monitoring configuration: {}", e);
        }

        // Initialize Sentry if configured
        if let Err(e) = service.initialize_sentry().await {
            warn!("Failed to initialize Sentry: {}", e);
        }

        Ok(service)
    }

    /// Load monitoring configuration from database
    async fn load_config(&mut self) -> Result<()> {
        let settings_service = crate::services::settings::SettingsService::new(self.pool.clone());

        let sentry_dsn = settings_service
            .get_setting("monitoring_sentry_dsn")
            .await?;
        let log_level = settings_service
            .get_setting("monitoring_log_level")
            .await?
            .unwrap_or_else(|| "info".to_string());
        let log_retention_days = settings_service
            .get_setting("monitoring_log_retention_days")
            .await?
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(30);
        let enable_error_tracking = settings_service
            .get_setting("monitoring_error_tracking_enabled")
            .await?
            .map(|v| v == "true")
            .unwrap_or(true);
        let enable_performance_monitoring = settings_service
            .get_setting("monitoring_performance_monitoring_enabled")
            .await?
            .map(|v| v == "true")
            .unwrap_or(false);

        let config = MonitoringConfig {
            sentry_dsn,
            log_level,
            log_retention_days,
            enable_error_tracking,
            enable_performance_monitoring,
        };

        *self.config.write().await = config;
        Ok(())
    }

    /// Initialize Sentry client
    pub async fn initialize_sentry(&mut self) -> Result<()> {
        let config = self.config.read().await;

        if !config.enable_error_tracking {
            info!("Error tracking is disabled");
            return Ok(());
        }

        let Some(dsn) = &config.sentry_dsn else {
            info!("Sentry DSN not configured");
            return Ok(());
        };

        if dsn.is_empty() {
            info!("Sentry DSN is empty");
            return Ok(());
        }

        // Initialize Sentry
        let guard = sentry::init((
            dsn.as_str(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: Some(
                    std::env::var("ENVIRONMENT")
                        .unwrap_or_else(|_| "development".to_string())
                        .into(),
                ),
                traces_sample_rate: if config.enable_performance_monitoring {
                    0.1
                } else {
                    0.0
                },
                ..Default::default()
            },
        ));

        self.sentry_guard = Some(guard);
        info!("Sentry initialized successfully");

        Ok(())
    }

    /// Log an error with context
    pub fn log_error(&self, error: &dyn std::error::Error, context: Option<Value>) {
        error!("Error occurred: {}", error);

        if let Some(ctx) = &context {
            error!(
                "Context: {}",
                serde_json::to_string_pretty(ctx).unwrap_or_default()
            );
        }

        // Send to Sentry if initialized
        sentry::capture_error(error);

        if let Some(context) = context {
            sentry::configure_scope(|scope| {
                scope.set_extra("error_context", context);
            });
        }
    }

    /// Log an event
    pub fn log_event(&self, event: &str, data: Option<Value>) {
        info!("Event: {}", event);

        if let Some(event_data) = &data {
            info!(
                "Data: {}",
                serde_json::to_string_pretty(event_data).unwrap_or_default()
            );
        }

        // Send to Sentry as breadcrumb
        let mut breadcrumb = sentry::Breadcrumb {
            ty: "default".into(),
            level: sentry::Level::Info,
            message: Some(event.to_string()),
            ..Default::default()
        };

        if let Some(data_value) = data {
            if let Value::Object(map) = data_value {
                for (key, value) in map {
                    breadcrumb.data.insert(key, value);
                }
            }
        }

        sentry::add_breadcrumb(breadcrumb);
    }

    /// Get current configuration
    pub async fn get_config(&self) -> MonitoringConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&mut self, new_config: MonitoringConfig) -> Result<()> {
        let settings_service = crate::services::settings::SettingsService::new(self.pool.clone());

        // Save to database
        if let Some(dsn) = &new_config.sentry_dsn {
            settings_service
                .set_setting("monitoring_sentry_dsn", dsn, true, Some("system"))
                .await?;
        }
        settings_service
            .set_setting(
                "monitoring_log_level",
                &new_config.log_level,
                false,
                Some("system"),
            )
            .await?;
        settings_service
            .set_setting(
                "monitoring_log_retention_days",
                &new_config.log_retention_days.to_string(),
                false,
                Some("system"),
            )
            .await?;
        settings_service
            .set_setting(
                "monitoring_error_tracking_enabled",
                &new_config.enable_error_tracking.to_string(),
                false,
                Some("system"),
            )
            .await?;
        settings_service
            .set_setting(
                "monitoring_performance_monitoring_enabled",
                &new_config.enable_performance_monitoring.to_string(),
                false,
                Some("system"),
            )
            .await?;

        // Update in-memory config
        *self.config.write().await = new_config;

        // Reinitialize Sentry with new config
        self.initialize_sentry().await?;

        Ok(())
    }

    /// Capture a message
    pub fn capture_message(&self, message: &str, level: sentry::Level) {
        sentry::capture_message(message, level);
    }

    /// Start a transaction for performance monitoring
    pub fn start_transaction(&self, name: &str, operation: &str) -> sentry::TransactionOrSpan {
        let ctx = sentry::TransactionContext::new(name, operation);
        sentry::start_transaction(ctx).into()
    }

    /// Test Sentry connection
    pub async fn test_connection(&self) -> Result<bool> {
        let config = self.config.read().await;

        if config.sentry_dsn.is_none() {
            return Err(anyhow::anyhow!("Sentry DSN not configured"));
        }

        // Send a test event
        sentry::capture_message("Sentry connection test", sentry::Level::Info);

        Ok(true)
    }
}

// Implement Drop to ensure Sentry is properly shut down
impl Drop for MonitoringService {
    fn drop(&mut self) {
        if self.sentry_guard.is_some() {
            info!("Shutting down Sentry");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_config() {
        let config = MonitoringConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.log_retention_days, 30);
        assert!(config.enable_error_tracking);
        assert!(!config.enable_performance_monitoring);
    }
}
