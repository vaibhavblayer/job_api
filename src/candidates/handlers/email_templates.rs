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
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #4F46E5;">Application Received</h2>
                        <p>Dear {},</p>
                        <p>Thank you for applying for the <strong>{}</strong> position at <strong>{}</strong>.</p>
                        <p>We have received your application and our team is currently reviewing it. We appreciate your interest in joining our organization.</p>
                        <p>We will contact you soon regarding the next steps in the hiring process.</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "shortlisted" => EmailTemplate {
            subject: format!("You've Been Shortlisted - {}", job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #10B981;">Congratulations! You've Been Shortlisted</h2>
                        <p>Dear {},</p>
                        <p>Great news! After reviewing your application for the <strong>{}</strong> position, we are pleased to inform you that you have been shortlisted for the next round.</p>
                        <p>Your qualifications and experience have impressed our team, and we would like to move forward with your application.</p>
                        <p>We will be in touch shortly with details about the next steps in our selection process.</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name
            ),
        },
        "interviewed" => EmailTemplate {
            subject: format!("Interview Invitation - {}", job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #4F46E5;">Interview Invitation</h2>
                        <p>Dear {},</p>
                        <p>We are pleased to invite you for an interview for the <strong>{}</strong> position at <strong>{}</strong>.</p>
                        <p>We were impressed with your application and would like to learn more about your experience and discuss how you can contribute to our team.</p>
                        <p>Our team will contact you shortly to schedule a convenient time for the interview.</p>
                        <p>We look forward to speaking with you!</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "offered" => EmailTemplate {
            subject: format!("Job Offer - {}", job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #10B981;">Congratulations! Job Offer</h2>
                        <p>Dear {},</p>
                        <p>We are delighted to offer you the position of <strong>{}</strong> at <strong>{}</strong>!</p>
                        <p>After careful consideration, we believe you would be an excellent addition to our team. Your skills, experience, and enthusiasm have impressed us throughout the selection process.</p>
                        <p>We will send you a formal offer letter with all the details including compensation, benefits, and start date shortly.</p>
                        <p>We look forward to welcoming you to our team!</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        "hired" => EmailTemplate {
            subject: format!("Welcome to {} - {}", company_name, job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #10B981;">Welcome Aboard!</h2>
                        <p>Dear {},</p>
                        <p>Welcome to <strong>{}</strong>! We are thrilled to have you join our team as <strong>{}</strong>.</p>
                        <p>We believe you will make significant contributions to our organization and we're excited to see you grow with us.</p>
                        <p>You will receive additional information about your onboarding process, including your start date, required documents, and first-day details.</p>
                        <p>Once again, welcome to the team!</p>
                        <p>Best regards,<br>
                        {} Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, company_name, job_title, company_name
            ),
        },
        "rejected" => EmailTemplate {
            subject: format!("Application Status Update - {}", job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #6B7280;">Application Status Update</h2>
                        <p>Dear {},</p>
                        <p>Thank you for your interest in the <strong>{}</strong> position at <strong>{}</strong> and for taking the time to apply.</p>
                        <p>After careful consideration, we have decided to move forward with other candidates whose qualifications more closely match our current needs.</p>
                        <p>We appreciate your interest in our organization and encourage you to apply for future opportunities that match your skills and experience.</p>
                        <p>We wish you all the best in your job search.</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name, company_name
            ),
        },
        _ => EmailTemplate {
            subject: format!("Application Update - {}", job_title),
            body: format!(
                r#"
                <html>
                <body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333;">
                    <div style="max-width: 600px; margin: 0 auto; padding: 20px;">
                        <h2 style="color: #4F46E5;">Application Status Update</h2>
                        <p>Dear {},</p>
                        <p>This is to inform you that there has been an update to your application for the <strong>{}</strong> position at <strong>{}</strong>.</p>
                        <p>We will contact you if we need any additional information or to discuss next steps.</p>
                        <p>Thank you for your continued interest.</p>
                        <p>Best regards,<br>
                        {} Hiring Team</p>
                    </div>
                </body>
                </html>
                "#,
                candidate_name, job_title, company_name, company_name
            ),
        },
    }
}

pub fn get_next_status(current_status: &str) -> Option<&'static str> {
    match current_status {
        "submitted" => Some("reviewed"),
        "reviewed" => Some("shortlisted"),
        "shortlisted" => Some("interviewed"),
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
        "interviewed" => Some(3),
        "offered" => Some(4),
        "hired" => Some(5),
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
