// src/candidates/handlers/interview_email_templates.rs
//! Email templates for interview scheduling and notifications

pub struct InterviewEmailTemplate {
    pub subject: String,
    pub body: String,
}

/// Generate interview scheduled email template
pub fn get_interview_scheduled_template(
    candidate_name: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
    duration_minutes: i32,
    interview_type: &str,
    google_meet_link: Option<&str>,
    notes: Option<&str>,
) -> InterviewEmailTemplate {
    let meet_link_html = if let Some(link) = google_meet_link {
        format!(
            r#"
            <div style="background-color: #EDE9FE; border-left: 4px solid #7C3AED; padding: 15px; margin: 20px 0;">
                <p style="margin: 0 0 10px 0; font-weight: 600; color: #5B21B6;">📹 Google Meet Link:</p>
                <a href="{}" target="_blank" rel="noopener noreferrer" 
                   style="color: #7C3AED; text-decoration: none; font-weight: 500; word-break: break-all;">
                    {}
                </a>
            </div>
            "#,
            link, link
        )
    } else {
        String::from(
            r#"
            <div style="background-color: #FEF3C7; border-left: 4px solid #F59E0B; padding: 15px; margin: 20px 0;">
                <p style="margin: 0; color: #92400E;">
                    <strong>Note:</strong> Meeting link will be shared separately.
                </p>
            </div>
            "#
        )
    };

    let notes_html = if let Some(note_text) = notes {
        if !note_text.trim().is_empty() {
            format!(
                r#"
                <div style="background-color: #F3F4F6; padding: 15px; border-radius: 8px; margin: 20px 0;">
                    <p style="margin: 0 0 5px 0; font-weight: 600; color: #374151;">Additional Notes:</p>
                    <p style="margin: 0; color: #6B7280; white-space: pre-wrap;">{}</p>
                </div>
                "#,
                note_text
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    InterviewEmailTemplate {
        subject: format!("Interview Scheduled - {} at {}", job_title, company_name),
        body: format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            line-height: 1.6;
            color: #1F2937;
            margin: 0;
            padding: 0;
            background-color: #F9FAFB;
        }}
        .container {{
            max-width: 600px;
            margin: 0 auto;
            background-color: #FFFFFF;
        }}
        .header {{
            background: linear-gradient(135deg, #667EEA 0%, #764BA2 100%);
            color: white;
            padding: 40px 30px;
            text-align: center;
        }}
        .header h1 {{
            margin: 0;
            font-size: 28px;
            font-weight: 700;
        }}
        .content {{
            padding: 40px 30px;
        }}
        .greeting {{
            font-size: 18px;
            color: #111827;
            margin-bottom: 20px;
        }}
        .details-box {{
            background-color: #F3F4F6;
            border-radius: 12px;
            padding: 25px;
            margin: 25px 0;
        }}
        .detail-row {{
            display: flex;
            padding: 12px 0;
            border-bottom: 1px solid #E5E7EB;
        }}
        .detail-row:last-child {{
            border-bottom: none;
        }}
        .detail-label {{
            font-weight: 600;
            color: #374151;
            min-width: 140px;
        }}
        .detail-value {{
            color: #6B7280;
            flex: 1;
        }}
        .cta-button {{
            display: inline-block;
            background: linear-gradient(135deg, #667EEA 0%, #764BA2 100%);
            color: white;
            padding: 14px 32px;
            text-decoration: none;
            border-radius: 8px;
            font-weight: 600;
            margin: 20px 0;
            text-align: center;
        }}
        .footer {{
            background-color: #F9FAFB;
            padding: 30px;
            text-align: center;
            color: #6B7280;
            font-size: 14px;
        }}
        .divider {{
            height: 1px;
            background-color: #E5E7EB;
            margin: 30px 0;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🎯 Interview Scheduled</h1>
        </div>
        
        <div class="content">
            <p class="greeting">Dear <strong>{}</strong>,</p>
            
            <p>Great news! Your interview has been scheduled for the <strong>{}</strong> position at <strong>{}</strong>.</p>
            
            <p>We're excited to learn more about your experience and discuss how you can contribute to our team.</p>
            
            <div class="details-box">
                <div class="detail-row">
                    <span class="detail-label">📅 Date & Time:</span>
                    <span class="detail-value">{}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">⏱️ Duration:</span>
                    <span class="detail-value">{} minutes</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">💼 Interview Type:</span>
                    <span class="detail-value">{}</span>
                </div>
            </div>
            
            {}
            
            {}
            
            <div class="divider"></div>
            
            <h3 style="color: #111827; margin-bottom: 15px;">📋 Preparation Tips:</h3>
            <ul style="color: #6B7280; line-height: 1.8;">
                <li>Test your internet connection and audio/video setup beforehand</li>
                <li>Join the meeting 5 minutes early</li>
                <li>Have your resume and any relevant documents ready</li>
                <li>Prepare questions about the role and company</li>
                <li>Find a quiet, well-lit space for the interview</li>
            </ul>
            
            <p style="margin-top: 30px;">If you need to reschedule or have any questions, please don't hesitate to reach out to us.</p>
            
            <p style="margin-top: 30px;">We look forward to speaking with you!</p>
            
            <p style="margin-top: 20px;">
                Best regards,<br>
                <strong>{} Hiring Team</strong>
            </p>
        </div>
        
        <div class="footer">
            <p>This is an automated message from {} recruitment system.</p>
            <p style="margin-top: 10px;">Please do not reply directly to this email.</p>
        </div>
    </div>
</body>
</html>"#,
            candidate_name,
            job_title,
            company_name,
            scheduled_date,
            duration_minutes,
            interview_type,
            meet_link_html,
            notes_html,
            company_name,
            company_name
        ),
    }
}

/// Generate interview reminder email template (for sending before interview)
pub fn get_interview_reminder_template(
    candidate_name: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
    google_meet_link: Option<&str>,
) -> InterviewEmailTemplate {
    let meet_link_html = if let Some(link) = google_meet_link {
        format!(
            r#"
            <div style="text-align: center; margin: 30px 0;">
                <a href="{}" target="_blank" rel="noopener noreferrer" 
                   style="display: inline-block; background: linear-gradient(135deg, #667EEA 0%, #764BA2 100%); 
                          color: white; padding: 16px 40px; text-decoration: none; border-radius: 8px; 
                          font-weight: 600; font-size: 16px;">
                    Join Interview Now
                </a>
            </div>
            "#,
            link
        )
    } else {
        String::from(
            r#"
            <p style="text-align: center; color: #F59E0B; font-weight: 600;">
                Meeting link will be shared separately.
            </p>
            "#
        )
    };

    InterviewEmailTemplate {
        subject: format!("Reminder: Interview Today - {} at {}", job_title, company_name),
        body: format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; margin: 0; padding: 0; background-color: #F9FAFB;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #FFFFFF;">
        <div style="background: linear-gradient(135deg, #10B981 0%, #059669 100%); color: white; padding: 40px 30px; text-align: center;">
            <h1 style="margin: 0; font-size: 28px;">⏰ Interview Reminder</h1>
        </div>
        
        <div style="padding: 40px 30px;">
            <p style="font-size: 18px;">Hi <strong>{}</strong>,</p>
            
            <p style="font-size: 16px;">This is a friendly reminder about your upcoming interview:</p>
            
            <div style="background-color: #F3F4F6; border-radius: 12px; padding: 25px; margin: 25px 0; text-align: center;">
                <p style="font-size: 24px; font-weight: 700; color: #111827; margin: 0 0 10px 0;">{}</p>
                <p style="font-size: 18px; color: #6B7280; margin: 0;">for <strong>{}</strong></p>
                <p style="font-size: 16px; color: #9CA3AF; margin: 10px 0 0 0;">at {}</p>
            </div>
            
            {}
            
            <p style="margin-top: 30px;">See you soon!</p>
            
            <p style="margin-top: 20px;">
                Best regards,<br>
                <strong>{} Team</strong>
            </p>
        </div>
    </div>
</body>
</html>"#,
            candidate_name,
            scheduled_date,
            job_title,
            company_name,
            meet_link_html,
            company_name
        ),
    }
}

/// Generate interview cancellation email template
pub fn get_interview_cancellation_template(
    candidate_name: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
) -> InterviewEmailTemplate {
    InterviewEmailTemplate {
        subject: format!("Interview Cancelled - {} at {}", job_title, company_name),
        body: format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; margin: 0; padding: 0; background-color: #F9FAFB;">
    <div style="max-width: 600px; margin: 0 auto; background-color: #FFFFFF;">
        <div style="background-color: #EF4444; color: white; padding: 40px 30px; text-align: center;">
            <h1 style="margin: 0; font-size: 28px;">Interview Cancelled</h1>
        </div>
        
        <div style="padding: 40px 30px;">
            <p style="font-size: 18px;">Dear <strong>{}</strong>,</p>
            
            <p>We regret to inform you that the interview scheduled for <strong>{}</strong> on <strong>{}</strong> has been cancelled.</p>
            
            <div style="background-color: #FEF2F2; border-left: 4px solid #EF4444; padding: 20px; margin: 25px 0;">
                <p style="margin: 0; color: #991B1B;">
                    We apologize for any inconvenience this may cause.
                </p>
            </div>
            
            <p>We will reach out to you shortly to reschedule at a more convenient time.</p>
            
            <p style="margin-top: 30px;">Thank you for your understanding.</p>
            
            <p style="margin-top: 20px;">
                Best regards,<br>
                <strong>{} Hiring Team</strong>
            </p>
        </div>
    </div>
</body>
</html>"#,
            candidate_name,
            job_title,
            scheduled_date,
            company_name
        ),
    }
}


/// Generate interview updated email template for candidate
pub fn get_interview_updated_template(
    candidate_name: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
    duration_minutes: i32,
    interview_type: &str,
    google_meet_link: Option<&str>,
    panel_members: &str,
) -> InterviewEmailTemplate {
    let meet_link_html = if let Some(link) = google_meet_link {
        format!(
            r#"
            <div style="background-color: #EDE9FE; border-left: 4px solid #7C3AED; padding: 15px; margin: 20px 0;">
                <p style="margin: 0 0 10px 0; font-weight: 600; color: #5B21B6;">📹 Google Meet Link:</p>
                <a href="{}" target="_blank" rel="noopener noreferrer" 
                   style="color: #7C3AED; text-decoration: none; font-weight: 500; word-break: break-all;">
                    {}
                </a>
            </div>
            "#,
            link, link
        )
    } else {
        String::new()
    };

    InterviewEmailTemplate {
        subject: format!("Interview Time Updated - {} at {}", job_title, company_name),
        body: format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
            </head>
            <body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); padding: 30px; border-radius: 10px 10px 0 0; text-align: center;">
                    <h1 style="color: white; margin: 0; font-size: 28px;">Interview Rescheduled</h1>
                </div>
                
                <div style="background-color: #ffffff; padding: 30px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 10px 10px;">
                    <p style="font-size: 16px; margin-bottom: 20px;">Dear {},</p>
                    
                    <p style="font-size: 16px; margin-bottom: 20px;">
                        Your interview for the <strong>{}</strong> position at <strong>{}</strong> has been <strong style="color: #F59E0B;">rescheduled</strong>.
                    </p>

                    <div style="background-color: #FEF3C7; border-left: 4px solid #F59E0B; padding: 15px; margin: 20px 0;">
                        <p style="margin: 0; color: #92400E;">
                            <strong>⚠️ Important:</strong> Please note the updated time below and confirm your availability.
                        </p>
                    </div>
                    
                    <div style="background-color: #F9FAFB; padding: 20px; border-radius: 8px; margin: 20px 0;">
                        <h2 style="color: #374151; margin-top: 0; font-size: 18px; border-bottom: 2px solid #E5E7EB; padding-bottom: 10px;">
                            📅 Updated Interview Details
                        </h2>
                        <table style="width: 100%; border-collapse: collapse;">
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Date & Time:</td>
                                <td style="padding: 10px 0; color: #111827; font-weight: 500;">{}</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Duration:</td>
                                <td style="padding: 10px 0; color: #111827;">{} minutes</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Interview Type:</td>
                                <td style="padding: 10px 0; color: #111827;">{}</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Panel Members:</td>
                                <td style="padding: 10px 0; color: #111827;">{}</td>
                            </tr>
                        </table>
                    </div>
                    
                    {}
                    
                    <div style="background-color: #DBEAFE; border-left: 4px solid #3B82F6; padding: 15px; margin: 20px 0;">
                        <p style="margin: 0; color: #1E40AF;">
                            <strong>💡 Tip:</strong> Please add this to your calendar and be ready 5 minutes before the scheduled time.
                        </p>
                    </div>
                    
                    <p style="font-size: 16px; margin-top: 30px;">
                        If you have any questions or need to reschedule, please don't hesitate to reach out.
                    </p>
                    
                    <p style="font-size: 16px; margin-top: 20px;">
                        Best regards,<br>
                        <strong>{} Hiring Team</strong>
                    </p>
                </div>
                
                <div style="text-align: center; padding: 20px; color: #6B7280; font-size: 14px;">
                    <p style="margin: 5px 0;">This is an automated message from {} Recruitment System</p>
                </div>
            </body>
            </html>
            "#,
            candidate_name,
            job_title,
            company_name,
            scheduled_date,
            duration_minutes,
            interview_type,
            panel_members,
            meet_link_html,
            company_name,
            company_name,
        ),
    }
}

