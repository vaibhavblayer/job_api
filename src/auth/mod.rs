//! # Auth Module
//!
//! This module handles all authentication-related functionality including:
//! - Google OAuth authentication
//! - JWT token generation and validation
//! - User authentication and authorization
//! - AuthedUser extractor for protected routes

pub mod extractors;
pub mod handlers;
pub mod models;
pub mod routes;

#[cfg(test)]
mod tests;

pub use extractors::AuthedUser;
pub use models::User;
pub use routes::auth_routes;
