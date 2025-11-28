// src/candidates/mod.rs

pub mod handlers;
pub mod models;
pub mod routes;
pub mod validators;

#[cfg(test)]
mod tests;

// Re-export commonly used items
pub use routes::candidates_routes;
