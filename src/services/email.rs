// src/services/email.rs
use serde::{Deserialize, Serialize};

/// Email template data for generating stage-specific emails
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailTemplateData {
    pub candidate_name: String,
    pub job_title: String,
    pub company_name: String,
    pub stage: String,
    pub additional_context: Option<String>,
}

#[allow(dead_code)]
pub fn generate_interview_invitation_email(data: &EmailTemplateData) -> String {
    let additional_info = data.additional_context.as_deref().unwrap_or("");

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background-color: #4F46E5; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background-color: #f9f9f9; }}
        .footer {{ padding: 20px; text-align: center; font-size: 12px; color: #666; }}
        .button {{ display: inline-block; padding: 12px 24px; background-color: #4F46E5; color: white; text-decoration: none; border-radius: 5px; margin: 10px 0; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Interview Invitation</h1>
        </div>
        <div class="content">
            <p>Dear {},</p>
            
            <p>Congratulations! We are pleased to invite you for an interview for the <strong>{}</strong> position at <strong>{}</strong>.</p>
            
            <p>We were impressed with your application and would like to learn more about your qualifications and experience.</p>
            
            {}
            
            <p>Please confirm your availability at your earliest convenience.</p>
            
            <p>We look forward to speaking with you!</p>
            
            <p>Best regards,<br>
            {} Hiring Team</p>
        </div>
        <div class="footer">
            <p>This is an automated message. Please do not reply directly to this email.</p>
        </div>
    </div>
</body>
</html>"#,
        data.candidate_name,
        data.job_title,
        data.company_name,
        if !additional_info.is_empty() {
            format!(
                "<p><strong>Interview Details:</strong><br>{}</p>",
                additional_info
            )
        } else {
            String::new()
        },
        data.company_name
    )
}

#[allow(dead_code)]
pub fn generate_offer_letter_email(data: &EmailTemplateData) -> String {
    let additional_info = data.additional_context.as_deref().unwrap_or("");

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background-color: #10B981; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background-color: #f9f9f9; }}
        .footer {{ padding: 20px; text-align: center; font-size: 12px; color: #666; }}
        .highlight {{ background-color: #D1FAE5; padding: 15px; border-left: 4px solid #10B981; margin: 15px 0; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🎉 Congratulations!</h1>
        </div>
        <div class="content">
            <p>Dear {},</p>
            
            <div class="highlight">
                <p><strong>We are delighted to offer you the position of {} at {}!</strong></p>
            </div>
            
            <p>After careful consideration of your qualifications, experience, and interview performance, we believe you would be an excellent addition to our team.</p>
            
            {}
            
            <p>Please review the attached offer letter carefully. If you have any questions or need clarification on any aspect of the offer, please don't hesitate to reach out.</p>
            
            <p>We are excited about the possibility of you joining our team and look forward to your response.</p>
            
            <p>Warm regards,<br>
            {} Hiring Team</p>
        </div>
        <div class="footer">
            <p>This is an automated message. Please do not reply directly to this email.</p>
        </div>
    </div>
</body>
</html>"#,
        data.candidate_name,
        data.job_title,
        data.company_name,
        if !additional_info.is_empty() {
            format!(
                "<p><strong>Offer Details:</strong><br>{}</p>",
                additional_info
            )
        } else {
            String::new()
        },
        data.company_name
    )
}

#[allow(dead_code)]
pub fn generate_rejection_email(data: &EmailTemplateData) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background-color: #6B7280; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background-color: #f9f9f9; }}
        .footer {{ padding: 20px; text-align: center; font-size: 12px; color: #666; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Application Update</h1>
        </div>
        <div class="content">
            <p>Dear {},</p>
            
            <p>Thank you for your interest in the <strong>{}</strong> position at <strong>{}</strong> and for taking the time to apply and interview with us.</p>
            
            <p>After careful consideration, we have decided to move forward with other candidates whose qualifications more closely match our current needs.</p>
            
            <p>We were impressed by your background and experience, and we encourage you to apply for future opportunities that align with your skills and career goals.</p>
            
            <p>We wish you all the best in your job search and future endeavors.</p>
            
            <p>Best regards,<br>
            {} Hiring Team</p>
        </div>
        <div class="footer">
            <p>This is an automated message. Please do not reply directly to this email.</p>
        </div>
    </div>
</body>
</html>"#,
        data.candidate_name, data.job_title, data.company_name, data.company_name
    )
}

#[allow(dead_code)]
pub fn generate_welcome_email(data: &EmailTemplateData) -> String {
    let additional_info = data.additional_context.as_deref().unwrap_or("");

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background-color: #8B5CF6; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background-color: #f9f9f9; }}
        .footer {{ padding: 20px; text-align: center; font-size: 12px; color: #666; }}
        .welcome-box {{ background-color: #EDE9FE; padding: 20px; border-radius: 8px; margin: 15px 0; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🎊 Welcome to the Team!</h1>
        </div>
        <div class="content">
            <p>Dear {},</p>
            
            <div class="welcome-box">
                <p><strong>Welcome to {}!</strong></p>
                <p>We are thrilled to have you join us as our new {}.</p>
            </div>
            
            <p>Your journey with us begins now, and we're excited to see the contributions you'll make to our team.</p>
            
            {}
            
            <p>If you have any questions before your start date, please don't hesitate to reach out.</p>
            
            <p>Once again, welcome aboard!</p>
            
            <p>Best regards,<br>
            {} Team</p>
        </div>
        <div class="footer">
            <p>This is an automated message. Please do not reply directly to this email.</p>
        </div>
    </div>
</body>
</html>"#,
        data.candidate_name,
        data.company_name,
        data.job_title,
        if !additional_info.is_empty() {
            format!("<p><strong>Next Steps:</strong><br>{}</p>", additional_info)
        } else {
            String::new()
        },
        data.company_name
    )
}
