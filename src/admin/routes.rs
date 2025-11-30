// src/admin/routes.rs

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

use super::handlers;

pub fn admin_routes() -> Router {
    Router::new()
        // Public contact form endpoint
        .route(
            "/api/public/contact",
            post(handlers::contact::submit_contact_form),
        )
        // Dashboard and analytics endpoints
        .route(
            "/api/admin/dashboard/metrics",
            get(handlers::dashboard::get_dashboard_metrics),
        )
        .route(
            "/api/admin/system/health",
            get(handlers::dashboard::get_system_health),
        )
        .route(
            "/api/admin/activity",
            get(handlers::dashboard::get_recent_activity),
        )
        // Admin user management endpoints
        .route(
            "/api/admin/users",
            get(handlers::users::get_admin_users).post(handlers::users::create_admin_user),
        )
        .route(
            "/api/admin/users/:id",
            put(handlers::users::update_admin_user).delete(handlers::users::delete_admin_user),
        )
        .route(
            "/api/admin/users/:id/toggle-status",
            patch(handlers::users::toggle_admin_user_status),
        )
        // Candidate management endpoints
        .route(
            "/api/admin/candidates",
            get(handlers::users::get_admin_candidates),
        )
        .route(
            "/api/admin/candidates/:id",
            get(handlers::users::get_admin_candidate_details),
        )
        // Data export endpoints
        .route(
            "/api/admin/export/jobs",
            get(handlers::exports::export_jobs),
        )
        .route(
            "/api/admin/export/applications",
            get(handlers::exports::export_applications),
        )
        .route(
            "/api/admin/export/candidates",
            get(handlers::exports::export_candidates),
        )
        // System settings endpoints
        .route(
            "/api/settings/public",
            get(handlers::settings::get_public_system_settings),
        )
        .route(
            "/api/admin/settings",
            get(handlers::settings::get_system_settings)
                .put(handlers::settings::update_system_settings),
        )
        .route(
            "/api/admin/settings/test-connection",
            post(handlers::settings::test_service_connection),
        )
        // Theme settings endpoints
        .route(
            "/api/settings/theme",
            get(handlers::theme::get_theme_settings),
        )
        .route(
            "/api/admin/settings/theme",
            put(handlers::theme::update_theme_settings),
        )
        // Google OAuth endpoints
        .route(
            "/api/admin/settings/google/auth-url",
            get(handlers::settings::get_google_auth_url),
        )
        .route(
            "/api/admin/settings/google/callback",
            get(handlers::settings::google_oauth_callback),
        )
        .route(
            "/api/admin/settings/google/status",
            get(handlers::settings::get_google_connection_status),
        )
        .route(
            "/api/admin/settings/google/disconnect",
            post(handlers::settings::disconnect_google_account),
        )
        // File manager endpoints
        .route("/api/admin/files", get(handlers::files::list_files_handler))
        .route(
            "/api/admin/files/delete-bulk",
            post(handlers::files::delete_files_bulk_handler),
        )
        .route(
            "/api/admin/files/stats",
            get(handlers::files::get_storage_stats_handler),
        )
        .route(
            "/api/admin/files/:path",
            delete(handlers::files::delete_file_handler),
        )
}
