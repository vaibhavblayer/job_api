// src/services/settings.rs
use crate::services::encryption::{EncryptionError, EncryptionService};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Setting not found: {0}")]
    NotFound(String),

    #[error("Encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[derive(Debug, Clone)]
struct CachedSetting {
    value: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingValue {
    pub value: String,
    pub encrypted: bool,
}

#[derive(Debug)]
pub struct SettingsService {
    db_pool: SqlitePool,
    cache: Arc<RwLock<HashMap<String, CachedSetting>>>,
    encryption_service: Option<EncryptionService>,
    cache_ttl: Duration,
}

impl SettingsService {
    /// Create a new SettingsService instance
    pub fn new(db_pool: SqlitePool) -> Self {
        // Try to initialize encryption service from environment
        let encryption_service = match EncryptionService::from_env() {
            Ok(service) => {
                info!("Encryption service initialized successfully");
                Some(service)
            }
            Err(e) => {
                warn!("Encryption service not available: {}. Sensitive settings will not be encrypted.", e);
                None
            }
        };

        Self {
            db_pool,
            cache: Arc::new(RwLock::new(HashMap::new())),
            encryption_service,
            cache_ttl: Duration::minutes(5),
        }
    }

    /// Get a setting value by key
    /// Falls back to environment variable if not found in database
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, SettingsError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(key) {
                if cached.expires_at > Utc::now() {
                    debug!(key = %key, "Setting retrieved from cache");
                    return Ok(Some(cached.value.clone()));
                }
            }
        }

        // Query database
        let result = sqlx::query_as::<_, (String, String, Option<i64>)>(
            "SELECT key, value, encrypted FROM system_settings WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.db_pool)
        .await?;

        if let Some((_, value, encrypted)) = result {
            let decrypted_value = if encrypted.unwrap_or(0) == 1 {
                // Decrypt the value
                match &self.encryption_service {
                    Some(service) => service.decrypt(&value).map_err(|e| {
                        error!(key = %key, error = %e, "Failed to decrypt setting");
                        SettingsError::EncryptionError(e)
                    })?,
                    None => {
                        error!(key = %key, "Setting is encrypted but encryption service not available");
                        return Err(SettingsError::InvalidConfig(
                            "Encryption service not configured".to_string(),
                        ));
                    }
                }
            } else {
                value
            };

            // Update cache
            {
                let mut cache = self.cache.write().await;
                cache.insert(
                    key.to_string(),
                    CachedSetting {
                        value: decrypted_value.clone(),
                        expires_at: Utc::now() + self.cache_ttl,
                    },
                );
            }

            debug!(key = %key, "Setting retrieved from database");
            Ok(Some(decrypted_value))
        } else {
            // Fallback to environment variable
            if let Ok(env_value) = env::var(key.to_uppercase()) {
                debug!(key = %key, "Setting retrieved from environment variable");
                return Ok(Some(env_value));
            }

            debug!(key = %key, "Setting not found");
            Ok(None)
        }
    }

    /// Set a setting value
    pub async fn set_setting(
        &self,
        key: &str,
        value: &str,
        encrypt: bool,
        updated_by: Option<&str>,
    ) -> Result<(), SettingsError> {
        // Validate encryption requirement
        if encrypt && self.encryption_service.is_none() {
            return Err(SettingsError::InvalidConfig(
                "Cannot encrypt setting: encryption service not configured".to_string(),
            ));
        }

        // Encrypt value if requested
        let stored_value = if encrypt {
            match &self.encryption_service {
                Some(service) => service.encrypt(value).map_err(|e| {
                    error!(key = %key, error = %e, "Failed to encrypt setting");
                    SettingsError::EncryptionError(e)
                })?,
                None => {
                    return Err(SettingsError::InvalidConfig(
                        "Encryption service not configured".to_string(),
                    ));
                }
            }
        } else {
            value.to_string()
        };

        // Insert or update in database
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, encrypted, updated_at, updated_by)
            VALUES (?, ?, ?, datetime('now'), ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                encrypted = excluded.encrypted,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by
            "#,
        )
        .bind(key)
        .bind(&stored_value)
        .bind(if encrypt { 1 } else { 0 })
        .bind(updated_by)
        .execute(&self.db_pool)
        .await?;

        // Invalidate cache for this key
        {
            let mut cache = self.cache.write().await;
            cache.remove(key);
        }

