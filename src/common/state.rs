// Application state shared across all modules

use reqwest::Client;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::services::{
    AWSService, GoogleService, OpenAIService, PDFService, RateLimitService, SettingsService,
};
use crate::common::dev_mode::DevModeConfig;

/// Application state containing database pool, services, and configuration
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub resumes_dir: PathBuf,
    pub avatars_dir: PathBuf,
    pub logos_dir: PathBuf,
    pub job_images_logos_dir: PathBuf,
    pub job_images_jobs_dir: PathBuf,
    pub http: Client,
    pub jwt_secret: String,
    pub google_client_id: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub admin_emails: HashSet<String>,
    pub dev_mode: DevModeConfig,
    pub settings_service: Arc<SettingsService>,
    pub openai_service: Arc<OpenAIService>,
    pub aws_service: Arc<AWSService>,
    pub google_service: Arc<GoogleService>,
    pub rate_limit_service: Arc<RateLimitService>,
    pub pdf_service: Arc<PDFService>,
    pub connection_manager: crate::messages::services::ConnectionManager,
}
