// src/candidates/models.rs

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// Resume Models
// ============================================================================

#[derive(FromRow, Serialize, Deserialize, Debug)]
pub struct Resume {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub status: String,
    pub score: Option<f64>,
    pub parsed_json: Option<String>,
    pub submitted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub label: Option<String>,
    // Candidate information (populated in admin queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub candidate_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub candidate_email: Option<String>,
}

#[derive(Clone, FromRow, Serialize, Deserialize, Debug)]
pub struct ResumeAsset {
    pub id: String,
    pub resume_id: String,
    pub kind: String,
    pub path: String,
    pub page: Option<i64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct UserResumeListItem {
    pub id: String,
    pub filename: String,
    pub label: Option<String>,
    pub status: String,
    pub file_size: Option<i64>,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResumeLabelRequest {
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct ResumeWithCandidate {
    #[serde(flatten)]
    pub resume: Resume,
    pub candidate_name: Option<String>,
    pub candidate_email: String,
    pub candidate_phone: Option<String>,
    pub candidate_location: Option<String>,
    pub processing_metadata: Option<ResumeProcessingMetadata>,
}

#[derive(Debug, Serialize)]
pub struct ResumeProcessingMetadata {
    pub processing_started_at: Option<String>,
    pub processing_completed_at: Option<String>,
    pub processing_duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub last_retry_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetailedResumeStatus {
    pub resume_id: String,
    pub status: String,
    pub progress_percentage: f64,
    pub current_step: String,
    pub error_messages: Vec<String>,
    pub estimated_completion_time: Option<String>,
    pub processing_metadata: ResumeProcessingMetadata,
}

#[derive(Debug, Deserialize)]
pub struct AdminResumeFilters {
    pub status: Option<String>,
    pub candidate_name: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub score_min: Option<f64>,
    pub score_max: Option<f64>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkResumeStatusUpdate {
    pub resume_ids: Vec<String>,
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RetryResumeProcessingRequest {
    pub force_reprocess: Option<bool>,
    pub priority: Option<String>,
}

// ============================================================================
// Application Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Application {
    pub id: String,
    pub user_id: String,
    pub job_id: String,
    pub resume_id: Option<String>,
    pub status: String,
    pub cover_letter: Option<String>,
    pub applied_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApplicationStatusHistory {
    pub id: String,
    pub application_id: String,
    pub status: String,
    pub changed_by: String,
    pub notes: Option<String>,
    pub changed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateApplicationRequest {
    pub job_id: String,
    pub resume_id: Option<String>,
    pub cover_letter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApplicationStatusRequest {
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplicationWithDetails {
    #[serde(flatten)]
    pub application: Application,
    pub job_title: Option<String>,
    pub candidate_name: Option<String>,
    pub candidate_email: Option<String>,
    pub status_history: Vec<ApplicationStatusHistory>,
}

#[derive(Debug, Serialize)]
pub struct EnhancedApplicationWithDetails {
    pub id: String,
    pub user_id: String,
    pub job_id: String,
    pub resume_id: Option<String>,
    pub video_id: Option<String>,
    pub status: String,
    pub cover_letter: Option<String>,
    pub applied_at: Option<String>,
    pub updated_at: Option<String>,
    pub job_title: String,
    pub job_company: Option<String>,
    pub job_location: Option<String>,
    pub job_salary_min: Option<i64>,
    pub job_salary_max: Option<i64>,
    pub job_image_url: Option<String>,
    pub company_logo_url: Option<String>,
    pub resume_filename: Option<String>,
    pub resume_label: Option<String>,
    pub status_history: Vec<ApplicationStatusHistory>,
}

#[derive(Debug, Serialize)]
pub struct JobApplicationDetails {
    pub application_id: String,
    pub candidate_id: String,
    pub candidate_name: String,
    pub candidate_email: String,
    pub resume_id: Option<String>,
    pub resume_filename: Option<String>,
    pub resume_label: Option<String>,
    pub status: String,
    pub applied_at: Option<String>,
    pub cover_letter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkApplicationStatusUpdate {
    pub application_ids: Vec<String>,
    pub status: String,
    pub notes: Option<String>,
}

// ============================================================================
// Video Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Video {
    pub id: String,
    pub user_id: String,
    pub s3_url: Option<String>,
    pub filename: Option<String>,
    pub file_size: Option<i64>,
    pub duration_seconds: i32,
    pub mime_type: Option<String>,
    pub uploaded_at: Option<String>,
    // YouTube fields
    pub video_source: Option<String>, // 'upload' or 'youtube'
    pub youtube_video_id: Option<String>,
    pub youtube_thumbnail_url: Option<String>,
    pub youtube_title: Option<String>,
    pub youtube_description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VideoMetadata {
    pub duration_seconds: i32,
    pub file_size: i64,
    pub format: String,
    pub resolution: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VideoUploadResponse {
    pub id: String,
    pub s3_url: String,
    pub filename: String,
    pub file_size: i64,
    pub duration_seconds: i32,
}

// Type alias for backward compatibility
pub type VideoSubmission = Video;

#[derive(Debug, Serialize, Deserialize)]
pub struct YouTubeVideoLinkRequest {
    pub youtube_video_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

// ============================================================================
// Interview Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Interview {
    pub id: String,
    pub application_id: String,
    pub scheduled_date: String,
    pub duration_minutes: i32,
    pub interview_type: String,
    pub google_meet_link: Option<String>,
    pub google_calendar_event_id: Option<String>,
    pub panel_members: String,
    pub notes: Option<String>,
    pub created_by: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InterviewPanelMember {
    pub email: String,
    pub name: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Panelist {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub role: Option<String>,
    pub department: Option<String>,
    pub is_active: i32,
    pub usage_count: i32,
    pub last_used_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInterviewRequest {
    pub application_id: String,
    pub scheduled_date: String,
    pub duration_minutes: i32,
    pub interview_type: String,
    pub panel_members: Vec<InterviewPanelMember>,
    pub notes: Option<String>,
    pub create_google_meet: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateInterviewRequest {
    pub scheduled_date: Option<String>,
    pub duration_minutes: Option<i32>,
    pub interview_type: Option<String>,
    pub panel_members: Option<Vec<InterviewPanelMember>>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InterviewWithDetails {
    #[serde(flatten)]
    pub interview: Interview,
    pub candidate_name: String,
    pub candidate_email: String,
    pub job_title: String,
    pub panel_members_parsed: Vec<InterviewPanelMember>,
}

#[derive(Debug, Serialize)]
pub struct GoogleMeetLinkResponse {
    pub meet_link: String,
    pub calendar_event_id: String,
    pub calendar_event_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateGoogleMeetRequest {
    pub summary: String,
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub attendees: Vec<String>,
}

// ============================================================================
// Candidate Stage Management Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CandidateApplication {
    pub id: String,
    pub job_id: String,
    pub candidate_id: String,
    pub resume_id: Option<String>,
    pub current_stage: String,
    pub status: String,
    pub cover_letter: Option<String>,
    pub applied_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StageHistory {
    pub id: String,
    pub application_id: String,
    pub stage: String,
    pub changed_by: String,
    pub changed_by_name: Option<String>,
    pub notes: Option<String>,
    pub changed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCandidateStageRequest {
    pub stage: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApproveCandidateRequest {
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RejectCandidateRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct SendCandidateEmailRequest {
    pub subject: String,
    pub content: String,
    pub cc: Option<Vec<String>>,
    pub email_type: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EmailHistory {
    pub id: String,
    pub application_id: String,
    pub candidate_id: String,
    pub job_id: String,
    pub subject: String,
    pub content: String,
    pub cc: Option<String>,
    pub sent_by: String,
    pub sent_at: String,
    pub email_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CandidateApplicationWithDetails {
    #[serde(flatten)]
    pub application: CandidateApplication,
    pub candidate_name: String,
    pub candidate_email: String,
    pub candidate_phone: Option<String>,
    pub job_title: String,
    pub job_company: Option<String>,
    pub resume_filename: Option<String>,
    pub stage_history: Vec<StageHistory>,
}

#[derive(Debug, Serialize)]
pub struct CandidateWithApplications {
    pub candidate_id: String,
    pub candidate_name: String,
    pub candidate_email: String,
    pub candidate_phone: Option<String>,
    pub candidate_avatar: Option<String>,
    pub applications: Vec<CandidateApplicationWithDetails>,
}
