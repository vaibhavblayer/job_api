// src/jobs/services/content_versions.rs
//! Content Versions Service for Inline AI Editor
//!
//! Manages AI-generated content versions for job fields (title, description, requirements, benefits).
//! Supports version history, activation, and automatic cleanup.

use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{error, info};

use crate::common::{generate_content_version_id, ApiError};
use crate::jobs::models::{ContentComponentType, ContentVersion, ContentVersionsResponse};
use crate::services::OpenAIService;

/// Maximum number of versions to keep per job+component
const MAX_VERSIONS_PER_COMPONENT: i32 = 10;

pub struct ContentVersionsService {
    db: SqlitePool,
    openai_service: Arc<OpenAIService>,
}

impl ContentVersionsService {
    pub fn new(db: SqlitePool, openai_service: Arc<OpenAIService>) -> Self {
        Self { db, openai_service }
    }

    /// Get all versions for a job component
    pub async fn get_versions(
        &self,
        job_id: &str,
        component_type: &str,
    ) -> Result<ContentVersionsResponse, ApiError> {
        // Validate component type
        ContentComponentType::from_str(component_type)
            .ok_or_else(|| ApiError::BadRequest(format!("Invalid component type: {}", component_type)))?;

        // Get active version
        let active = sqlx::query_as::<_, ContentVersion>(
            r#"
            SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at
            FROM job_content_versions
            WHERE job_id = ? AND component_type = ? AND is_active = 1
            "#,
        )
        .bind(job_id)
        .bind(component_type)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Get history (all versions, newest first)
        let history = sqlx::query_as::<_, ContentVersion>(
            r#"
            SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at
            FROM job_content_versions
            WHERE job_id = ? AND component_type = ?
            ORDER BY version_number DESC
            LIMIT ?
            "#,
        )
        .bind(job_id)
        .bind(component_type)
        .bind(MAX_VERSIONS_PER_COMPONENT)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        let total = history.len();

        Ok(ContentVersionsResponse {
            active,
            history,
            total,
        })
    }

