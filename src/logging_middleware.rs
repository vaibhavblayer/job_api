// src/logging_middleware.rs
//! Middleware for logging request and response bodies in debug mode

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum::body::to_bytes;
use tracing::debug;

/// Middleware to log request and response bodies in debug mode
pub async fn log_request_response(request: Request, next: Next) -> Result<Response, StatusCode> {
    let (parts, body) = request.into_parts();
    
    // Read request body
    let bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Log request body if not empty
    if !bytes.is_empty() {
        if let Ok(body_str) = std::str::from_utf8(&bytes) {
            // Try to parse as JSON for pretty printing
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(body_str) {
                debug!(
                    method = %parts.method,
                    uri = %parts.uri,
                    request_body = %serde_json::to_string_pretty(&json).unwrap_or_else(|_| body_str.to_string()),
                    "📥 Request"
                );
            } else {
                debug!(
                    method = %parts.method,
                    uri = %parts.uri,
                    request_body = %body_str,
                    "📥 Request"
                );
            }
        }
    }
    
    // Reconstruct request
    let request = Request::from_parts(parts, Body::from(bytes));
    
    // Call next middleware/handler
    let response = next.run(request).await;
    
    // Extract response parts
    let (parts, body) = response.into_parts();
    
    // Read response body
    let bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Log response body if not empty
    if !bytes.is_empty() {
        if let Ok(body_str) = std::str::from_utf8(&bytes) {
            // Try to parse as JSON for pretty printing
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(body_str) {
                debug!(
                    status = %parts.status,
                    response_body = %serde_json::to_string_pretty(&json).unwrap_or_else(|_| body_str.to_string()),
                    "📤 Response"
                );
            } else {
                debug!(
                    status = %parts.status,
                    response_body = %body_str,
                    "📤 Response"
                );
            }
        }
    }
    
    // Reconstruct response
    let response = Response::from_parts(parts, Body::from(bytes));
    
    Ok(response)
}
