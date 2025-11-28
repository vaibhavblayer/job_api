// src/profile/models.rs

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ============================================================================
// Profile Management Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Experience {
    pub id: String,
    pub user_id: String,
    pub company: String,
    pub title: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Education {
    pub id: String,
    pub user_id: String,
    pub institution: String,
    pub degree: String,
    pub field_of_study: Option<String>,
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

// Request models for profile management
#[derive(Debug, Deserialize)]
pub struct CreateExperienceRequest {
    pub company: String,
    pub title: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateExperienceRequest {
    pub company: Option<String>,
    pub title: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEducationRequest {
    pub institution: String,
    pub degree: String,
    pub field_of_study: Option<String>,
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEducationRequest {
    pub institution: Option<String>,
    pub degree: Option<String>,
    pub field_of_study: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub description: Option<String>,
}

// ============================================================================
// Profile Models
// ============================================================================

#[derive(FromRow, Serialize, Deserialize, Debug)]
pub struct Profile {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    #[serde(rename = "linkedinUrl")]
    pub linkedin_url: Option<String>,
    #[serde(rename = "githubUrl")]
    pub github_url: Option<String>,
    #[serde(
        rename = "skills",
        serialize_with = "crate::common::helpers::serialize_skills",
        deserialize_with = "crate::common::helpers::deserialize_skills"
    )]
    pub skills: Option<String>, // JSON string of skills array
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(rename = "resumeStatus")]
    pub resume_status: Option<String>,
    #[serde(rename = "lastResumeId")]
    pub last_resume_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    #[serde(rename = "linkedinUrl")]
    pub linkedin_url: Option<String>,
    #[serde(rename = "githubUrl")]
    pub github_url: Option<String>,
    pub skills: Option<Vec<String>>,
}

// ============================================================================
// Avatar Models
// ============================================================================

#[derive(Serialize)]
pub struct AvatarUploadResponse {
    pub avatar_url: String,
    pub message: String,
}

#[derive(Deserialize)]
pub struct AvatarUpdateRequest {
    pub avatar_url: Option<String>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

// ============================================================================
// Testimonial Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Testimonial {
    pub id: String,
    pub user_id: String,
    pub content: String,
    pub rating: Option<i32>,
    pub position: Option<String>,
    pub company: Option<String>,
    #[serde(deserialize_with = "deserialize_bool_from_int")]
    #[serde(serialize_with = "serialize_bool_to_bool")]
    pub featured: i64,
    #[serde(deserialize_with = "deserialize_bool_from_int")]
    #[serde(serialize_with = "serialize_bool_to_bool")]
    pub approved: i64,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestimonialWithUser {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_email: String,
    pub user_avatar: Option<String>,
    pub content: String,
    pub rating: Option<i32>,
    pub position: Option<String>,
    pub company: Option<String>,
    pub featured: bool,
    pub approved: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTestimonialRequest {
    pub content: String,
    pub rating: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTestimonialRequest {
    pub content: Option<String>,
    pub rating: Option<i32>,
    pub position: Option<String>,
    pub company: Option<String>,
    pub featured: Option<bool>,
    pub approved: Option<bool>,
}

// Helper functions for serializing/deserializing SQLite INTEGER booleans
fn deserialize_bool_from_int<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    i64::deserialize(deserializer)
}

fn serialize_bool_to_bool<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bool(*value != 0)
}
