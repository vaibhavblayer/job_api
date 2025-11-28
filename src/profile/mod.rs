// src/profile/mod.rs

pub mod handlers;
pub mod models;
pub mod routes;
pub mod validators;

#[cfg(test)]
mod tests;

pub use routes::profile_routes;
