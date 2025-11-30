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
    pub summary: Option<String>,
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
    pub summary: Option<String>,
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
            summary: job.summary,
            description: job.description,
            location: job.location,
            company: job.company,
            company_id: job.company_id,
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

// ============================================================================
// Content Version Models (Inline AI Editor)
// ============================================================================

/// Content component types that can be versioned
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentComponentType {
    Title,
    Summary,
    Description,
    Requirements,
    Benefits,
    Image,
}

impl ContentComponentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentComponentType::Title => "title",
            ContentComponentType::Summary => "summary",
            ContentComponentType::Description => "description",
            ContentComponentType::Requirements => "requirements",
            ContentComponentType::Benefits => "benefits",
            ContentComponentType::Image => "image",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "title" => Some(ContentComponentType::Title),
            "summary" => Some(ContentComponentType::Summary),
            "description" => Some(ContentComponentType::Description),
            "requirements" => Some(ContentComponentType::Requirements),
            "benefits" => Some(ContentComponentType::Benefits),
            "image" => Some(ContentComponentType::Image),
            _ => None,
        }
    }
}

impl std::fmt::Display for ContentComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single content version record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ContentVersion {
    pub id: String,
    pub job_id: String,
    pub component_type: String,
    pub content: String,
    pub prompt_used: Option<String>,
    pub is_active: i32,
    pub version_number: i32,
    pub created_by: Option<String>,
    pub created_at: String,
}

/// Response for getting content versions
#[derive(Debug, Serialize)]
pub struct ContentVersionsResponse {
    pub active: Option<ContentVersion>,
    pub history: Vec<ContentVersion>,
    pub total: usize,
}

/// Request to generate new content
#[derive(Debug, Deserialize)]
pub struct GenerateContentRequest {
    pub prompt: Option<String>,
    pub tone: Option<String>, // "professional", "casual", "technical"
}

/// Response after generating content
#[derive(Debug, Serialize)]
pub struct GenerateContentResponse {
    pub version: ContentVersion,
}

/// Response for activation
#[derive(Debug, Serialize)]
pub struct ActivateVersionResponse {
    pub success: bool,
    pub version: ContentVersion,
}

/// Response for deletion
#[derive(Debug, Serialize)]
pub struct DeleteVersionResponse {
    pub success: bool,
}
