//! # Companies Module
//!
//! This module handles all company-related functionality including:
//! - Company CRUD operations
//! - Company asset management (logos and images)
//! - Company information and metadata

pub mod assets;
pub mod handlers;
pub mod models;
pub mod routes;
pub mod services;
pub mod validators;

#[cfg(test)]
mod tests;

pub use routes::companies_routes;
