// src/services/interviews.rs

use crate::common::{ApiError, Validator};
use crate::candidates::models::{
    Application, CreateInterviewRequest, GoogleMeetLinkResponse, Interview,
    InterviewPanelMember, InterviewWithDetails, UpdateInterviewRequest,
};
use crate::candidates::handlers::interview_email_templates::get_interview_scheduled_template;
use crate::services::google::{CalendarEvent, GoogleService};
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::common::generate_interview_id;

/// Schedule an interview with validation
pub async fn schedule_interview(
    pool: &SqlitePool,
    google_service: Arc<GoogleService>,
    request: CreateInterviewRequest,
    created_by: &str,
) -> Result<Interview, ApiError> {
    debug!(
        application_id = %request.application_id,
        scheduled_date = %request.scheduled_date,
        duration_minutes = request.duration_minutes,
        "Scheduling interview"
    );

    // Validate request
    let validator = crate::candidates::validators::InterviewValidator;
    let validation = validator.validate(&request);
    if !validation.is_valid {
        let error_messages: Vec<String> = validation
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect();
        warn!(
            errors = ?error_messages,
            "Interview request validation failed"
        );
        return Err(ApiError::BadRequest(error_messages.join(", ")));
    }

    // Fetch application to get candidate_id and job_id
    let application: (String, String) = sqlx::query_as(
        "SELECT user_id, job_id FROM applications WHERE id = ?"
    )
        .bind(&request.application_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error checking application existence");
            ApiError::DatabaseError(e)
        })?
        .ok_or_else(|| {
            warn!(
                application_id = %request.application_id,
                "Application not found for interview scheduling"
            );
            ApiError::BadRequest("Application not found".to_string())
        })?;

    let (candidate_id, job_id) = application;

    // Parse scheduled date (keep original timezone, don't convert to UTC)
    let scheduled_datetime = DateTime::parse_from_rfc3339(&request.scheduled_date)
        .map_err(|e| {
            warn!(
                error = %e,
                scheduled_date = %request.scheduled_date,
                "Invalid scheduled date format"
            );
            ApiError::BadRequest(format!("Invalid date format: {}", e))
        })?;

    // Validate that scheduled date is in the future (compare in UTC)
    if scheduled_datetime.with_timezone(&Utc) <= Utc::now() {
        return Err(ApiError::BadRequest(
            "Scheduled date must be in the future".to_string(),
        ));
    }
    
    // Store the date as-is with original timezone
    let scheduled_date_str = request.scheduled_date.clone();
    
    info!(
        scheduled_date_input = %request.scheduled_date,
        scheduled_date_parsed = %scheduled_datetime,
        "Storing interview with timezone"
    );

    // Serialize panel members to JSON
    let panel_members_json = serde_json::to_string(&request.panel_members).map_err(|e| {
        error!(error = %e, "Failed to serialize panel members");
        ApiError::InternalServer(format!("Failed to serialize panel members: {}", e))
    })?;

    // Log the create_google_meet flag for debugging
    info!(
        create_google_meet = request.create_google_meet,
        panel_members_count = request.panel_members.len(),
        "Processing interview creation request"
    );

    // Create Google Meet link if requested
    let (google_meet_link, google_calendar_event_id) = if request.create_google_meet {
        info!("Creating Google Meet link for interview");
        match create_google_meet_link_internal(
            google_service,
            scheduled_datetime.with_timezone(&Utc),
            request.duration_minutes,
            &request.panel_members,
            &request.interview_type,
        )
        .await
        {
            Ok((meet_link, calendar_id)) => {
                info!(meet_link = %meet_link, "Google Meet link created successfully");
                (Some(meet_link), Some(calendar_id))
            }
            Err(e) => {
                warn!(error = %e, "Failed to create Google Meet link, continuing without it");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // Create interview record
    let interview_id = generate_interview_id();
    sqlx::query(
        r#"
        INSERT INTO interviews (
            id, application_id, candidate_id, job_id, scheduled_date, duration_minutes, interview_type,
            google_meet_link, google_calendar_event_id, panel_members, notes, created_by
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&interview_id)
    .bind(&request.application_id)
    .bind(&candidate_id)
    .bind(&job_id)
    .bind(&request.scheduled_date)
    .bind(request.duration_minutes)
    .bind(&request.interview_type)
    .bind(&google_meet_link)
    .bind(&google_calendar_event_id)
    .bind(&panel_members_json)
    .bind(&request.notes)
    .bind(created_by)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error creating interview");
        ApiError::DatabaseError(e)
    })?;

    // Fetch created interview
    let interview = sqlx::query_as::<_, Interview>("SELECT * FROM interviews WHERE id = ?")
        .bind(&interview_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching created interview");
            ApiError::DatabaseError(e)
        })?;

    info!(
        interview_id = %interview_id,
        application_id = %request.application_id,
        scheduled_date_stored = %interview.scheduled_date,
        has_meet_link = google_meet_link.is_some(),
        "Interview scheduled successfully"
    );

    // Save panelists to database for future suggestions
    for panel_member in &request.panel_members {
        if let Err(e) = crate::services::panelists::upsert_panelist(pool, panel_member).await {
            warn!(error = %e, email = %panel_member.email, "Failed to save panelist");
        }
        if let Err(e) = crate::services::panelists::increment_panelist_usage(pool, &panel_member.email).await {
            warn!(error = %e, email = %panel_member.email, "Failed to increment panelist usage");
        }
    }

    Ok(interview)
}