/// Generate interview scheduled email template for panelist
pub fn get_panelist_interview_scheduled_template(
    panelist_name: &str,
    candidate_name: &str,
    candidate_email: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
    duration_minutes: i32,
    interview_type: &str,
    google_meet_link: Option<&str>,
) -> InterviewEmailTemplate {
    let meet_link_html = if let Some(link) = google_meet_link {
        format!(
            r#"
            <div style="background-color: #EDE9FE; border-left: 4px solid #7C3AED; padding: 15px; margin: 20px 0;">
                <p style="margin: 0 0 10px 0; font-weight: 600; color: #5B21B6;">📹 Google Meet Link:</p>
                <a href="{}" target="_blank" rel="noopener noreferrer" 
                   style="color: #7C3AED; text-decoration: none; font-weight: 500; word-break: break-all;">
                    {}
                </a>
            </div>
            "#,
            link, link
        )
    } else {
        String::from(
            r#"
            <div style="background-color: #FEF3C7; border-left: 4px solid #F59E0B; padding: 15px; margin: 20px 0;">
                <p style="margin: 0; color: #92400E;">
                    <strong>Note:</strong> Meeting link will be shared separately.
                </p>
            </div>
            "#
        )
    };

    InterviewEmailTemplate {
        subject: format!("Interview Panel Assignment - {} at {}", job_title, company_name),
        body: format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            line-height: 1.6;
            color: #1F2937;
            margin: 0;
            padding: 0;
            background-color: #F9FAFB;
        }}
        .container {{
            max-width: 600px;
            margin: 0 auto;
            background-color: #FFFFFF;
        }}
        .header {{
            background: linear-gradient(135deg, #10B981 0%, #059669 100%);
            color: white;
            padding: 40px 30px;
            text-align: center;
        }}
        .header h1 {{
            margin: 0;
            font-size: 28px;
            font-weight: 700;
        }}
        .content {{
            padding: 40px 30px;
        }}
        .greeting {{
            font-size: 18px;
            color: #111827;
            margin-bottom: 20px;
        }}
        .details-box {{
            background-color: #F3F4F6;
            border-radius: 12px;
            padding: 25px;
            margin: 25px 0;
        }}
        .detail-row {{
            display: flex;
            padding: 12px 0;
            border-bottom: 1px solid #E5E7EB;
        }}
        .detail-row:last-child {{
            border-bottom: none;
        }}
        .detail-label {{
            font-weight: 600;
            color: #374151;
            min-width: 140px;
        }}
        .detail-value {{
            color: #6B7280;
            flex: 1;
        }}
        .footer {{
            background-color: #F9FAFB;
            padding: 30px;
            text-align: center;
            color: #6B7280;
            font-size: 14px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>👥 Interview Panel Assignment</h1>
        </div>
        
        <div class="content">
            <p class="greeting">Dear <strong>{}</strong>,</p>
            
            <p>You have been assigned as a panel member for an upcoming interview. Please review the details below and mark your calendar.</p>
            
            <div class="details-box">
                <div class="detail-row">
                    <span class="detail-label">👤 Candidate:</span>
                    <span class="detail-value"><strong>{}</strong> ({})</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">💼 Position:</span>
                    <span class="detail-value">{}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">🏢 Company:</span>
                    <span class="detail-value">{}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">📅 Date & Time:</span>
                    <span class="detail-value">{}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">⏱️ Duration:</span>
                    <span class="detail-value">{} minutes</span>
                </div>
                <div class="detail-row">
                    <span class="detail-label">🎯 Interview Type:</span>
                    <span class="detail-value">{}</span>
                </div>
            </div>
            
            {}
            
            <div style="background-color: #DBEAFE; border-left: 4px solid #3B82F6; padding: 15px; margin: 20px 0;">
                <p style="margin: 0; color: #1E40AF;">
                    <strong>📋 Action Items:</strong>
                </p>
                <ul style="margin: 10px 0 0 0; padding-left: 20px; color: #1E40AF;">
                    <li>Review the candidate's profile and resume</li>
                    <li>Add this interview to your calendar</li>
                    <li>Join the meeting 5 minutes early</li>
                    <li>Prepare relevant questions for the candidate</li>
                </ul>
            </div>
            
            <p style="margin-top: 30px;">If you have any conflicts with this schedule or questions about the interview, please contact the hiring team immediately.</p>
            
            <p style="margin-top: 20px;">
                Best regards,<br>
                <strong>{} Hiring Team</strong>
            </p>
        </div>
        
        <div class="footer">
            <p>This is an automated message from {} recruitment system.</p>
            <p style="margin-top: 10px;">A calendar invitation has been sent to your email.</p>
        </div>
    </div>
</body>
</html>"#,
            panelist_name,
            candidate_name,
            candidate_email,
            job_title,
            company_name,
            scheduled_date,
            duration_minutes,
            interview_type,
            meet_link_html,
            company_name,
            company_name
        ),
    }
}

