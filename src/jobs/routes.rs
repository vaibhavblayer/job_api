// src/jobs/routes.rs

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

use super::handlers::{self, ai, images};

/// Create the jobs router with all job-related routes
pub fn jobs_routes() -> Router {
    Router::new()
        // AI-powered job content generation routes
        .route("/api/admin/jobs/ai/generate-description", post(ai::generate_job_description))
        .route("/api/admin/jobs/ai/generate-benefits", post(ai::generate_job_benefits))
        .route("/api/admin/jobs/ai/generate-requirements", post(ai::generate_job_requirements))
        .route("/api/admin/jobs/ai/suggest-skills", post(ai::suggest_skills))
        .route("/api/admin/jobs/ai/analyze-bias", post(ai::analyze_bias))
        .route("/api/admin/jobs/ai/readability-score", post(ai::calculate_readability_score))
        .route("/api/admin/jobs/ai/generate-social-post", post(ai::generate_social_post))
        .route("/api/admin/jobs/ai/generate-all", post(ai::generate_all_job_content))
        .route("/api/admin/jobs/ai/generate-from-template", post(ai::generate_from_ai_template))
        // Job image management routes
        .route("/api/admin/jobs/upload-image", post(images::upload_job_image))
        .route("/api/job-images/:type/:filename", get(images::serve_job_image))
        .route("/api/admin/jobs/images/:filename", delete(images::delete_job_image))
        // Public routes
        .route("/api/jobs", get(handlers::list_jobs_or_featured))
        .route("/api/jobs/:id", get(handlers::get_job_by_id))
        .route("/api/jobs/:id/view", post(handlers::track_job_view))
        .route("/api/jobs/:id/stats", get(handlers::get_job_stats))
        .route("/api/public/stats", get(handlers::get_public_stats))
        // Admin job management routes
        .route("/api/admin/jobs", post(handlers::admin_create_job))
        .route(
            "/api/admin/jobs/:id",
            put(handlers::admin_update_job).delete(handlers::admin_delete_job),
        )
        // Enhanced job management endpoints
        .route(
            "/api/admin/jobs/:id/status",
            patch(handlers::admin_update_job_status),
        )
        .route(
            "/api/admin/jobs/:id/toggle-featured",
            patch(handlers::admin_toggle_featured_status),
        )
        .route(
            "/api/admin/jobs/draft",
            post(handlers::admin_save_job_draft),
        )
        .route(
            "/api/admin/jobs/draft/:id",
            get(handlers::admin_load_job_draft),
        )
        .route(
            "/api/admin/jobs/:id/detailed-analytics",
            get(handlers::admin_get_job_detailed_analytics),
        )
        // Job analytics endpoints
        .route(
            "/api/admin/jobs/analytics",
            get(handlers::get_job_analytics),
        )
        // Bulk job operations
        .route(
            "/api/admin/jobs/bulk-update-status",
            post(handlers::bulk_update_job_status),
        )
        .route(
            "/api/admin/jobs/bulk-delete",
            post(handlers::bulk_delete_jobs),
        )
        // Job template routes
        // NOTE: Specific routes must come BEFORE parameterized routes (:id)
        .route(
            "/api/admin/job-templates",
            get(handlers::get_templates).post(handlers::create_template),
        )
        .route(
            "/api/admin/job-templates/available",
            get(handlers::get_available_templates),
        )
        .route(
            "/api/admin/job-templates/composer",
            get(handlers::get_job_composer_templates),
        )
        .route(
            "/api/admin/job-templates/ai",
            post(handlers::create_ai_template),
        )
        .route(
            "/api/admin/job-templates/:id/ai-context",
            get(handlers::get_ai_template_context),
        )
        .route(
            "/api/admin/job-templates/:id",
            get(handlers::get_template_by_id)
                .put(handlers::update_template)
                .delete(handlers::delete_template),
        )
}
