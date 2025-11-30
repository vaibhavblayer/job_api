// src/admin/handlers/contact.rs
//! Contact form handler - sends emails via AWS SES

use axum::{extract::Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::common::{ApiError, AppState};

#[derive(Debug, Deserialize)]
pub struct ContactFormRequest {
    pub name: String,
    pub email: String,
    pub subject: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ContactFormResponse {
    pub success: bool,
    pub message: String,
}

/// POST /api/public/contact - Submit contact form (public endpoint)
pub async fn submit_contact_form(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(request): Json<ContactFormRequest>,
) -> Result<Json<ContactFormResponse>, ApiError> {
    // Validate input
    if request.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name is required".to_string()));
    }
    if request.email.trim().is_empty() || !request.email.contains('@') {
        return Err(ApiError::BadRequest("Valid email is required".to_string()));
    }
    if request.subject.trim().is_empty() {
        return Err(ApiError::BadRequest("Subject is required".to_string()));
    }
    if request.message.trim().is_empty() {
        return Err(ApiError::BadRequest("Message is required".to_string()));
    }

    let state = state_lock.read().await;

    // Get admin email from settings (company contact email)
    let admin_email = state
        .settings_service
        .get_setting("company_email")
        .await
        .ok()
        .flatten()
        .or_else(|| {
            // Fallback to SES from email
            None
        });

    // Build email content
    let subject_line = format!("Contact Form: {} - {}", request.subject, request.name);
    let email_body = build_contact_email_html(&request);

    // Try to send via AWS SES if configured
    if let Some(to_email) = admin_email {
        match state
            .aws_service
            .send_email(vec![to_email.clone()], &subject_line, &email_body, None)
            .await
        {
            Ok(_) => {
                info!(
                    from_name = %request.name,
                    from_email = %request.email,
                    subject = %request.subject,
                    "Contact form email sent successfully"
                );
                return Ok(Json(ContactFormResponse {
                    success: true,
                    message: "Thank you for your message! We'll get back to you soon.".to_string(),
                }));
            }
            Err(e) => {
                error!(error = %e, "Failed to send contact form email via SES");
                // Fall through to store in database
            }
        }
    }

    // Store in database as fallback (or primary if email not configured)
    let contact_id = crate::common::generate_raw_id(12);
    let now = chrono::Utc::now().to_rfc3339();

    // Create contact_submissions table if it doesn't exist and insert
    let _ = sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS contact_submissions (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            subject TEXT NOT NULL,
            message TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            created_at TEXT DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(&state.db)
    .await;

    sqlx::query(
        r#"
        INSERT INTO contact_submissions (id, name, email, subject, message, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&contact_id)
    .bind(&request.name)
    .bind(&request.email)
    .bind(&request.subject)
    .bind(&request.message)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        contact_id = %contact_id,
        from_name = %request.name,
        from_email = %request.email,
        "Contact form submission stored in database"
    );

    Ok(Json(ContactFormResponse {
        success: true,
        message: "Thank you for your message! We'll get back to you soon.".to_string(),
    }))
}

/// Build HTML email content for contact form
fn build_contact_email_html(request: &ContactFormRequest) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; margin: 0; padding: 0; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%); color: white; padding: 30px; border-radius: 12px 12px 0 0; }}
        .header h1 {{ margin: 0; font-size: 24px; }}
        .content {{ padding: 30px; background-color: #f9fafb; border: 1px solid #e5e7eb; border-top: none; }}
        .field {{ margin-bottom: 20px; }}
        .field-label {{ font-size: 12px; font-weight: 600; color: #6b7280; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 4px; }}
        .field-value {{ font-size: 16px; color: #111827; }}
        .message-box {{ background: white; padding: 20px; border-radius: 8px; border: 1px solid #e5e7eb; margin-top: 20px; }}
        .footer {{ padding: 20px; text-align: center; font-size: 12px; color: #9ca3af; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📬 New Contact Form Submission</h1>
        </div>
        <div class="content">
            <div class="field">
                <div class="field-label">From</div>
                <div class="field-value">{} &lt;{}&gt;</div>
            </div>
            <div class="field">
                <div class="field-label">Subject</div>
                <div class="field-value">{}</div>
            </div>
            <div class="message-box">
                <div class="field-label">Message</div>
                <div class="field-value" style="white-space: pre-wrap;">{}</div>
            </div>
        </div>
        <div class="footer">
            <p>This message was sent via the website contact form.</p>
        </div>
    </div>
</body>
</html>"#,
        html_escape(&request.name),
        html_escape(&request.email),
        html_escape(&request.subject),
        html_escape(&request.message)
    )
}

/// Simple HTML escape function
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
