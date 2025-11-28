// rate_limit_middleware.rs
use crate::services::rate_limit::{RateLimitResult, RateLimitService};
use axum::{
    extract::{ConnectInfo, Extension, Request},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, warn};

#[derive(Serialize)]
struct RateLimitErrorResponse {
    error: String,
    code: String,
    retry_after: u32,
}

/// Extract IP address from request
fn extract_ip_address(
    headers: &HeaderMap,
    connect_info: Option<&ConnectInfo<SocketAddr>>,
) -> Option<String> {
    // Try X-Forwarded-For header first (for proxied requests)
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // Take the first IP in the chain
            if let Some(first_ip) = forwarded_str.split(',').next() {
                return Some(first_ip.trim().to_string());
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return Some(ip_str.to_string());
        }
    }

    // Fall back to connection info
    connect_info.map(|info| info.0.ip().to_string())
}

/// Extract user identifier from JWT token in Authorization header
fn extract_user_identifier(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|auth| {
            // Extract token from "Bearer <token>" format
            if let Some(token) = auth.strip_prefix("Bearer ") {
                // For now, use a hash of the token as identifier
                // In production, you'd decode the JWT to get the user ID
                Some(format!("token:{}", &token[..token.len().min(20)]))
            } else {
                None
            }
        })
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    Extension(rate_limit_service): Extension<Arc<RateLimitService>>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    // Note: Rate limiting enabled/disabled is now controlled via RATE_LIMIT_ENABLED env var
    // and checked inside the rate_limit_service.check_rate_limit() method

    let headers = request.headers().clone();

    // Extract IP address
    let ip_address = extract_ip_address(&headers, connect_info.as_ref());

    // Extract user identifier (from JWT token)
    let user_identifier = extract_user_identifier(&headers);
    let is_authenticated = user_identifier.is_some();

    // Use IP as identifier if no user token is present
    let identifier = user_identifier
        .or_else(|| ip_address.clone().map(|ip| format!("anon:{}", ip)))
        .unwrap_or_else(|| "unknown".to_string());

    // Get request path for logging
    let path = request.uri().path().to_string();

    // Check rate limit
    match rate_limit_service
        .check_rate_limit(&identifier, ip_address.as_deref(), is_authenticated)
        .await
    {
        Ok(RateLimitResult::Allowed) => {
            debug!(
                identifier = %identifier,
                ip = ?ip_address,
                path = %path,
                "Request allowed by rate limiter"
            );
            Ok(next.run(request).await)
        }
        Ok(RateLimitResult::Limited { retry_after }) => {
            warn!(
                identifier = %identifier,
                ip = ?ip_address,
                path = %path,
                retry_after = retry_after,
                "Request blocked by rate limiter"
            );

            // Log the violation
            rate_limit_service
                .log_violation(&identifier, ip_address.as_deref(), &path)
                .await;

            // Return 429 Too Many Requests with retry-after header
            let error_response = RateLimitErrorResponse {
                error: "Rate limit exceeded. Please try again later.".to_string(),
                code: "RATE_LIMIT_EXCEEDED".to_string(),
                retry_after,
            };

            let mut response =
                (StatusCode::TOO_MANY_REQUESTS, Json(error_response)).into_response();

            // Add Retry-After header
            if let Ok(retry_header) = HeaderValue::from_str(&retry_after.to_string()) {
                response.headers_mut().insert("retry-after", retry_header);
            }

            // Add X-RateLimit headers for client information
            if let Ok(limit_header) = HeaderValue::from_str("exceeded") {
                response
                    .headers_mut()
                    .insert("x-ratelimit-limit", limit_header);
            }

            Err(response)
        }
        Err(e) => {
            warn!(
                error = %e,
                identifier = %identifier,
                "Error checking rate limit, allowing request"
            );
            // On error, allow the request to proceed
            Ok(next.run(request).await)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_extract_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 198.51.100.1".parse().unwrap(),
        );

        let ip = extract_ip_address(&headers, None);
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }

    #[test]
    fn test_extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.1".parse().unwrap());

        let ip = extract_ip_address(&headers, None);
        assert_eq!(ip, Some("203.0.113.1".to_string()));
    }

    #[test]
    fn test_extract_user_identifier_from_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
                .parse()
                .unwrap(),
        );

        let identifier = extract_user_identifier(&headers);
        assert!(identifier.is_some());
        assert!(identifier.unwrap().starts_with("token:"));
    }

    #[test]
    fn test_extract_user_identifier_no_token() {
        let headers = HeaderMap::new();
        let identifier = extract_user_identifier(&headers);
        assert!(identifier.is_none());
    }
}
