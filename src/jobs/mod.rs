// src/jobs/mod.rs

pub mod handlers;
pub mod models;
pub mod routes;
pub mod validators;

#[cfg(test)]
mod tests;

// Re-export commonly used items
pub use models::*;
pub use routes::jobs_routes;
