use super::{assets, handlers};
use axum::{
    routing::{delete, get, patch, post},
    Router,
};

/// Creates the companies router with all company-related routes
pub fn companies_routes() -> Router {
    Router::new()
        // Logo management routes
        .route("/api/admin/logo/upload", post(assets::upload_logo))
        .route("/api/admin/logos", get(assets::list_logos))
        .route("/api/admin/logo/activate", post(assets::activate_logo))
        .route("/api/logos/:filename", get(assets::serve_logo))
        .route("/api/admin/logo/:filename", delete(assets::delete_logo_file))
        // Company CRUD routes
        .route(
            "/api/admin/companies",
            get(handlers::get_companies).post(handlers::create_company),
        )
        .route(
            "/api/admin/companies/:id",
            get(handlers::get_company_by_id)
                .put(handlers::update_company)
                .delete(handlers::delete_company),
        )
        // Company asset routes
        .route(
            "/api/admin/companies/:id/assets",
            get(handlers::get_company_assets).post(handlers::upload_company_asset),
        )
        .route(
            "/api/admin/companies/:company_id/assets/:asset_id",
            delete(handlers::delete_company_asset),
        )
        .route(
            "/api/admin/companies/:company_id/assets/:asset_id/set-default",
            patch(handlers::set_default_asset),
        )
        .route(
            "/api/admin/companies/:company_id/assets/save-url",
            post(handlers::save_url_as_company_asset),
        )
}