        info!(key = %key, encrypted = encrypt, "Setting updated successfully");
        Ok(())
    }

    /// Get all settings (decrypted)
    pub async fn get_all_settings(&self) -> Result<HashMap<String, String>, SettingsError> {
        let rows = sqlx::query_as::<_, (String, String, Option<i64>)>(
            "SELECT key, value, encrypted FROM system_settings ORDER BY key",
        )
        .fetch_all(&self.db_pool)
        .await?;

        let mut settings = HashMap::new();

        for (key, value, encrypted) in rows {
            let decrypted_value = if encrypted.unwrap_or(0) == 1 {
                match &self.encryption_service {
                    Some(service) => service.decrypt(&value).map_err(|e| {
                        error!(key = %key, error = %e, "Failed to decrypt setting");
                        SettingsError::EncryptionError(e)
                    })?,
                    None => {
                        warn!(key = %key, "Skipping encrypted setting: encryption service not available");
                        continue;
                    }
                }
            } else {
                value
            };

            settings.insert(key, decrypted_value);
        }

        debug!(count = settings.len(), "Retrieved all settings");
        Ok(settings)
    }

    /// Invalidate the entire cache
    pub async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("Settings cache invalidated");
    }

    /// Invalidate a specific cache entry
    pub async fn invalidate_cache_key(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
        debug!(key = %key, "Cache entry invalidated");
    }

    /// Check if encryption is available
    pub fn is_encryption_available(&self) -> bool {
        self.encryption_service.is_some()
    }

    /// Get multiple settings at once
    pub async fn get_settings(
        &self,
        keys: &[&str],
    ) -> Result<HashMap<String, Option<String>>, SettingsError> {
        let mut result = HashMap::new();

        for key in keys {
            let value = self.get_setting(key).await?;
            result.insert(key.to_string(), value);
        }

        Ok(result)
    }

    /// Delete a setting
    pub async fn delete_setting(&self, key: &str) -> Result<(), SettingsError> {
        sqlx::query("DELETE FROM system_settings WHERE key = ?")
            .bind(key)
            .execute(&self.db_pool)
            .await?;

        // Invalidate cache
        self.invalidate_cache_key(key).await;

        info!(key = %key, "Setting deleted");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Create table
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
    async fn test_set_and_get_setting() {
        let pool = setup_test_db().await;
        let service = SettingsService::new(pool);

        // Set a setting
        service
            .set_setting("test_key", "test_value", false, Some("admin"))
            .await
            .unwrap();

        // Get the setting
        let value = service.get_setting("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let pool = setup_test_db().await;
        let service = SettingsService::new(pool);

        // Set a setting
        service
            .set_setting("cached_key", "cached_value", false, Some("admin"))
            .await
            .unwrap();

        // First get - from database
        let value1 = service.get_setting("cached_key").await.unwrap();
        assert_eq!(value1, Some("cached_value".to_string()));

        // Second get - from cache
        let value2 = service.get_setting("cached_key").await.unwrap();
        assert_eq!(value2, Some("cached_value".to_string()));

        // Invalidate cache
        service.invalidate_cache_key("cached_key").await;

        // Third get - from database again
        let value3 = service.get_setting("cached_key").await.unwrap();
        assert_eq!(value3, Some("cached_value".to_string()));
    }

    #[tokio::test]
    async fn test_get_all_settings() {
        let pool = setup_test_db().await;
        let service = SettingsService::new(pool);

        // Set multiple settings
        service
            .set_setting("key1", "value1", false, Some("admin"))
            .await
            .unwrap();
        service
            .set_setting("key2", "value2", false, Some("admin"))
            .await
            .unwrap();
        service
            .set_setting("key3", "value3", false, Some("admin"))
            .await
            .unwrap();

        // Get all settings
        let all_settings = service.get_all_settings().await.unwrap();
        assert_eq!(all_settings.len(), 3);
        assert_eq!(all_settings.get("key1"), Some(&"value1".to_string()));
        assert_eq!(all_settings.get("key2"), Some(&"value2".to_string()));
        assert_eq!(all_settings.get("key3"), Some(&"value3".to_string()));
    }

    #[tokio::test]
    async fn test_delete_setting() {
        let pool = setup_test_db().await;
        let service = SettingsService::new(pool);

        // Set a setting
        service
            .set_setting("delete_me", "value", false, Some("admin"))
            .await
            .unwrap();

        // Verify it exists
        let value = service.get_setting("delete_me").await.unwrap();
        assert_eq!(value, Some("value".to_string()));

        // Delete it
        service.delete_setting("delete_me").await.unwrap();

        // Verify it's gone
        let value = service.get_setting("delete_me").await.unwrap();
        assert_eq!(value, None);
    }
}