/// Create Google Meet link with Google Calendar API integration
pub async fn create_google_meet_link(
    google_service: Arc<GoogleService>,
    request: crate::candidates::models::CreateGoogleMeetRequest,
) -> Result<GoogleMeetLinkResponse, ApiError> {
    let scheduled_date = request.start_time.clone();
    let summary = request.summary.clone();
    let attendees = request.attendees.clone();
    
    // Calculate duration from start and end time
    let start_time = DateTime::parse_from_rfc3339(&request.start_time)
        .map_err(|e| ApiError::BadRequest(format!("Invalid start_time format: {}", e)))?
        .with_timezone(&Utc);
    let end_time = DateTime::parse_from_rfc3339(&request.end_time)
        .map_err(|e| ApiError::BadRequest(format!("Invalid end_time format: {}", e)))?
        .with_timezone(&Utc);
    let duration_minutes = (end_time - start_time).num_minutes() as i32;
    debug!(
        scheduled_date = %scheduled_date,
        duration_minutes = duration_minutes,
        attendee_count = attendees.len(),
        "Creating Google Meet link"
    );

    // Parse scheduled date
    let start_time = DateTime::parse_from_rfc3339(&scheduled_date)
        .map_err(|e| {
            warn!(
                error = %e,
                scheduled_date = %scheduled_date,
                "Invalid scheduled date format"
            );
            ApiError::BadRequest(format!("Invalid date format: {}", e))
        })?
        .with_timezone(&Utc);

    // Calculate end time
    let end_time = start_time + Duration::minutes(duration_minutes as i64);

    // Create calendar event
    let event = CalendarEvent {
        summary,
        description: None,
        start: start_time,
        end: end_time,
        attendees,
        create_meet_link: true,
    };

    let calendar_response = google_service
        .create_calendar_event(event)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create Google Calendar event");
            ApiError::InternalServer(format!("Failed to create Google Meet link: {}", e))
        })?;

    let meet_link = calendar_response.hangout_link.ok_or_else(|| {
        error!("Google Calendar event created but no Meet link was generated");
        ApiError::InternalServer("Failed to generate Google Meet link".to_string())
    })?;

    info!(
        event_id = %calendar_response.id,
        meet_link = %meet_link,
        "Google Meet link created successfully"
    );

    Ok(GoogleMeetLinkResponse {
        meet_link,
        calendar_event_id: calendar_response.id,
        calendar_event_url: calendar_response.html_link,
    })
}