/// Generate interview updated email template for panelist
pub fn get_panelist_interview_updated_template(
    panelist_name: &str,
    candidate_name: &str,
    candidate_email: &str,
    job_title: &str,
    company_name: &str,
    scheduled_date: &str,
    duration_minutes: i32,
    google_meet_link: Option<&str>,
) -> InterviewEmailTemplate {
    let meet_link_html = if let Some(link) = google_meet_link {
        format!(
            r#"
            <div style="background-color: #EDE9FE; border-left: 4px solid #7C3AED; padding: 15px; margin: 20px 0;">
                <p style="margin: 0 0 10px 0; font-weight: 600; color: #5B21B6;">📹 Google Meet Link:</p>
                <a href="{}" target="_blank" rel="noopener noreferrer" 
                   style="color: #7C3AED; text-decoration: none; font-weight: 500; word-break: break-all;">
                    {}
                </a>
            </div>
            "#,
            link, link
        )
    } else {
        String::new()
    };

    InterviewEmailTemplate {
        subject: format!("Interview Schedule Updated - {} at {}", job_title, company_name),
        body: format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
            </head>
            <body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); padding: 30px; border-radius: 10px 10px 0 0; text-align: center;">
                    <h1 style="color: white; margin: 0; font-size: 28px;">Interview Rescheduled</h1>
                </div>
                
                <div style="background-color: #ffffff; padding: 30px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 10px 10px;">
                    <p style="font-size: 16px; margin-bottom: 20px;">Dear {},</p>
                    
                    <p style="font-size: 16px; margin-bottom: 20px;">
                        The interview for <strong>{}</strong> applying for the <strong>{}</strong> position has been <strong style="color: #F59E0B;">rescheduled</strong>.
                    </p>

                    <div style="background-color: #FEF3C7; border-left: 4px solid #F59E0B; padding: 15px; margin: 20px 0;">
                        <p style="margin: 0; color: #92400E;">
                            <strong>⚠️ Action Required:</strong> Please update your calendar with the new time below.
                        </p>
                    </div>
                    
                    <div style="background-color: #F9FAFB; padding: 20px; border-radius: 8px; margin: 20px 0;">
                        <h2 style="color: #374151; margin-top: 0; font-size: 18px; border-bottom: 2px solid #E5E7EB; padding-bottom: 10px;">
                            📅 Updated Interview Details
                        </h2>
                        <table style="width: 100%; border-collapse: collapse;">
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Candidate:</td>
                                <td style="padding: 10px 0; color: #111827; font-weight: 500;">{} ({})</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Position:</td>
                                <td style="padding: 10px 0; color: #111827;">{}</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Date & Time:</td>
                                <td style="padding: 10px 0; color: #111827; font-weight: 500;">{}</td>
                            </tr>
                            <tr>
                                <td style="padding: 10px 0; color: #6B7280; font-weight: 600;">Duration:</td>
                                <td style="padding: 10px 0; color: #111827;">{} minutes</td>
                            </tr>
                        </table>
                    </div>
                    
                    {}
                    
                    <div style="background-color: #DBEAFE; border-left: 4px solid #3B82F6; padding: 15px; margin: 20px 0;">
                        <p style="margin: 0; color: #1E40AF;">
                            <strong>💡 Reminder:</strong> Please review the candidate's profile before the interview.
                        </p>
                    </div>
                    
                    <p style="font-size: 16px; margin-top: 30px;">
                        If you have any conflicts with this time, please contact the hiring team immediately.
                    </p>
                    
                    <p style="font-size: 16px; margin-top: 20px;">
                        Best regards,<br>
                        <strong>{} Hiring Team</strong>
                    </p>
                </div>
                
                <div style="text-align: center; padding: 20px; color: #6B7280; font-size: 14px;">
                    <p style="margin: 5px 0;">This is an automated message from {} Recruitment System</p>
                </div>
            </body>
            </html>
            "#,
            panelist_name,
            candidate_name,
            job_title,
            candidate_name,
            candidate_email,
            job_title,
            scheduled_date,
            duration_minutes,
            meet_link_html,
            company_name,
            company_name,
        ),
    }
}
