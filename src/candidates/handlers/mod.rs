// src/candidates/handlers/mod.rs

pub mod ai;
pub mod applications;
pub mod email_templates;
pub mod interview_email_templates;
pub mod files;
pub mod interviews;
pub mod resumes;
pub mod saved_jobs;
pub mod videos;
pub mod youtube_videos;

// Re-export handler functions
pub use ai::*;
pub use applications::*;
pub use interviews::*;
pub use resumes::*;
pub use saved_jobs::*;
pub use videos::*;
pub use youtube_videos::*;