/// Internal helper to create Google Meet link
async fn create_google_meet_link_internal(
    google_service: Arc<GoogleService>,
    start_time: DateTime<Utc>,
    duration_minutes: i32,
    panel_members: &[InterviewPanelMember],
    interview_type: &str,
) -> Result<(String, String), ApiError> {
    let end_time = start_time + Duration::minutes(duration_minutes as i64);

    let attendees: Vec<String> = panel_members.iter().map(|m| m.email.clone()).collect();

    let event = CalendarEvent {
        summary: format!("Interview - {}", interview_type),
        description: Some(format!(
            "Interview scheduled for {} minutes",
            duration_minutes
        )),
        start: start_time,
        end: end_time,
        attendees,
        create_meet_link: true,
    };

    let calendar_response = google_service
        .create_calendar_event(event)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create Google Calendar event");
            ApiError::InternalServer(format!("Failed to create Google Meet link: {}", e))
        })?;

    let meet_link = calendar_response.hangout_link.ok_or_else(|| {
        error!("Google Calendar event created but no Meet link was generated");
        ApiError::InternalServer("Failed to generate Google Meet link".to_string())
    })?;

    Ok((meet_link, calendar_response.id))
}

/// Send calendar invitations to attendees
pub async fn send_calendar_invitations(
    pool: &SqlitePool,
    aws_service: &crate::services::aws::AWSService,
    interview_id: &str,
) -> Result<(), ApiError> {
    debug!(
        interview_id = %interview_id,
        "Sending calendar invitations"
    );

    // Get interview details
    let interview = get_interview(pool, interview_id).await?;

    // Parse panel members
    let panel_members: Vec<InterviewPanelMember> = serde_json::from_str(&interview.panel_members)
        .map_err(|e| {
        error!(error = %e, "Failed to parse panel members");
        ApiError::InternalServer(format!("Failed to parse panel members: {}", e))
    })?;

    // Get application details
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE id = ?",
    )
    .bind(&interview.application_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching application");
        ApiError::DatabaseError(e)
    })?;

    // Get candidate email
    let candidate = sqlx::query_as::<_, crate::auth::User>("SELECT * FROM users WHERE id = ?")
        .bind(&application.user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching candidate");
            ApiError::DatabaseError(e)
        })?;

    // Get job title
    let job = sqlx::query_as::<_, crate::jobs::Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(&application.job_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching job");
            ApiError::DatabaseError(e)
        })?;

    // Build email content using template
    let candidate_name = candidate.name.unwrap_or_else(|| "Candidate".to_string());
    let company_name = job.company.unwrap_or_else(|| "Our Company".to_string());
    
    let email_template = get_interview_scheduled_template(
        &candidate_name,
        &job.title,
        &company_name,
        &interview.scheduled_date,
        interview.duration_minutes,
        &interview.interview_type,
        interview.google_meet_link.as_deref(),
        interview.notes.as_deref(),
    );

    let subject = email_template.subject;
    let content = email_template.body;

    // Send email to candidate
    let mut recipients = vec![candidate.email.clone()];

    // Add panel members to recipients
    for member in &panel_members {
        recipients.push(member.email.clone());
    }

    let recipient_count = recipients.len();

    aws_service
        .send_email(recipients, &subject, &content, None)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to send calendar invitation emails");
            ApiError::InternalServer(format!("Failed to send calendar invitations: {}", e))
        })?;

    info!(
        interview_id = %interview_id,
        recipient_count = recipient_count,
        "Calendar invitations sent successfully"
    );

    Ok(())
}

