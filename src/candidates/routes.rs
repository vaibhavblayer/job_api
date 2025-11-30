// src/candidates/routes.rs

use crate::candidates::handlers::{self, files};
use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

pub fn candidates_routes() -> Router {
    Router::new()
        // File serving routes
        .route("/uploads/resumes/*path", get(files::serve_resume_file))
        // Application routes
        .route(
            "/api/applications",
            post(handlers::create_application).get(handlers::get_user_applications),
        )
        .route(
            "/api/applications/:id",
            get(handlers::get_application_details),
        )
        .route(
            "/api/applications/:id/status",
            patch(handlers::update_application_status),
        )
        // Admin application routes
        .route(
            "/api/admin/jobs/:id/applications",
            get(handlers::get_job_applications),
        )
        // Job-centric candidate management routes (for frontend compatibility)
        .route(
            "/api/admin/jobs/:job_id/candidates/:candidate_id/approve",
            post(handlers::approve_candidate_for_job),
        )
        .route(
            "/api/admin/jobs/:job_id/candidates/:candidate_id/reject",
            post(handlers::reject_candidate_for_job),
        )
        .route(
            "/api/admin/jobs/:job_id/candidates/:candidate_id/email",
            post(handlers::send_candidate_email_for_job),
        )
        .route(
            "/api/admin/applications/analytics",
            get(handlers::get_application_analytics),
        )
        .route(
            "/api/admin/applications/bulk-update-status",
            post(handlers::bulk_update_application_status),
        )
        // Enhanced application management routes
        .route(
            "/api/admin/applications/:id/advance-stage",
            post(handlers::advance_application_stage),
        )
        .route(
            "/api/admin/applications/:id/send-email",
            post(handlers::send_application_email),
        )
        .route(
            "/api/admin/applications/bulk-action",
            post(handlers::bulk_application_action),
        )
        // Resume routes
        .route("/api/resumes", post(handlers::upload_resume))
        .route("/api/user/resumes", get(handlers::get_user_resumes))
        .route("/api/resumes/:id", delete(handlers::delete_resume))
        .route("/api/resumes/:id/label", put(handlers::update_resume_label))
        .route("/api/resumes/:id/scan", post(handlers::scan_resume))
        .route("/api/resumes/:id/review", get(handlers::get_resume_review))
        .route(
            "/api/resumes/:id/propagate-profile",
            post(handlers::propagate_resume_to_profile),
        )
        .route("/api/admin/resumes", get(handlers::admin_list_resumes))
        .route(
            "/api/admin/resumes/bulk-update-status",
            post(handlers::bulk_update_resume_status),
        )
        .route(
            "/api/resumes/:id/status",
            get(handlers::get_resume_processing_status),
        )
        .route("/api/resumes/:id/download", get(handlers::download_resume))
        .route(
            "/api/resumes/:id/retry-processing",
            post(handlers::retry_resume_processing),
        )
        // Video routes
        .route(
            "/api/user/videos",
            get(handlers::list_user_videos).post(handlers::upload_video),
        )
        .route("/api/user/videos/:id", delete(handlers::delete_video))
        .route(
            "/api/applications/:id/video",
            post(handlers::upload_video)
                .get(handlers::get_video)
                .delete(handlers::delete_video),
        )
        .route(
            "/api/admin/applications/:id/video/download",
            get(handlers::download_video),
        )
        // YouTube video routes
        .route(
            "/api/user/youtube/videos",
            get(handlers::list_youtube_videos),
        )
        .route(
            "/api/user/videos/youtube",
            post(handlers::link_youtube_video),
        )
        // YouTube OAuth routes
        .route(
            "/api/auth/youtube",
            get(handlers::youtube_oauth_start),
        )
        .route(
            "/api/auth/youtube/callback",
            get(handlers::youtube_oauth_callback),
        )
        // Interview routes
        .route(
            "/api/admin/interviews/schedule",
            post(handlers::schedule_interview),
        )
        .route(
            "/api/admin/interviews/:id",
            get(handlers::get_interview)
                .put(handlers::update_interview)
                .delete(handlers::cancel_interview),
        )
        .route(
            "/api/admin/interviews/create-google-meet",
            post(handlers::create_google_meet_link),
        )
        .route(
            "/api/admin/candidates/:candidateId/interviews",
            get(handlers::get_candidate_interviews),
        )
        .route(
            "/api/admin/jobs/:jobId/interviews",
            get(handlers::get_job_interviews),
        )
        // Panelist routes
        .route(
            "/api/admin/panelists",
            get(handlers::get_panelists),
        )
        // AI-powered candidate communication routes
        .route(
            "/api/admin/candidates/ai/generate-email",
            post(handlers::generate_candidate_email),
        )
        // Saved jobs routes
        .route(
            "/api/saved-jobs",
            get(handlers::get_saved_jobs).post(handlers::save_job),
        )
        .route(
            "/api/saved-jobs/:job_id",
            get(handlers::is_job_saved).delete(handlers::unsave_job),
        )
}
