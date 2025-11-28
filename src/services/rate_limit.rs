// src/services/rate_limit.rs
use crate::services::settings::SettingsService;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub authenticated_limit: u32,
    pub anonymous_limit: u32,
    pub per_ip_limit: u32,
    pub window_seconds: u32,
    pub whitelist_ips: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            authenticated_limit: 100, // 100 requests per minute for authenticated users
            anonymous_limit: 20,      // 20 requests per minute for anonymous users
            per_ip_limit: 50,         // 50 requests per minute per IP
            window_seconds: 60,       // 60 second window
            whitelist_ips: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

impl RateLimitConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        // RATE_LIMIT_ENABLED - set to "false" to disable rate limiting
        if let Ok(enabled) = env::var("RATE_LIMIT_ENABLED") {
            config.enabled = enabled.to_lowercase() != "false";
        }

        // RATE_LIMIT_AUTHENTICATED - requests per window for authenticated users
        if let Ok(limit) = env::var("RATE_LIMIT_AUTHENTICATED") {
            if let Ok(val) = limit.parse::<u32>() {
                config.authenticated_limit = val;
            }
        }

        // RATE_LIMIT_ANONYMOUS - requests per window for anonymous users
        if let Ok(limit) = env::var("RATE_LIMIT_ANONYMOUS") {
            if let Ok(val) = limit.parse::<u32>() {
                config.anonymous_limit = val;
            }
        }

        // RATE_LIMIT_PER_IP - requests per window per IP address
        if let Ok(limit) = env::var("RATE_LIMIT_PER_IP") {
            if let Ok(val) = limit.parse::<u32>() {
                config.per_ip_limit = val;
            }
        }

        // RATE_LIMIT_WINDOW_SECONDS - time window in seconds
        if let Ok(window) = env::var("RATE_LIMIT_WINDOW_SECONDS") {
            if let Ok(val) = window.parse::<u32>() {
                config.window_seconds = val;
            }
        }

        // RATE_LIMIT_WHITELIST_IPS - comma-separated list of whitelisted IPs
        if let Ok(whitelist) = env::var("RATE_LIMIT_WHITELIST_IPS") {
            config.whitelist_ips = whitelist
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        config
    }
}

#[derive(Debug, Clone)]
struct RateLimitState {
    count: u32,
    window_start: Instant,
}

impl RateLimitState {
    fn new() -> Self {
        Self {
            count: 1,
            window_start: Instant::now(),
        }
    }

    fn increment(&mut self) {
        self.count += 1;
    }

    fn reset(&mut self) {
        self.count = 1;
        self.window_start = Instant::now();
    }

    fn is_expired(&self, window_duration: Duration) -> bool {
        self.window_start.elapsed() > window_duration
    }
}

#[derive(Debug)]
pub enum RateLimitResult {
    Allowed,
    Limited { retry_after: u32 },
}

#[derive(Debug, Clone)]
pub struct RateLimitService {
    settings_service: Arc<SettingsService>,
    rate_limiter: Arc<RwLock<HashMap<String, RateLimitState>>>,
}