/// Get interviews for a specific candidate
pub async fn get_candidate_interviews(
    pool: &SqlitePool,
    candidate_id: &str,
) -> Result<Vec<InterviewWithDetails>, ApiError> {
    debug!(
        candidate_id = %candidate_id,
        "Fetching interviews for candidate"
    );

    // Get all applications for this candidate
    let application_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM applications WHERE user_id = ?")
            .bind(candidate_id)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Database error fetching candidate applications");
                ApiError::DatabaseError(e)
            })?;

    if application_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build query with IN clause
    let placeholders = application_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT * FROM interviews WHERE application_id IN ({}) ORDER BY scheduled_date DESC",
        placeholders
    );

    let mut query_builder = sqlx::query_as::<_, Interview>(&query);
    for app_id in &application_ids {
        query_builder = query_builder.bind(app_id);
    }

    let interviews = query_builder.fetch_all(pool).await.map_err(|e| {
        error!(error = %e, "Database error fetching candidate interviews");
        ApiError::DatabaseError(e)
    })?;

    // Build detailed response
    let mut result = Vec::new();
    for interview in interviews {
        let details = build_interview_details(pool, interview).await?;
        result.push(details);
    }

    info!(
        candidate_id = %candidate_id,
        interview_count = result.len(),
        "Successfully fetched candidate interviews"
    );

    Ok(result)
}

/// Get interviews for a specific job
pub async fn get_job_interviews(
    pool: &SqlitePool,
    job_id: &str,
) -> Result<Vec<InterviewWithDetails>, ApiError> {
    debug!(
        job_id = %job_id,
        "Fetching interviews for job"
    );

    // Get all applications for this job
    let application_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM applications WHERE job_id = ?")
            .bind(job_id)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Database error fetching job applications");
                ApiError::DatabaseError(e)
            })?;

    if application_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build query with IN clause
    let placeholders = application_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT * FROM interviews WHERE application_id IN ({}) ORDER BY scheduled_date DESC",
        placeholders
    );

    let mut query_builder = sqlx::query_as::<_, Interview>(&query);
    for app_id in &application_ids {
        query_builder = query_builder.bind(app_id);
    }

    let interviews = query_builder.fetch_all(pool).await.map_err(|e| {
        error!(error = %e, "Database error fetching job interviews");
        ApiError::DatabaseError(e)
    })?;

    // Build detailed response
    let mut result = Vec::new();
    for interview in interviews {
        let details = build_interview_details(pool, interview).await?;
        result.push(details);
    }

    info!(
        job_id = %job_id,
        interview_count = result.len(),
        "Successfully fetched job interviews"
    );

    Ok(result)
}

/// Get interview by ID
pub async fn get_interview(pool: &SqlitePool, interview_id: &str) -> Result<Interview, ApiError> {
    debug!(
        interview_id = %interview_id,
        "Fetching interview"
    );

    let interview = sqlx::query_as::<_, Interview>("SELECT * FROM interviews WHERE id = ?")
        .bind(interview_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching interview");
            ApiError::DatabaseError(e)
        })?
        .ok_or_else(|| {
            warn!(interview_id = %interview_id, "Interview not found");
            ApiError::BadRequest("Interview not found".to_string())
        })?;

    info!(
        interview_id = %interview_id,
        "Successfully fetched interview"
    );

    Ok(interview)
}

