// src/main.rs
use axum::{extract::Extension, middleware, Router};
use dotenv::dotenv;
use reqwest::Client;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::{net::SocketAddr, str::FromStr, sync::Arc};
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

// ============================================================================
// MODULE IMPORTS
// ============================================================================

mod admin;
mod auth;
mod candidates;
mod common;
mod companies;
mod jobs;
mod logging_middleware;
mod messages;
mod profile;
mod rate_limit_middleware;
mod services;

// ============================================================================
// COMMON IMPORTS
// ============================================================================

use common::AppState;
use common::dev_mode::{apply_cli_override, print_dev_mode_status, DevModeConfig};
use rate_limit_middleware::rate_limit_middleware;
use services::{AWSService, GoogleService, OpenAIService, RateLimitService, SettingsService};

// ============================================================================
// MAIN APPLICATION ENTRY POINT
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let admin_emails_raw = env::var("ADMIN_EMAILS").unwrap_or_default();
    info!("Raw ADMIN_EMAILS from env: '{}'", admin_emails_raw);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    // ========================================================================
    // ENVIRONMENT CONFIGURATION
    // ========================================================================

    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://job_api.db".to_string());
    let resumes_dir = env::var("RESUMES_DIR").unwrap_or_else(|_| "./resumes".to_string());
    let avatars_dir = env::var("AVATARS_DIR").unwrap_or_else(|_| "./uploads/avatars".to_string());
    let logos_dir = env::var("LOGOS_DIR").unwrap_or_else(|_| "./uploads/logos".to_string());
    let jwt_secret =
        env::var("JWT_SECRET").unwrap_or_else(|_| "replace_with_strong_secret".to_string());
    let google_client_id = env::var("GOOGLE_CLIENT_ID").ok();
    let openai_api_key = env::var("OPENAI_API_KEY").ok();
    let openai_model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4".to_string());

    // Parse admin emails from comma-separated env var
    let admin_emails: HashSet<String> = admin_emails_raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Loaded admin emails: {:?}", admin_emails);

    // ========================================================================
    // DEV MODE CONFIGURATION
    // ========================================================================

    let dev_mode = apply_cli_override(DevModeConfig::from_env());
    print_dev_mode_status(&dev_mode);

    // ========================================================================
    // DIRECTORY SETUP
    // ========================================================================

    tokio::fs::create_dir_all(&resumes_dir).await?;
    tokio::fs::create_dir_all(&avatars_dir).await?;
    tokio::fs::create_dir_all(&logos_dir).await?;
    tokio::fs::create_dir_all("./uploads/job-images/logos").await?;
    tokio::fs::create_dir_all("./uploads/job-images/jobs").await?;

    // ========================================================================
    // DATABASE SETUP
    // ========================================================================

    if let Some(path_part) = database_url.strip_prefix("sqlite://") {
        let path_without_params = path_part.split('?').next().unwrap_or("");
        if !path_without_params.is_empty() && !path_without_params.starts_with(':') {
            let db_path = PathBuf::from(path_without_params);
            if let Some(parent) = db_path.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
        }
    }

    let connect_options = SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options)
        .await?;
    
    // Run database migrations
    common::migrations::run_migrations(&pool).await?;

    // ========================================================================
    // SERVICE INITIALIZATION
    // ========================================================================

    let http_client = Client::builder().no_proxy().build()?;

    let settings_service = Arc::new(SettingsService::new(pool.clone()));
    info!("SettingsService initialized");

    let openai_service = Arc::new(OpenAIService::new(settings_service.clone()));
    info!("OpenAIService initialized");

    let aws_service = Arc::new(AWSService::new(settings_service.clone()));
    info!("AWSService initialized");

    let google_service = Arc::new(GoogleService::new(settings_service.clone()));
    info!("GoogleService initialized");
    
    // Sync Google OAuth credentials from environment to database
    if let Err(e) = google_service.sync_env_to_settings().await {
        warn!("Failed to sync Google credentials to database: {}", e);
    }

    let rate_limit_service = Arc::new(RateLimitService::new(settings_service.clone()));
    info!("RateLimitService initialized");

    let pdf_service = Arc::new(services::PDFService::new(
        pool.clone(),
        settings_service.clone(),
        aws_service.clone(),
    ));
    info!("PDFService initialized");

    let connection_manager = messages::services::ConnectionManager::new();
    info!("ConnectionManager initialized");

    messages::services::WebSocketService::start_cleanup_task(connection_manager.clone());
    info!("WebSocket cleanup task started");

    // Initialize job templates
    let templates_service = services::job_templates::JobTemplatesService::new(pool.clone());
    if let Err(e) = templates_service.initialize_system_templates().await {
        tracing::warn!("Failed to initialize system templates: {}", e);
    } else {
        info!("Job templates initialized");
    }

    // ========================================================================
    // APPLICATION STATE
    // ========================================================================

    let app_state = AppState {
        db: pool,
        resumes_dir: PathBuf::from(resumes_dir),
        avatars_dir: PathBuf::from(avatars_dir),
        logos_dir: PathBuf::from(logos_dir),
        job_images_logos_dir: PathBuf::from("./uploads/job-images/logos"),
        job_images_jobs_dir: PathBuf::from("./uploads/job-images/jobs"),
        http: http_client,
        jwt_secret,
        google_client_id,
        openai_api_key,
        openai_model,
        admin_emails,
        dev_mode,
        settings_service,
        openai_service,
        aws_service,
        google_service,
        rate_limit_service: rate_limit_service.clone(),
        pdf_service,
        connection_manager,
    };

    let shared = Arc::new(RwLock::new(app_state));

    // ========================================================================
    // ROUTER COMPOSITION
    // ========================================================================

    let app = Router::new()
        // ====================================================================
        // AUTHENTICATION ROUTES
        // ====================================================================
        .merge(auth::auth_routes())
        // ====================================================================
        // JOB ROUTES (Public and Admin)
        // ====================================================================
        .merge(jobs::jobs_routes())
        // ====================================================================
        // CANDIDATE ROUTES (Applications, Resumes, Interviews, Videos)
        // ====================================================================
        .merge(candidates::candidates_routes())
        // ====================================================================
        // PROFILE ROUTES (Profile, Experience, Education, Avatar, Testimonials)
        // ====================================================================
        .merge(profile::profile_routes())
        // ====================================================================
        // MESSAGING ROUTES (WebSocket and REST API)
        // ====================================================================
        .merge(messages::messages_routes())
        // ====================================================================
        // COMPANY ROUTES
        // ====================================================================
        .merge(companies::companies_routes())
        // ====================================================================
        // ADMIN ROUTES (Dashboard, Users, Settings, Exports, Files)
        // ====================================================================
        .merge(admin::admin_routes())
        // ====================================================================
        // MIDDLEWARE AND LAYERS
        // ====================================================================
        // Add request/response body logging in debug mode
        .layer(middleware::from_fn(logging_middleware::log_request_response))
        .layer(middleware::from_fn(rate_limit_middleware))
        .layer(Extension(rate_limit_service))
        .layer(Extension(shared.clone()))
        .layer({
            // Get CORS origins from environment variable
            let cors_origins = std::env::var("CORS_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:3000,http://localhost:3001,http://localhost:5173".to_string());
            
            let origins: Vec<axum::http::HeaderValue> = cors_origins
                .split(',')
                .filter_map(|origin| origin.trim().parse().ok())
                .collect();
            
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::PATCH,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    axum::http::HeaderName::from_static("x-request-id"),
                ])
                .allow_credentials(true)
        })
        .layer(TraceLayer::new_for_http());

    // ========================================================================
    // SERVER STARTUP
    // ========================================================================

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
