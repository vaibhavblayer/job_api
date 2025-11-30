// src/candidates/handlers/email_templates.rs
//! Email templates for application status updates

use crate::candidates::models::Application;

pub struct EmailTemplate {
    pub subject: String,
    pub body: String,
}

pub fn get_email_template(
    status: &str,
    candidate_name: &str,
    job_title: &str,
    company_name: &str,
) -> EmailTemplate {
    match status {
        "reviewed" => EmailTemplate {
            subject: format!("Application Received - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>Thanks for applying for <strong>{}</strong> at <strong>{}</strong>. We've received your application and will review it shortly.</p>
<p>We'll be in touch with next steps soon.</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "shortlisted" => EmailTemplate {
            subject: format!("You're Shortlisted! - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>Great news! You've been shortlisted for the <strong>{}</strong> position at <strong>{}</strong>.</p>
<p>We'll contact you soon to discuss next steps.</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "interviewed" => EmailTemplate {
            subject: format!("Interview Invitation - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>We'd like to invite you for an interview for <strong>{}</strong> at <strong>{}</strong>.</p>
<p>We'll reach out shortly to schedule a time that works for you.</p>
<p>Looking forward to speaking with you!</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "offered" => EmailTemplate {
            subject: format!("Job Offer - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>Congratulations! We're pleased to offer you the <strong>{}</strong> position at <strong>{}</strong>.</p>
<p>You'll receive a formal offer letter with compensation details shortly.</p>
<p>Welcome to the team!</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "hired" => EmailTemplate {
            subject: format!("Welcome to {}!", company_name),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>Welcome to <strong>{}</strong>! We're excited to have you join us as <strong>{}</strong>.</p>
<p>You'll receive onboarding details including your start date and first-day information soon.</p>
<p>See you soon!</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, company_name, job_title, company_name
            ),
        },
        "rejected" => EmailTemplate {
            subject: format!("Application Update - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>Thank you for your interest in <strong>{}</strong> at <strong>{}</strong>.</p>
<p>After careful review, we've decided to move forward with other candidates. We encourage you to apply for future openings that match your skills.</p>
<p>Best of luck in your search!</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        _ => EmailTemplate {
            subject: format!("Application Update - {}", job_title),
            body: format!(
                r#"<html><body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
<div style="max-width: 600px; margin: 0 auto; padding: 20px;">
<p>Hi {},</p>
<p>There's an update on your application for <strong>{}</strong> at <strong>{}</strong>.</p>
<p>We'll be in touch if we need anything further.</p>
<p>Best,<br>{} Team</p>
</div></body></html>"#,
                candidate_name, job_title, company_name, company_name
            ),
        },
    }
}

pub fn get_next_status(current_status: &str) -> Option<&'static str> {
    match current_status {
        "submitted" => Some("reviewed"),
        "reviewed" => Some("shortlisted"),
        "shortlisted" => Some("interview_scheduled"), // Now requires scheduling interview
        "interview_scheduled" => Some("interviewed"),
        "interviewed" => Some("offered"),
        "offered" => Some("hired"),
        _ => None,
    }
}

/// Map application status to current_stage field
pub fn status_to_stage(status: &str) -> &'static str {
    match status {
        "submitted" => "Applied",
        "reviewed" => "Resume Review",
        "shortlisted" => "Shortlisted",
        "interview_scheduled" => "Interview Scheduled",
        "interviewed" => "Interview Completed",
        "offered" => "Offer Extended",
        "hired" => "Hired",
        "rejected" => "Rejected",
        "withdrawn" => "Applied",
        _ => "Applied",
    }
}

/// Get the stage order index for comparison
pub fn get_status_order(status: &str) -> Option<u8> {
    match status {
        "submitted" => Some(0),
        "reviewed" => Some(1),
        "shortlisted" => Some(2),
        "interview_scheduled" => Some(3),
        "interviewed" => Some(4),
        "offered" => Some(5),
        "hired" => Some(6),
        "rejected" => Some(99), // Can happen at any stage
        "withdrawn" => Some(98), // Can happen at any stage
        _ => None,
    }
}

/// Check if a status transition is valid
/// Returns Ok(()) if valid, Err with message if invalid
pub fn validate_status_transition(current_status: &str, new_status: &str) -> Result<(), String> {
    // Same status is always valid (no-op)
    if current_status == new_status {
        return Ok(());
    }
    
    // Can always reject or withdraw from any non-final state
    if new_status == "rejected" || new_status == "withdrawn" {
        if current_status == "hired" {
            return Err("Cannot reject or withdraw a hired candidate".to_string());
        }
        return Ok(());
    }
    
    // Cannot change status of rejected/withdrawn/hired applications (except to reject)
    if current_status == "rejected" || current_status == "withdrawn" || current_status == "hired" {
        return Err(format!("Cannot change status from '{}' to '{}'", current_status, new_status));
    }
    
    // For forward progression, check the order
    let current_order = get_status_order(current_status)
        .ok_or_else(|| format!("Invalid current status: {}", current_status))?;
    let new_order = get_status_order(new_status)
        .ok_or_else(|| format!("Invalid new status: {}", new_status))?;
    
    // Allow forward progression only (no skipping stages)
    if new_order == current_order + 1 {
        return Ok(());
    }
    
    // Allow admins to skip stages forward (but not backward)
    if new_order > current_order {
        return Ok(());
    }
    
    Err(format!("Invalid status transition from '{}' to '{}'. Status can only move forward.", current_status, new_status))
}