/// Update interview
pub async fn update_interview(
    pool: &SqlitePool,
    aws_service: Arc<crate::services::aws::AWSService>,
    interview_id: &str,
    request: UpdateInterviewRequest,
    _user_id: &str,
) -> Result<Interview, ApiError> {
    debug!(
        interview_id = %interview_id,
        "Updating interview"
    );

    // Verify interview exists
    let existing = get_interview(pool, interview_id).await?;

    // Build update query dynamically based on what fields are provided
    let mut query_parts = Vec::new();

    if request.scheduled_date.is_some() {
        query_parts.push("scheduled_date");
    }
    if request.duration_minutes.is_some() {
        query_parts.push("duration_minutes");
    }
    if request.interview_type.is_some() {
        query_parts.push("interview_type");
    }
    if request.panel_members.is_some() {
        query_parts.push("panel_members");
    }
    if request.notes.is_some() {
        query_parts.push("notes");
    }

    if query_parts.is_empty() {
        return Ok(existing);
    }

    // Validate scheduled_date if provided
    if let Some(ref scheduled_date) = request.scheduled_date {
        DateTime::parse_from_rfc3339(scheduled_date).map_err(|e| {
            warn!(
                error = %e,
                scheduled_date = %scheduled_date,
                "Invalid scheduled date format"
            );
            ApiError::BadRequest(format!("Invalid date format: {}", e))
        })?;
    }

    // Validate duration_minutes if provided
    if let Some(duration_minutes) = request.duration_minutes {
        if duration_minutes <= 0 || duration_minutes > 480 {
            return Err(ApiError::BadRequest(
                "Duration must be between 1 and 480 minutes".to_string(),
            ));
        }
    }

    // Serialize panel_members if provided
    let panel_members_json = if let Some(ref panel_members) = request.panel_members {
        Some(serde_json::to_string(panel_members).map_err(|e| {
            error!(error = %e, "Failed to serialize panel members");
            ApiError::InternalServer(format!("Failed to serialize panel members: {}", e))
        })?)
    } else {
        None
    };

    // Build and execute update query
    let set_clauses: Vec<String> = query_parts
        .iter()
        .map(|field| format!("{} = ?", field))
        .collect();
    let query = format!(
        "UPDATE interviews SET {}, updated_at = datetime('now') WHERE id = ?",
        set_clauses.join(", ")
    );

    let mut query_builder = sqlx::query(&query);

    if let Some(ref scheduled_date) = request.scheduled_date {
        query_builder = query_builder.bind(scheduled_date);
    }
    if let Some(duration_minutes) = request.duration_minutes {
        query_builder = query_builder.bind(duration_minutes);
    }
    if let Some(ref interview_type) = request.interview_type {
        query_builder = query_builder.bind(interview_type);
    }
    if let Some(ref panel_json) = panel_members_json {
        query_builder = query_builder.bind(panel_json);
    }
    if let Some(ref notes) = request.notes {
        query_builder = query_builder.bind(notes);
    }
    query_builder = query_builder.bind(interview_id);

    query_builder.execute(pool).await.map_err(|e| {
        error!(error = %e, "Database error updating interview");
        ApiError::DatabaseError(e)
    })?;

    // Fetch updated interview
    let updated = get_interview(pool, interview_id).await?;

    info!(
        interview_id = %interview_id,
        scheduled_date_after_update = %updated.scheduled_date,
        "Interview updated successfully"
    );

    // Send update emails to candidate and panelists
    if let Err(e) = send_interview_update_email_to_candidate(pool, &aws_service, interview_id).await {
        warn!(
            error = %e,
            interview_id = %interview_id,
            "Failed to send interview update email to candidate"
        );
    }

    if let Err(e) = send_interview_update_emails_to_panelists(pool, &aws_service, interview_id).await {
        warn!(
            error = %e,
            interview_id = %interview_id,
            "Failed to send interview update emails to panelists"
        );
    }

    Ok(updated)
}

