// src/candidates/handlers/interviews.rs

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::json;
use tracing::info;

use crate::auth::AuthedUser;
use crate::common::{ApiError, AppState};
use crate::candidates::models::*;
use crate::services::interviews;

/// POST /api/admin/interviews/schedule - Schedule an interview
pub async fn schedule_interview(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(body): Json<CreateInterviewRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can schedule interviews".to_string(),
        ));
    }

    info!(
        admin_id = %authed.id,
        application_id = %body.application_id,
        "Admin scheduling interview"
    );

    let interview = interviews::schedule_interview(
        &state.db,
        state.google_service.clone(),
        body,
        &authed.id,
    )
    .await?;

    // Send calendar invitations to all attendees
    if let Err(e) = interviews::send_calendar_invitations(
        &state.db,
        &state.aws_service,
        &interview.id,
    )
    .await
    {
        // Log error but don't fail the request
        tracing::warn!(
            error = %e,
            interview_id = %interview.id,
            "Failed to send calendar invitations, but interview was created"
        );
    }

    Ok((StatusCode::CREATED, Json(interview)))
}

/// PUT /api/admin/interviews/:id - Update an interview
pub async fn update_interview(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateInterviewRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can update interviews".to_string(),
        ));
    }

    info!(
        admin_id = %authed.id,
        interview_id = %id,
        "Admin updating interview"
    );

    let interview = interviews::update_interview(
        &state.db,
        state.aws_service.clone(),
        &id,
        body,
        &authed.id
    ).await?;

    Ok(Json(interview))
}

/// DELETE /api/admin/interviews/:id - Cancel an interview
pub async fn cancel_interview(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can cancel interviews".to_string(),
        ));
    }

    info!(
        admin_id = %authed.id,
        interview_id = %id,
        "Admin canceling interview"
    );

    interviews::cancel_interview(&state.db, &state.aws_service, &id).await?;

    Ok(Json(json!({ "message": "Interview cancelled successfully" })))
}

/// GET /api/admin/interviews/:id - Get a single interview by ID
pub async fn get_interview(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can view interview details".to_string(),
        ));
    }

    let interview = interviews::get_interview(&state.db, &id).await?;

    Ok(Json(interview))
}

/// GET /api/interviews/candidate/:candidate_id - Get interviews for a candidate
pub async fn get_candidate_interviews(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(candidate_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    // Users can only view their own interviews, admins can view any
    if !authed.is_admin && authed.id != candidate_id {
        return Err(ApiError::Forbidden(
            "You can only view your own interviews".to_string(),
        ));
    }

    let interviews = interviews::get_candidate_interviews(&state.db, &candidate_id).await?;

    Ok(Json(interviews))
}

/// GET /api/admin/panelists - Get all panelists for dropdown suggestions
pub async fn get_panelists(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can view panelists".to_string(),
        ));
    }

    let panelists = crate::services::panelists::get_panelists(&state.db).await?;

    Ok(Json(panelists))
}

/// GET /api/interviews/job/:job_id - Get interviews for a job
pub async fn get_job_interviews(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(job_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can view job interviews".to_string(),
        ));
    }

    let interviews = interviews::get_job_interviews(&state.db, &job_id).await?;

    Ok(Json(interviews))
}

/// POST /api/admin/interviews/google-meet - Create Google Meet link
pub async fn create_google_meet_link(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(body): Json<CreateGoogleMeetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;
    if !authed.is_admin {
        return Err(ApiError::Forbidden(
            "Only admins can create Google Meet links".to_string(),
        ));
    }

    info!(
        admin_id = %authed.id,
        "Admin creating Google Meet link"
    );

    let response = interviews::create_google_meet_link(state.google_service.clone(), body).await?;

    Ok(Json(response))
}
