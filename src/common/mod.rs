// Common module - shared types and utilities across all modules

pub mod dev_mode;
pub mod error;
pub mod helpers;
pub mod id_generator;
pub mod migrations;
pub mod state;
pub mod validation;

// Re-export commonly used types for convenience
pub use error::ApiError;
pub use helpers::safe_email_log;
pub use id_generator::*;
pub use state::AppState;
pub use validation::{ValidationError, ValidationResult, Validator};