/// Cancel interview with notification
pub async fn cancel_interview(
    pool: &SqlitePool,
    aws_service: &crate::services::aws::AWSService,
    interview_id: &str,
) -> Result<(), ApiError> {
    debug!(
        interview_id = %interview_id,
        "Canceling interview"
    );

    // Get interview details before deletion
    let interview = get_interview(pool, interview_id).await?;

    // Parse panel members
    let panel_members: Vec<InterviewPanelMember> = serde_json::from_str(&interview.panel_members)
        .map_err(|e| {
        error!(error = %e, "Failed to parse panel members");
        ApiError::InternalServer(format!("Failed to parse panel members: {}", e))
    })?;

    // Get application details
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE id = ?",
    )
    .bind(&interview.application_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching application");
        ApiError::DatabaseError(e)
    })?;

    // Get candidate email
    let candidate = sqlx::query_as::<_, crate::auth::User>("SELECT * FROM users WHERE id = ?")
        .bind(&application.user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching candidate");
            ApiError::DatabaseError(e)
        })?;

    // Get job title
    let job = sqlx::query_as::<_, crate::jobs::Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(&application.job_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching job");
            ApiError::DatabaseError(e)
        })?;

    // Delete interview
    sqlx::query("DELETE FROM interviews WHERE id = ?")
        .bind(interview_id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error deleting interview");
            ApiError::DatabaseError(e)
        })?;

    // Send cancellation notification
    let subject = format!("Interview Cancelled - {}", job.title);
    let content = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: Arial, sans-serif; line-height: 1.6; color: #333; }}
        .container {{ max-width: 600px; margin: 0 auto; padding: 20px; }}
        .header {{ background-color: #EF4444; color: white; padding: 20px; text-align: center; }}
        .content {{ padding: 20px; background-color: #f9f9f9; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Interview Cancelled</h1>
        </div>
        <div class="content">
            <p>Dear {},</p>
            
            <p>We regret to inform you that the interview scheduled for <strong>{}</strong> on <strong>{}</strong> has been cancelled.</p>
            
            <p>We apologize for any inconvenience this may cause. We will reach out to you shortly to reschedule.</p>
            
            <p>Best regards,<br>
            Hiring Team</p>
        </div>
    </div>
</body>
</html>"#,
        candidate.name.unwrap_or_else(|| "Candidate".to_string()),
        job.title,
        interview.scheduled_date
    );

    let mut recipients = vec![candidate.email.clone()];
    for member in &panel_members {
        recipients.push(member.email.clone());
    }

    // Send cancellation email (don't fail if email fails)
    if let Err(e) = aws_service
        .send_email(recipients, &subject, &content, None)
        .await
    {
        warn!(
            error = %e,
            interview_id = %interview_id,
            "Failed to send cancellation notification, but interview was deleted"
        );
    }

    info!(
        interview_id = %interview_id,
        "Interview cancelled successfully"
    );

    Ok(())
}

/// Helper function to build interview details with related data
async fn build_interview_details(
    pool: &SqlitePool,
    interview: Interview,
) -> Result<InterviewWithDetails, ApiError> {
    // Get application
    let application = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE id = ?",
    )
    .bind(&interview.application_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching application");
        ApiError::DatabaseError(e)
    })?;

    // Get candidate info
    let candidate = sqlx::query_as::<_, crate::auth::User>("SELECT * FROM users WHERE id = ?")
        .bind(&application.user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching candidate");
            ApiError::DatabaseError(e)
        })?;

    // Get job info
    let job = sqlx::query_as::<_, crate::jobs::Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(&application.job_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "Database error fetching job");
            ApiError::DatabaseError(e)
        })?;

    // Parse panel members
    let panel_members_parsed: Vec<InterviewPanelMember> =
        serde_json::from_str(&interview.panel_members).map_err(|e| {
            error!(error = %e, "Failed to parse panel members");
            ApiError::InternalServer(format!("Failed to parse panel members: {}", e))
        })?;

    Ok(InterviewWithDetails {
        interview,
        candidate_name: candidate.name.unwrap_or_else(|| "Unknown".to_string()),
        candidate_email: candidate.email,
        job_title: job.title,
        panel_members_parsed,
    })
}