    /// Generate new content using AI and create a version
    pub async fn generate_content(
        &self,
        job_id: &str,
        component_type: &str,
        prompt: Option<String>,
        tone: Option<String>,
        user_id: &str,
    ) -> Result<ContentVersion, ApiError> {
        // Validate component type
        let comp_type = ContentComponentType::from_str(component_type)
            .ok_or_else(|| ApiError::BadRequest(format!("Invalid component type: {}", component_type)))?;

        // Verify job exists and get current content for context
        let job = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
            "SELECT title, description, requirements, benefits, company, company_id FROM jobs WHERE id = ?"
        )
        .bind(job_id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

        let (title, description, requirements, benefits, company, company_id) = job;

        // Get company info if available
        let company_info = if let Some(cid) = &company_id {
            sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
                "SELECT name, industry, description FROM companies WHERE id = ?"
            )
            .bind(cid)
            .fetch_optional(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?
        } else {
            None
        };

        let company_name = company_info.as_ref().map(|c| c.0.clone()).or(company);
        let company_industry = company_info.as_ref().and_then(|c| c.1.clone());

        // Build the AI prompt based on component type
        let ai_prompt = self.build_prompt(
            comp_type,
            &title,
            description.as_deref(),
            requirements.as_deref(),
            benefits.as_deref(),
            company_name.as_deref(),
            company_industry.as_deref(),
            prompt.as_deref(),
            tone.as_deref(),
        );

        // Generate content using OpenAI
        let generated_content = self.openai_service
            .generate_text(
                crate::services::openai::TextGenerationPurpose::JobDescriptionGeneration,
                &ai_prompt,
                None,
            )
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to generate content with AI");
                ApiError::ServiceUnavailable(format!("AI service error: {}", e))
            })?;

        // Process the generated content based on type
        let processed_content = self.process_generated_content(comp_type, &generated_content);

        // Get next version number
        let next_version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM job_content_versions WHERE job_id = ? AND component_type = ?"
        )
        .bind(job_id)
        .bind(component_type)
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Deactivate all existing versions for this component
        sqlx::query(
            "UPDATE job_content_versions SET is_active = 0 WHERE job_id = ? AND component_type = ?"
        )
        .bind(job_id)
        .bind(component_type)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Create new version (active by default)
        let version_id = generate_content_version_id();
        let now = chrono::Utc::now().to_rfc3339();
        let prompt_used = prompt.unwrap_or_else(|| format!("Auto-generated {} with tone: {}", component_type, tone.as_deref().unwrap_or("professional")));

        sqlx::query(
            r#"
            INSERT INTO job_content_versions (id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at)
            VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?)
            "#,
        )
        .bind(&version_id)
        .bind(job_id)
        .bind(component_type)
        .bind(&processed_content)
        .bind(&prompt_used)
        .bind(next_version)
        .bind(user_id)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Sync to job table
        self.sync_to_job(job_id, component_type, &processed_content).await?;

        // Cleanup old versions
        self.cleanup_old_versions(job_id, component_type).await?;

        info!(
            job_id = %job_id,
            component_type = %component_type,
            version_id = %version_id,
            version_number = next_version,
            "Generated new content version"
        );

        // Return the created version
        let version = sqlx::query_as::<_, ContentVersion>(
            "SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at FROM job_content_versions WHERE id = ?"
        )
        .bind(&version_id)
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(version)
    }

    /// Activate a specific version
    pub async fn activate_version(
        &self,
        job_id: &str,
        component_type: &str,
        version_id: &str,
    ) -> Result<ContentVersion, ApiError> {
        // Validate component type
        ContentComponentType::from_str(component_type)
            .ok_or_else(|| ApiError::BadRequest(format!("Invalid component type: {}", component_type)))?;

        // Get the version to activate
        let version = sqlx::query_as::<_, ContentVersion>(
            "SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at FROM job_content_versions WHERE id = ? AND job_id = ? AND component_type = ?"
        )
        .bind(version_id)
        .bind(job_id)
        .bind(component_type)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::NotFound(format!("Version {} not found", version_id)))?;

        // Deactivate all versions for this component
        sqlx::query(
            "UPDATE job_content_versions SET is_active = 0 WHERE job_id = ? AND component_type = ?"
        )
        .bind(job_id)
        .bind(component_type)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Activate the selected version
        sqlx::query(
            "UPDATE job_content_versions SET is_active = 1 WHERE id = ?"
        )
        .bind(version_id)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        // Sync to job table
        self.sync_to_job(job_id, component_type, &version.content).await?;

        info!(
            job_id = %job_id,
            component_type = %component_type,
            version_id = %version_id,
            "Activated content version"
        );

        // Return updated version
        let updated_version = sqlx::query_as::<_, ContentVersion>(
            "SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at FROM job_content_versions WHERE id = ?"
        )
        .bind(version_id)
        .fetch_one(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(updated_version)
    }

    /// Delete a version (cannot delete active version)
    pub async fn delete_version(
        &self,
        job_id: &str,
        component_type: &str,
        version_id: &str,
    ) -> Result<(), ApiError> {
        // Check if version exists and is not active
        let version = sqlx::query_as::<_, ContentVersion>(
            "SELECT id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at FROM job_content_versions WHERE id = ? AND job_id = ? AND component_type = ?"
        )
        .bind(version_id)
        .bind(job_id)
        .bind(component_type)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::NotFound(format!("Version {} not found", version_id)))?;

        if version.is_active == 1 {
            return Err(ApiError::BadRequest("Cannot delete the active version".to_string()));
        }

        sqlx::query("DELETE FROM job_content_versions WHERE id = ?")
            .bind(version_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        info!(
            job_id = %job_id,
            component_type = %component_type,
            version_id = %version_id,
            "Deleted content version"
        );

        Ok(())
    }

    /// Sync content to the job table
    async fn sync_to_job(
        &self,
        job_id: &str,
        component_type: &str,
        content: &str,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().to_rfc3339();

        let query = match component_type {
            "title" => "UPDATE jobs SET title = ?, updated_at = ? WHERE id = ?",
            "summary" => "UPDATE jobs SET summary = ?, updated_at = ? WHERE id = ?",
            "description" => "UPDATE jobs SET description = ?, updated_at = ? WHERE id = ?",
            "requirements" => "UPDATE jobs SET requirements = ?, updated_at = ? WHERE id = ?",
            "benefits" => "UPDATE jobs SET benefits = ?, updated_at = ? WHERE id = ?",
            "image" => "UPDATE jobs SET job_image_url = ?, updated_at = ? WHERE id = ?",
            _ => return Err(ApiError::BadRequest(format!("Invalid component type: {}", component_type))),
        };

        sqlx::query(query)
            .bind(content)
            .bind(&now)
            .bind(job_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        Ok(())
    }

    /// Cleanup old versions, keeping only MAX_VERSIONS_PER_COMPONENT
    async fn cleanup_old_versions(
        &self,
        job_id: &str,
        component_type: &str,
    ) -> Result<(), ApiError> {
        // Delete versions beyond the limit, but never delete active version
        sqlx::query(
            r#"
            DELETE FROM job_content_versions
            WHERE job_id = ? AND component_type = ? AND is_active = 0
            AND id NOT IN (
                SELECT id FROM job_content_versions
                WHERE job_id = ? AND component_type = ?
                ORDER BY version_number DESC
                LIMIT ?
            )
            "#,
        )
        .bind(job_id)
        .bind(component_type)
        .bind(job_id)
        .bind(component_type)
        .bind(MAX_VERSIONS_PER_COMPONENT)
        .execute(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(())
    }

    /// Build AI prompt based on component type
    fn build_prompt(
        &self,
        component_type: ContentComponentType,
        title: &str,
        description: Option<&str>,
        requirements: Option<&str>,
        benefits: Option<&str>,
        company_name: Option<&str>,
        company_industry: Option<&str>,
        custom_prompt: Option<&str>,
        tone: Option<&str>,
    ) -> String {
        let tone_str = tone.unwrap_or("professional");
        let company = company_name.unwrap_or("our company");
        let industry = company_industry.unwrap_or("technology");

        match component_type {
            ContentComponentType::Title => {
                let base = format!(
                    "Generate a compelling job title for a position currently titled '{}'.\n\
                    Company: {}\n\
                    Industry: {}\n\
                    Tone: {}\n",
                    title, company, industry, tone_str
                );
                if let Some(prompt) = custom_prompt {
                    format!("{}\nAdditional instructions: {}\n\nReturn only the job title, no quotes or extra text.", base, prompt)
                } else {
                    format!("{}\nReturn only the job title, no quotes or extra text.", base)
                }
            }
            ContentComponentType::Summary => {
                let desc = description.unwrap_or("No description available");
                let reqs = requirements.unwrap_or("[]");
                let bens = benefits.unwrap_or("[]");
                let base = format!(
                    "Generate an engaging job summary for '{}' at {}.\n\
                    Industry: {}\n\
                    Full description: {}\n\
                    Requirements: {}\n\
                    Benefits: {}\n\
                    Tone: {}\n\n\
                    Create a 3-5 sentence summary that captures the essence of this role.\n\
                    Focus on what makes this opportunity exciting and unique.\n\
                    Include key responsibilities and what the candidate will do.\n\
                    Aim for 300-400 characters. No markdown, just plain text. Do not truncate with '...'.",
                    title, company, industry, desc, reqs, bens, tone_str
                );
                if let Some(prompt) = custom_prompt {
                    format!("{}\n\nAdditional instructions: {}", base, prompt)
                } else {
                    base
                }
            }
            ContentComponentType::Description => {
                let current = description.unwrap_or("No current description");
                let base = format!(
                    "Write a job description for '{}' at {}.\n\
                    Industry: {}\n\
                    Current description: {}\n\
                    Tone: {}\n\n\
                    Structure with markdown:\n\
                    ## Overview\n2-3 sentences about the role and impact.\n\n\
                    ## Key Responsibilities\n5-6 bullet points, each under 15 words.\n\n\
                    ## What You'll Bring\n4-5 must-have qualifications.\n\n\
                    Be specific and direct. No corporate jargon. Keep it under 500 words.",
                    title, company, industry, current, tone_str
                );
                if let Some(prompt) = custom_prompt {
                    format!("{}\n\nAdditional instructions: {}", base, prompt)
                } else {
                    base
                }
            }
            ContentComponentType::Requirements => {
                let current = requirements.unwrap_or("[]");
                let base = format!(
                    "Generate 4-6 job requirements for '{}' at {}.\n\
                    Industry: {}\n\
                    Current requirements: {}\n\
                    Tone: {}\n\n\
                    Return ONLY a JSON array of strings. Keep each requirement under 10 words.\n\
                    Example: [\"5+ years experience in software development\", \"Bachelor's degree in CS or related field\"]",
                    title, company, industry, current, tone_str
                );
                if let Some(prompt) = custom_prompt {
                    format!("{}\n\nAdditional instructions: {}", base, prompt)
                } else {
                    base
                }
            }
            ContentComponentType::Benefits => {
                let current = benefits.unwrap_or("[]");
                let base = format!(
                    "Generate 4-6 job benefits for '{}' at {}.\n\
                    Industry: {}\n\
                    Current benefits: {}\n\
                    Tone: {}\n\n\
                    Return ONLY a JSON array of strings. Keep each benefit under 8 words.\n\
                    Example: [\"Competitive salary and equity\", \"Health insurance\", \"Flexible work hours\"]",
                    title, company, industry, current, tone_str
                );
                if let Some(prompt) = custom_prompt {
                    format!("{}\n\nAdditional instructions: {}", base, prompt)
                } else {
                    base
                }
            }
            ContentComponentType::Image => {
                // Image generation is handled separately via the images handler
                // This case should not be reached through the normal content generation flow
                "Image generation not supported through this endpoint".to_string()
            }
        }
    }

    /// Process generated content based on component type
    fn process_generated_content(&self, component_type: ContentComponentType, content: &str) -> String {
        match component_type {
            ContentComponentType::Title => {
                // Clean up title - remove quotes, trim
                content.trim().trim_matches('"').trim().to_string()
            }
            ContentComponentType::Summary => {
                // Clean up summary - remove quotes, trim (no length limit)
                content.trim().trim_matches('"').trim().to_string()
            }
            ContentComponentType::Description => {
                // Keep markdown as-is
                content.trim().to_string()
            }
            ContentComponentType::Requirements | ContentComponentType::Benefits => {
                // Try to parse as JSON array, or extract from text
                if let Ok(_) = serde_json::from_str::<Vec<String>>(content) {
                    content.trim().to_string()
                } else {
                    // Try to find JSON array in the response
                    if let Some(start) = content.find('[') {
                        if let Some(end) = content.rfind(']') {
                            let json_str = &content[start..=end];
                            if serde_json::from_str::<Vec<String>>(json_str).is_ok() {
                                return json_str.to_string();
                            }
                        }
                    }
                    // Fallback: wrap as single-item array
                    serde_json::to_string(&vec![content.trim()]).unwrap_or_else(|_| "[]".to_string())
                }
            }
            ContentComponentType::Image => {
                // Image URLs are stored as-is
                content.trim().to_string()
            }
        }
    }
}
