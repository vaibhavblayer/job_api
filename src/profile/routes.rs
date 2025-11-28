// src/profile/routes.rs

use axum::{
    routing::{get, post, put},
    Router,
};

use super::handlers::{avatar, education, experience, profile, testimonials};

pub fn profile_routes() -> Router {
    Router::new()
        // Profile routes
        .route(
            "/api/profile",
            get(profile::profile_handler).put(profile::update_profile_handler),
        )
        // Experience routes
        .route(
            "/api/profile/experience",
            get(experience::get_experiences).post(experience::create_experience),
        )
        .route(
            "/api/profile/experience/:id",
            put(experience::update_experience).delete(experience::delete_experience),
        )
        // Education routes
        .route(
            "/api/profile/education",
            get(education::get_education).post(education::create_education),
        )
        .route(
            "/api/profile/education/:id",
            put(education::update_education).delete(education::delete_education),
        )
        // Avatar routes
        .route(
            "/api/user/avatar",
            post(avatar::upload_avatar)
                .put(avatar::update_avatar_url)
                .delete(avatar::remove_avatar),
        )
        .route("/api/avatars/:filename", get(avatar::serve_avatar))
        // Testimonial routes
        .route(
            "/api/testimonials",
            get(testimonials::get_public_testimonials).post(testimonials::create_testimonial),
        )
        .route(
            "/api/testimonials/my",
            get(testimonials::get_my_testimonials),
        )
        .route(
            "/api/testimonials/:id",
            put(testimonials::update_testimonial).delete(testimonials::delete_testimonial),
        )
        .route(
            "/api/admin/testimonials",
            get(testimonials::get_all_testimonials),
        )
        .route(
            "/api/admin/testimonials/:id/approve",
            post(testimonials::approve_testimonial),
        )
        .route(
            "/api/admin/testimonials/:id/feature",
            post(testimonials::toggle_feature_testimonial),
        )
        .route(
            "/api/admin/candidates/:id/testimonials",
            get(testimonials::get_candidate_testimonials),
        )
}