/// Send interview update email to candidate
async fn send_interview_update_email_to_candidate(
    pool: &SqlitePool,
    aws_service: &Arc<crate::services::AWSService>,
    interview_id: &str,
) -> Result<(), ApiError> {
    use crate::candidates::handlers::interview_email_templates::get_interview_updated_template;

    debug!(interview_id = %interview_id, "Sending interview update email to candidate");

    // Fetch interview with details
    let interview = get_interview(pool, interview_id).await?;

    // Fetch application to get candidate_id and job_id
    let application: (String, String) = sqlx::query_as(
        "SELECT user_id, job_id FROM applications WHERE id = ?"
    )
    .bind(&interview.application_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching application");
        ApiError::DatabaseError(e)
    })?;

    let (candidate_id, job_id) = application;

    // Fetch candidate
    let candidate: (String, String) = sqlx::query_as(
        "SELECT name, email FROM users WHERE id = ?"
    )
    .bind(&candidate_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching candidate");
        ApiError::DatabaseError(e)
    })?;

    // Fetch job
    let job: (String,) = sqlx::query_as(
        "SELECT title FROM jobs WHERE id = ?"
    )
    .bind(&job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching job");
        ApiError::DatabaseError(e)
    })?;

    // Parse panel members for display
    let panel_members_display = if let Ok(members) = serde_json::from_str::<Vec<crate::candidates::models::InterviewPanelMember>>(&interview.panel_members) {
        members
            .iter()
            .map(|m| {
                if let Some(name) = &m.name {
                    if let Some(role) = &m.role {
                        format!("{} ({})", name, role)
                    } else {
                        name.clone()
                    }
                } else {
                    m.email.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "Panel members".to_string()
    };

    // Generate email template
    let template = get_interview_updated_template(
        &candidate.0,
        &job.0,
        "Company", // TODO: Get from settings
        &interview.scheduled_date,
        interview.duration_minutes,
        &interview.interview_type,
        interview.google_meet_link.as_deref(),
        &panel_members_display,
    );

    // Send email via AWS SES
    aws_service
        .send_email(vec![candidate.1.clone()], &template.subject, &template.body, None)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to send interview update email to candidate");
            ApiError::InternalServer(format!("Failed to send email: {}", e))
        })?;

    info!(
        interview_id = %interview_id,
        candidate_email = %candidate.1,
        "Interview update email sent to candidate"
    );

    Ok(())
}

/// Send interview update emails to all panelists
async fn send_interview_update_emails_to_panelists(
    pool: &SqlitePool,
    aws_service: &Arc<crate::services::AWSService>,
    interview_id: &str,
) -> Result<(), ApiError> {
    use crate::candidates::handlers::interview_email_templates::get_panelist_interview_updated_template;

    debug!(interview_id = %interview_id, "Sending interview update emails to panelists");

    // Fetch interview with details
    let interview = get_interview(pool, interview_id).await?;

    // Fetch application to get candidate_id and job_id
    let application: (String, String) = sqlx::query_as(
        "SELECT user_id, job_id FROM applications WHERE id = ?"
    )
    .bind(&interview.application_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching application");
        ApiError::DatabaseError(e)
    })?;

    let (candidate_id, job_id) = application;

    // Fetch candidate
    let candidate: (String, String) = sqlx::query_as(
        "SELECT name, email FROM users WHERE id = ?"
    )
    .bind(&candidate_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching candidate");
        ApiError::DatabaseError(e)
    })?;

    // Fetch job
    let job: (String,) = sqlx::query_as(
        "SELECT title FROM jobs WHERE id = ?"
    )
    .bind(&job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "Database error fetching job");
        ApiError::DatabaseError(e)
    })?;

    // Parse panel members
    let panel_members: Vec<crate::candidates::models::InterviewPanelMember> = 
        serde_json::from_str(&interview.panel_members).map_err(|e| {
            error!(error = %e, "Failed to parse panel members");
            ApiError::InternalServer("Failed to parse panel members".to_string())
        })?;

    // Send email to each panelist
    for panel_member in panel_members {
        let panelist_name = panel_member.name.as_deref().unwrap_or(&panel_member.email);
        
        let template = get_panelist_interview_updated_template(
            panelist_name,
            &candidate.0,
            &candidate.1,
            &job.0,
            "Company", // TODO: Get from settings
            &interview.scheduled_date,
            interview.duration_minutes,
            interview.google_meet_link.as_deref(),
        );

        match aws_service
            .send_email(vec![panel_member.email.clone()], &template.subject, &template.body, None)
            .await
        {
            Ok(_) => {
                info!(
                    interview_id = %interview_id,
                    panelist_email = %panel_member.email,
                    "Interview update email sent to panelist"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    panelist_email = %panel_member.email,
                    "Failed to send interview update email to panelist"
                );
            }
        }
    }

    Ok(())
}