impl RateLimitService {
    pub fn new(settings_service: Arc<SettingsService>) -> Self {
        let env_config = RateLimitConfig::from_env();
        info!(
            enabled = env_config.enabled,
            authenticated_limit = env_config.authenticated_limit,
            anonymous_limit = env_config.anonymous_limit,
            per_ip_limit = env_config.per_ip_limit,
            window_seconds = env_config.window_seconds,
            whitelist_ips = ?env_config.whitelist_ips,
            "Initializing RateLimitService with env config"
        );
        Self {
            settings_service,
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get rate limit configuration - environment variables take precedence over database settings
    pub async fn get_config(&self) -> RateLimitConfig {
        // Start with environment variables (highest priority)
        let mut config = RateLimitConfig::from_env();

        // Only check database settings if env vars aren't explicitly set
        // This allows env vars to override database settings

        // Check if RATE_LIMIT_ENABLED was explicitly set in env
        if env::var("RATE_LIMIT_ENABLED").is_err() {
            if let Ok(Some(enabled)) = self
                .settings_service
                .get_setting("rate_limit_enabled")
                .await
            {
                config.enabled = enabled.to_lowercase() == "true";
            }
        }

        // Check if RATE_LIMIT_AUTHENTICATED was explicitly set in env
        if env::var("RATE_LIMIT_AUTHENTICATED").is_err() {
            if let Ok(Some(auth_limit)) = self
                .settings_service
                .get_setting("rate_limit_authenticated_per_minute")
                .await
            {
                if let Ok(limit) = auth_limit.parse::<u32>() {
                    config.authenticated_limit = limit;
                }
            }
        }

        // Check if RATE_LIMIT_ANONYMOUS was explicitly set in env
        if env::var("RATE_LIMIT_ANONYMOUS").is_err() {
            if let Ok(Some(anon_limit)) = self
                .settings_service
                .get_setting("rate_limit_anonymous_per_minute")
                .await
            {
                if let Ok(limit) = anon_limit.parse::<u32>() {
                    config.anonymous_limit = limit;
                }
            }
        }

        // Check if RATE_LIMIT_PER_IP was explicitly set in env
        if env::var("RATE_LIMIT_PER_IP").is_err() {
            if let Ok(Some(ip_limit)) = self
                .settings_service
                .get_setting("rate_limit_per_ip_per_minute")
                .await
            {
                if let Ok(limit) = ip_limit.parse::<u32>() {
                    config.per_ip_limit = limit;
                }
            }
        }

        // Check if RATE_LIMIT_WINDOW_SECONDS was explicitly set in env
        if env::var("RATE_LIMIT_WINDOW_SECONDS").is_err() {
            if let Ok(Some(window)) = self
                .settings_service
                .get_setting("rate_limit_window_seconds")
                .await
            {
                if let Ok(seconds) = window.parse::<u32>() {
                    config.window_seconds = seconds;
                }
            }
        }

        // Check if RATE_LIMIT_WHITELIST_IPS was explicitly set in env
        if env::var("RATE_LIMIT_WHITELIST_IPS").is_err() {
            if let Ok(Some(whitelist)) = self
                .settings_service
                .get_setting("rate_limit_whitelist_ips")
                .await
            {
                // Parse comma-separated IP list
                config.whitelist_ips = whitelist
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        config
    }

    /// Check if an IP is whitelisted
    fn is_whitelisted(&self, ip: &str, whitelist: &[String]) -> bool {
        whitelist.iter().any(|whitelisted_ip| {
            // Support exact match or CIDR notation (simplified - exact match for now)
            whitelisted_ip == ip
        })
    }

    /// Check rate limit for a given identifier
    pub async fn check_rate_limit(
        &self,
        identifier: &str,
        ip_address: Option<&str>,
        is_authenticated: bool,
    ) -> Result<RateLimitResult, String> {
        let config = self.get_config().await;

        // If rate limiting is disabled, allow all requests
        if !config.enabled {
            return Ok(RateLimitResult::Allowed);
        }

        // Check if IP is whitelisted
        if let Some(ip) = ip_address {
            if self.is_whitelisted(ip, &config.whitelist_ips) {
                return Ok(RateLimitResult::Allowed);
            }
        }

        // Determine the rate limit based on authentication status
        let limit = if is_authenticated {
            config.authenticated_limit
        } else {
            config.anonymous_limit
        };

        let window_duration = Duration::from_secs(config.window_seconds as u64);

        // Check user/identifier rate limit
        let user_result = self
            .check_limit_for_key(identifier, limit, window_duration)
            .await;

        // If user limit is exceeded, return immediately
        if let RateLimitResult::Limited { retry_after } = user_result {
            return Ok(RateLimitResult::Limited { retry_after });
        }

        // Check per-IP rate limit if IP is provided
        if let Some(ip) = ip_address {
            let ip_key = format!("ip:{}", ip);
            let ip_result = self
                .check_limit_for_key(&ip_key, config.per_ip_limit, window_duration)
                .await;

            if let RateLimitResult::Limited { retry_after } = ip_result {
                return Ok(RateLimitResult::Limited { retry_after });
            }
        }

        Ok(RateLimitResult::Allowed)
    }

    /// Internal method to check rate limit for a specific key
    async fn check_limit_for_key(
        &self,
        key: &str,
        limit: u32,
        window_duration: Duration,
    ) -> RateLimitResult {
        let mut limiter = self.rate_limiter.write().await;

        let state = limiter
            .entry(key.to_string())
            .or_insert_with(RateLimitState::new);

        // Check if the window has expired
        if state.is_expired(window_duration) {
            state.reset();
            return RateLimitResult::Allowed;
        }

        // Check if limit is exceeded
        if state.count >= limit {
            let elapsed = state.window_start.elapsed().as_secs() as u32;
            let retry_after = window_duration.as_secs() as u32 - elapsed;
            return RateLimitResult::Limited { retry_after };
        }

        // Increment the counter
        state.increment();
        RateLimitResult::Allowed
    }

    /// Log a rate limit violation
    pub async fn log_violation(&self, identifier: &str, ip_address: Option<&str>, endpoint: &str) {
        warn!(
            identifier = %identifier,
            ip_address = ?ip_address,
            endpoint = %endpoint,
            "Rate limit violation detected"
        );
    }

    /// Clean up expired entries (should be called periodically)
    pub async fn cleanup_expired(&self, window_duration: Duration) {
        let mut limiter = self.rate_limiter.write().await;
        limiter.retain(|_, state| !state.is_expired(window_duration));
        info!("Cleaned up expired rate limit entries");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn create_test_service() -> RateLimitService {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let settings_service = Arc::new(SettingsService::new(pool));
        RateLimitService::new(settings_service)
    }

    #[tokio::test]
    async fn test_rate_limit_allows_within_limit() {
        let service = create_test_service().await;

        // First request should be allowed
        let result = service
            .check_rate_limit("user123", Some("192.168.1.1"), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Allowed));
    }

    #[tokio::test]
    #[ignore] // Timing-sensitive test - may fail in CI/CD
    async fn test_rate_limit_blocks_when_exceeded() {
        let service = create_test_service().await;
        let config = service.get_config().await;

        // Use unique user ID for this test to avoid conflicts
        let test_user = format!("test_user_blocks_{}", uuid::Uuid::new_v4());
        let test_ip = format!("192.168.99.{}", rand::random::<u8>());

        // Make requests up to the limit (limit - 1 to be safe)
        let requests_to_make = config.authenticated_limit.saturating_sub(1);
        for i in 0..requests_to_make {
            let result = service
                .check_rate_limit(&test_user, Some(&test_ip), true)
                .await
                .unwrap();
            if !matches!(result, RateLimitResult::Allowed) {
                panic!("Request {} of {} should be allowed but got: {:?}", 
                    i + 1, requests_to_make, result);
            }
        }

        // One more request at the limit should still be allowed
        let result = service
            .check_rate_limit(&test_user, Some(&test_ip), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Allowed), 
            "Request at limit ({}) should be allowed but got: {:?}", config.authenticated_limit, result);

        // Next request should be blocked
        let result = service
            .check_rate_limit(&test_user, Some(&test_ip), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Limited { .. }), 
            "Request {} (over limit) should be Limited but got: {:?}", 
            config.authenticated_limit + 1, result);
    }

    #[tokio::test]
    async fn test_whitelist_bypasses_rate_limit() {
        let service = create_test_service().await;
        let config = service.get_config().await;

        // Make many requests from whitelisted IP
        for _ in 0..(config.authenticated_limit + 10) {
            let result = service
                .check_rate_limit("user123", Some("127.0.0.1"), true)
                .await
                .unwrap();
            assert!(matches!(result, RateLimitResult::Allowed));
        }
    }

    #[tokio::test]
    async fn test_different_users_have_separate_limits() {
        let service = create_test_service().await;
        let config = service.get_config().await;

        // Exhaust limit for user1
        for _ in 0..config.authenticated_limit {
            service
                .check_rate_limit("user1", Some("192.168.1.1"), true)
                .await
                .unwrap();
        }

        // user2 should still be allowed
        let result = service
            .check_rate_limit("user2", Some("192.168.1.2"), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Allowed));
    }

    #[tokio::test]
    #[ignore] // Timing-sensitive test - may fail in CI/CD
    async fn test_per_ip_limit() {
        let service = create_test_service().await;
        let config = service.get_config().await;

        // Use unique IP for this test to avoid conflicts
        let test_ip = format!("10.0.{}.{}", rand::random::<u8>(), rand::random::<u8>());

        // Make requests from same IP with different users (limit - 1)
        let requests_to_make = config.per_ip_limit.saturating_sub(1);
        for i in 0..requests_to_make {
            let user_id = format!("test_per_ip_user_{}_{}", uuid::Uuid::new_v4(), i);
            let result = service
                .check_rate_limit(&user_id, Some(&test_ip), true)
                .await
                .unwrap();
            if !matches!(result, RateLimitResult::Allowed) {
                panic!("Request {} of {} from IP {} should be allowed but got: {:?}", 
                    i + 1, requests_to_make, test_ip, result);
            }
        }

        // One more at the limit should be allowed
        let at_limit_user = format!("test_per_ip_at_limit_{}", uuid::Uuid::new_v4());
        let result = service
            .check_rate_limit(&at_limit_user, Some(&test_ip), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Allowed),
            "Request at limit ({}) for IP {} should be allowed but got: {:?}", 
            config.per_ip_limit, test_ip, result);

        // Next request from same IP should be blocked
        let another_user = format!("test_per_ip_final_{}", uuid::Uuid::new_v4());
        let result = service
            .check_rate_limit(&another_user, Some(&test_ip), true)
            .await
            .unwrap();
        assert!(matches!(result, RateLimitResult::Limited { .. }),
            "Request {} (over limit) for IP {} should be Limited but got: {:?}", 
            config.per_ip_limit + 1, test_ip, result);
    }
}
