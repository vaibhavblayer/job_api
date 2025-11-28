// src/jobs/models.rs

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

// ============================================================================
// Job Models
// ============================================================================

#[derive(FromRow, Serialize, Deserialize, Debug)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub company_logo_url: Option<String>,
    pub job_image_url: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub job_type: Option<String>,
    pub experience_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>, // JSON string in DB, will be parsed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benefits: Option<String>, // JSON string in DB, will be parsed
    pub status: Option<String>,
    pub is_featured: Option<i64>, // 0 or 1 in SQLite
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub published_at: Option<String>,
}

// Enhanced Job response with parsed arrays
#[derive(Serialize, Debug)]
pub struct JobResponse {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub company_logo_url: Option<String>,
    pub job_image_url: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub job_type: Option<String>,
    pub experience_level: Option<String>,
    pub requirements: Option<Vec<String>>,
    pub benefits: Option<Vec<String>>,
    pub status: Option<String>,
    pub is_featured: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub published_at: Option<String>,
}

// Paginated job list response
#[derive(Serialize, Debug)]
pub struct JobListResponse {
    pub jobs: Vec<JobResponse>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

impl From<Job> for JobResponse {
    fn from(job: Job) -> Self {
        // Parse requirements JSON string to Vec<String>
        let requirements = job
            .requirements
            .and_then(|r| serde_json::from_str::<Vec<String>>(&r).ok());

        // Parse benefits JSON string to Vec<String>
        let benefits = job
            .benefits
            .and_then(|b| serde_json::from_str::<Vec<String>>(&b).ok());

        JobResponse {
            id: job.id,
            title: job.title,
            description: job.description,
            location: job.location,
            company: job.company,
            company_logo_url: job.company_logo_url,
            job_image_url: job.job_image_url,
            salary_min: job.salary_min,
            salary_max: job.salary_max,
            job_type: job.job_type,
            experience_level: job.experience_level,
            requirements,
            benefits,
            status: job.status,
            is_featured: job.is_featured.unwrap_or(0) == 1,
            created_at: job.created_at,
            updated_at: job.updated_at,
            published_at: job.published_at,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateJob {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub company_id: Option<String>,
    pub company_logo_url: Option<String>,
    pub job_image_url: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub job_type: Option<String>,
    pub experience_level: Option<String>,
    pub requirements: Option<Vec<String>>,
    pub benefits: Option<Vec<String>>,
    pub educational_qualifications: Option<serde_json::Value>,
    pub is_featured: Option<bool>,
    pub template_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateJob {
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub company: Option<String>,
    pub company_id: Option<String>,
    pub company_logo_url: Option<String>,
    pub job_image_url: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub job_type: Option<String>,
    pub experience_level: Option<String>,
    pub requirements: Option<Vec<String>>,
    pub benefits: Option<Vec<String>>,
    pub educational_qualifications: Option<serde_json::Value>,
    pub is_featured: Option<bool>,
    pub template_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateJobStatusRequest {
    pub status: String,
    pub notes: Option<String>,
}

// ============================================================================
// Job Analytics Models
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct JobQueryParams {
    pub featured: Option<String>,
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct JobAnalyticsRequest {
    pub job_id: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JobViewRequest {
    pub user_agent: Option<String>,
    pub referrer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobView {
    pub id: String,
    pub job_id: String,
    pub user_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub viewed_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DailyMetric {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ReferrerStats {
    pub referrer: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct JobStats {
    pub job_id: String,
    pub job_title: String,
    pub total_views: i64,
    pub unique_views: i64,
    pub total_applications: i64,
    pub conversion_rate: f64,
    pub recent_views: Vec<JobView>,
    pub view_trend: Vec<DailyMetric>,
    pub application_trend: Vec<DailyMetric>,
}

#[derive(Debug, Serialize)]
pub struct JobAnalyticsResponse {
    pub total_jobs: i64,
    pub total_views: i64,
    pub total_applications: i64,
    pub average_conversion_rate: f64,
    pub top_performing_jobs: Vec<JobPerformanceStats>,
    pub view_trends: Vec<DailyMetric>,
    pub application_trends: Vec<DailyMetric>,
}

#[derive(Debug, Serialize)]
pub struct JobPerformanceStats {
    pub job_id: String,
    pub job_title: String,
    pub views: i64,
    pub applications: i64,
    pub conversion_rate: f64,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobDetailedAnalytics {
    pub job_id: String,
    pub job_title: String,
    pub views: i64,
    pub applications: i64,
    pub conversion_rate: f64,
    pub view_trend: Vec<DailyMetric>,
    pub application_trend: Vec<DailyMetric>,
    pub top_referrers: Vec<ReferrerStats>,
    pub applications_by_stage: HashMap<String, i64>,
    pub candidate_list: Vec<CandidateApplication>,
    pub status_history: Vec<JobStatusHistory>,
}

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
pub struct JobStatusHistory {
    pub id: String,
    pub job_id: String,
    pub old_status: Option<String>,
    pub new_status: String,
    pub changed_by: String,
    pub changed_by_name: Option<String>,
    pub notes: Option<String>,
    pub changed_at: Option<String>,
}

// ============================================================================
// Bulk Operation Models
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct BulkJobStatusUpdate {
    pub job_ids: Vec<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct BulkJobDelete {
    pub job_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkOperationResult {
    pub success_count: usize,
    pub failed_count: usize,
    pub errors: Vec<String>,
}

// ============================================================================
// Job Template Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobTemplate {
    pub id: String,
    pub name: String,
    pub company_id: Option<String>,
    pub template_type: String, // 'system', 'custom', or 'ai'
    pub job_data: String,      // JSON string
    pub ai_context: Option<String>, // JSON string for AI template context prompts
    pub created_by: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobTemplateRequest {
    pub name: String,
    pub company_id: Option<String>,
    pub job_data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateJobTemplateRequest {
    pub name: Option<String>,
    pub job_data: Option<serde_json::Value>,
    pub ai_context: Option<AITemplateContext>,
}

// ============================================================================
// AI Template Models
// ============================================================================

/// AI Template Context structure containing prompts for AI generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AITemplateContext {
    pub title_context: Option<String>,
    pub description_context: Option<String>,
    pub requirements_context: Option<String>,
    pub benefits_context: Option<String>,
    pub educational_qualifications_context: Option<String>,
}

/// Request to create an AI template
#[derive(Debug, Deserialize)]
pub struct CreateAITemplateRequest {
    pub name: String,
    pub company_id: String, // Required for AI templates
    pub ai_context: AITemplateContext,
}
