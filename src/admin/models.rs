// src/admin/models.rs

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

// Dashboard models
#[derive(Debug, Serialize)]
pub struct DashboardMetrics {
    // Core metrics
    pub total_jobs: i64,
    pub active_jobs: i64,
    pub draft_jobs: i64,
    pub closed_jobs: i64,
    pub total_applications: i64,
    pub pending_reviews: i64,
    pub new_messages: i64,
    pub total_candidates: i64,
    pub system_health: String,
    pub last_updated: String,
    
    // Breakdowns for charts
    pub jobs_by_status: HashMap<String, i64>,
    pub applications_by_status: HashMap<String, i64>,
    
    // Recent activity
    pub recent_activity: Vec<ActivityLog>,
    
    // Top items
    pub top_jobs: Vec<TopJob>,
    
    // Trends (last 7 days)
    pub application_trends: Vec<TrendData>,
}

#[derive(Debug, Serialize)]
pub struct TopJob {
    pub job_id: String,
    pub job_title: String,
    pub company: String,
    pub application_count: i64,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct TrendData {
    pub date: String,
    pub applications: i64,
    pub jobs_posted: i64,
}

#[derive(Debug, Serialize)]
pub struct SystemHealth {
    pub database_status: String,
    pub api_status: String,
    pub storage_status: String,
    pub overall_health: String,
    pub last_check: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ActivityLog {
    pub id: String,
    pub activity_type: String,
    pub description: String,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub metadata: Option<String>,
    pub timestamp: String,
}

// User management models
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AdminUser {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub permissions: Option<String>, // JSON string
    pub created_at: Option<String>,
    pub created_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminUserRequest {
    pub user_id: String,
    pub role: String,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAdminUserRequest {
    pub role: Option<String>,
    pub permissions: Option<Vec<String>>,
}

// Settings models
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemSetting {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SettingUpdate {
    pub value: String,
    pub encrypt: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSystemSettingsRequestV2 {
    pub settings: HashMap<String, SettingUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct TestConnectionRequest {
    pub service: String, // "openai", "aws_s3", "aws_ses", "google"
    pub credentials: Option<HashMap<String, String>>,
}

// File management models
#[derive(Deserialize)]
pub struct ListFilesQuery {
    pub prefix: Option<String>,
    pub search: Option<String>,
    pub storage_type: Option<String>,
}

#[derive(Serialize)]
pub struct FileItem {
    pub name: String,
    pub path: String,
    pub size: i64,
    #[serde(rename = "type")]
    pub file_type: String,
    pub uploaded_at: Option<String>,
    pub uploaded_by: Option<String>,
    pub url: String,
}

#[derive(Serialize)]
pub struct ListFilesResponse {
    pub files: Vec<FileItem>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct StorageStats {
    pub total_size: i64,
    pub file_count: i64,
    pub storage_type: String,
    pub quota: Option<i64>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

// Candidate profile model
#[derive(Debug, Serialize)]
pub struct CandidateProfile {
    pub user: crate::auth::models::User,
    pub profile: Option<crate::profile::models::Profile>,
    pub experiences: Vec<crate::profile::models::Experience>,
    pub education: Vec<crate::profile::models::Education>,
    pub applications: Vec<crate::candidates::models::Application>,
    pub resume_status: String,
    pub resume_id: Option<String>,
    pub resume_filename: Option<String>,
    pub last_activity: Option<String>,
    pub total_applications: i64,
    pub application_success_rate: f64,
}
