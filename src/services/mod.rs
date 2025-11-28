// src/services/mod.rs
//
// Shared services module containing business logic services
// that can be used across different domain modules

pub mod aws;
pub mod email;
pub mod encryption;
pub mod google;
pub mod interviews;
pub mod job_templates;
pub mod monitoring;
pub mod openai;
pub mod panelists;
pub mod pdf;
pub mod rate_limit;
pub mod settings;
pub mod video;
pub mod youtube;

// Re-export commonly used types for convenience
pub use aws::AWSService;
pub use google::GoogleService;
pub use openai::OpenAIService;
pub use pdf::PDFService;
pub use rate_limit::RateLimitService;
pub use settings::SettingsService;
