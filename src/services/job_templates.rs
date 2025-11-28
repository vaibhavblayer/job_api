// src/job_templates_service.rs
use crate::common::ApiError;
use crate::jobs::models::{
    AITemplateContext, CreateAITemplateRequest, CreateJobTemplateRequest, JobTemplate,
    UpdateJobTemplateRequest,
};
use sqlx::SqlitePool;
use tracing::info;

use crate::common::generate_template_id;

pub struct JobTemplatesService {
    db: SqlitePool,
}

impl JobTemplatesService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Initialize system templates if they don't exist
    pub async fn initialize_system_templates(&self) -> Result<(), ApiError> {
        info!("Initializing system job templates...");

        let templates = vec![
            self.create_physics_faculty_template(),
            self.create_chemistry_faculty_template(),
            self.create_biology_faculty_template(),
            self.create_math_faculty_template(),
            self.create_academic_officer_template(),
        ];

        for template_data in templates {
            // Check if template already exists
            let existing = sqlx::query_as::<_, JobTemplate>(
                r#"
                SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
                FROM job_templates
                WHERE name = ? AND template_type = 'system'
                "#,
            )
            .bind(&template_data.0)
            .fetch_optional(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

            if existing.is_none() {
                let template_id = generate_template_id();
                let now = chrono::Utc::now().to_rfc3339();

                sqlx::query(
                    r#"
                    INSERT INTO job_templates (id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at)
                    VALUES (?, ?, NULL, 'system', ?, NULL, NULL, ?, ?)
                    "#,
                )
                .bind(&template_id)
                .bind(&template_data.0)
                .bind(&template_data.1)
                .bind(&now)
                .bind(&now)
                .execute(&self.db)
                .await
                .map_err(ApiError::DatabaseError)?;

                info!("Created system template: {}", template_data.0);
            }
        }

        Ok(())
    }

    // ============================================================================
    // Template CRUD Operations
    // ============================================================================

    /// Get all templates (system + custom + ai)
    pub async fn get_all_templates(&self) -> Result<Vec<JobTemplate>, ApiError> {
        let templates = sqlx::query_as::<_, JobTemplate>(
            r#"
            SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
            FROM job_templates
            ORDER BY 
                CASE template_type 
                    WHEN 'system' THEN 0 
                    WHEN 'custom' THEN 1
                    ELSE 2 
                END,
                name ASC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(templates)
    }

    /// Get templates by type
    pub async fn get_templates_by_type(
        &self,
        template_type: &str,
    ) -> Result<Vec<JobTemplate>, ApiError> {
        if template_type != "system" && template_type != "custom" && template_type != "ai" {
            return Err(ApiError::ValidationError(
                "Template type must be 'system', 'custom', or 'ai'".to_string(),
            ));
        }

        let templates = sqlx::query_as::<_, JobTemplate>(
            r#"
            SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
            FROM job_templates
            WHERE template_type = ?
            ORDER BY name ASC
            "#,
        )
        .bind(template_type)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(templates)
    }

    /// Get templates by company (company-specific only)
    pub async fn get_templates_by_company(
        &self,
        company_id: &str,
    ) -> Result<Vec<JobTemplate>, ApiError> {
        let templates = sqlx::query_as::<_, JobTemplate>(
            r#"
            SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
            FROM job_templates
            WHERE company_id = ?
            ORDER BY 
                CASE template_type 
                    WHEN 'ai' THEN 0 
                    ELSE 1 
                END,
                name ASC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(templates)
    }

    /// Get available templates for a company (system + company-specific)
    pub async fn get_available_templates(
        &self,
        company_id: Option<&str>,
    ) -> Result<Vec<JobTemplate>, ApiError> {
        let templates = if let Some(cid) = company_id {
            sqlx::query_as::<_, JobTemplate>(
                r#"
                SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
                FROM job_templates
                WHERE template_type = 'system' OR company_id = ?
                ORDER BY 
                    CASE template_type 
                        WHEN 'system' THEN 0 
                        WHEN 'ai' THEN 1
                        ELSE 2 
                    END,
                    name ASC
                "#,
            )
            .bind(cid)
            .fetch_all(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?
        } else {
            // If no company_id, return only system templates
            sqlx::query_as::<_, JobTemplate>(
                r#"
                SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
                FROM job_templates
                WHERE template_type = 'system'
                ORDER BY name ASC
                "#,
            )
            .fetch_all(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?
        };

        Ok(templates)
    }

    /// Get template by ID
    pub async fn get_template_by_id(&self, template_id: &str) -> Result<JobTemplate, ApiError> {
        let template = sqlx::query_as::<_, JobTemplate>(
            r#"
            SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
            FROM job_templates
            WHERE id = ?
            "#,
        )
        .bind(template_id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Template not found".to_string()))?;

        Ok(template)
    }

    /// Create a new custom template
    pub async fn create_template(
        &self,
        request: CreateJobTemplateRequest,
        user_id: &str,
    ) -> Result<JobTemplate, ApiError> {
        // Validate request - basic validation
        if request.name.trim().is_empty() {
            return Err(ApiError::ValidationError("Template name cannot be empty".to_string()));
        }

        let template_id = generate_template_id();
        let now = chrono::Utc::now().to_rfc3339();
        let job_data_json = serde_json::to_string(&request.job_data)
            .map_err(|e| ApiError::ValidationError(format!("Invalid job data: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO job_templates (id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at)
            VALUES (?, ?, ?, 'custom', ?, NULL, ?, ?, ?)
            "#,
        )
        .bind(&template_id)
        .bind(&request.name)
        .bind(&request.company_id)
        .bind(&job_data_json)
        .bind(user_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Template name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!(
            "Created custom template: {} ({})",
            request.name, template_id
        );

        self.get_template_by_id(&template_id).await
    }

    /// Update an existing custom or AI template
    pub async fn update_template(
        &self,
        template_id: &str,
        request: UpdateJobTemplateRequest,
        user_id: &str,
    ) -> Result<JobTemplate, ApiError> {
        // Check if template exists
        let template = self.get_template_by_id(template_id).await?;

        // Prevent updating system templates
        if template.template_type == "system" {
            return Err(ApiError::Forbidden(
                "Cannot update system templates".to_string(),
            ));
        }

        // Check if user owns the template
        if let Some(created_by) = &template.created_by {
            if created_by != user_id {
                return Err(ApiError::Forbidden(
                    "You can only update your own templates".to_string(),
                ));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(name) = &request.name {
            if name.trim().is_empty() {
                return Err(ApiError::ValidationError(
                    "Template name cannot be empty".to_string(),
                ));
            }
            updates.push("name = ?");
            params.push(name.clone());
        }

        if let Some(job_data) = &request.job_data {
            let job_data_json = serde_json::to_string(job_data)
                .map_err(|e| ApiError::ValidationError(format!("Invalid job data: {}", e)))?;
            updates.push("job_data = ?");
            params.push(job_data_json);
        }

        // Handle ai_context for AI templates
        if let Some(ai_context) = &request.ai_context {
            let ai_context_json = serde_json::to_string(ai_context)
                .map_err(|e| ApiError::ValidationError(format!("Invalid AI context: {}", e)))?;
            updates.push("ai_context = ?");
            params.push(ai_context_json);
        }

        if updates.is_empty() {
            return self.get_template_by_id(template_id).await;
        }

        updates.push("updated_at = ?");
        params.push(now.clone());
        params.push(template_id.to_string());

        let query = format!(
            "UPDATE job_templates SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        query_builder.execute(&self.db).await.map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Template name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!("Updated template: {}", template_id);

        self.get_template_by_id(template_id).await
    }

    /// Delete a custom template
    pub async fn delete_template(&self, template_id: &str, user_id: &str) -> Result<(), ApiError> {
        // Check if template exists
        let template = self.get_template_by_id(template_id).await?;

        // Prevent deleting system templates
        if template.template_type == "system" {
            return Err(ApiError::Forbidden(
                "Cannot delete system templates".to_string(),
            ));
        }

        // Check if user owns the template
        if let Some(created_by) = &template.created_by {
            if created_by != user_id {
                return Err(ApiError::Forbidden(
                    "You can only delete your own templates".to_string(),
                ));
            }
        }

        let result = sqlx::query("DELETE FROM job_templates WHERE id = ?")
            .bind(template_id)
            .execute(&self.db)
            .await
            .map_err(ApiError::DatabaseError)?;

        if result.rows_affected() == 0 {
            return Err(ApiError::BadRequest("Template not found".to_string()));
        }

        info!("Deleted template: {}", template_id);

        Ok(())
    }

    // ============================================================================
    // AI Template Operations
    // ============================================================================

    /// Create a new AI template (requires company_id)
    pub async fn create_ai_template(
        &self,
        request: CreateAITemplateRequest,
        user_id: &str,
    ) -> Result<JobTemplate, ApiError> {
        // Validate request - name cannot be empty
        if request.name.trim().is_empty() {
            return Err(ApiError::ValidationError(
                "Template name cannot be empty".to_string(),
            ));
        }

        // AI templates require company_id (validated by struct, but double-check)
        if request.company_id.trim().is_empty() {
            return Err(ApiError::ValidationError(
                "AI templates must be associated with a company".to_string(),
            ));
        }

        let template_id = generate_template_id();
        let now = chrono::Utc::now().to_rfc3339();

        // Serialize AI context to JSON
        let ai_context_json = serde_json::to_string(&request.ai_context)
            .map_err(|e| ApiError::ValidationError(format!("Invalid AI context: {}", e)))?;

        // AI templates have empty job_data (context is in ai_context)
        let empty_job_data = serde_json::json!({}).to_string();

        sqlx::query(
            r#"
            INSERT INTO job_templates (id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at)
            VALUES (?, ?, ?, 'ai', ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&template_id)
        .bind(&request.name)
        .bind(&request.company_id)
        .bind(&empty_job_data)
        .bind(&ai_context_json)
        .bind(user_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Template name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!(
            "Created AI template: {} ({}) for company {}",
            request.name, template_id, request.company_id
        );

        self.get_template_by_id(&template_id).await
    }

    /// Get AI template context by template ID
    pub async fn get_ai_template_context(
        &self,
        template_id: &str,
    ) -> Result<AITemplateContext, ApiError> {
        let template = self.get_template_by_id(template_id).await?;

        // Verify it's an AI template
        if template.template_type != "ai" {
            return Err(ApiError::BadRequest(
                "Template is not an AI template".to_string(),
            ));
        }

        // Parse the ai_context JSON
        let ai_context = template
            .ai_context
            .ok_or_else(|| ApiError::BadRequest("AI template has no context".to_string()))?;

        let context: AITemplateContext = serde_json::from_str(&ai_context)
            .map_err(|e| ApiError::ValidationError(format!("Invalid AI context format: {}", e)))?;

        Ok(context)
    }

    /// Update an AI template's context
    pub async fn update_ai_template(
        &self,
        template_id: &str,
        request: UpdateJobTemplateRequest,
        user_id: &str,
    ) -> Result<JobTemplate, ApiError> {
        // Check if template exists
        let template = self.get_template_by_id(template_id).await?;

        // Prevent updating system templates
        if template.template_type == "system" {
            return Err(ApiError::Forbidden(
                "Cannot update system templates".to_string(),
            ));
        }

        // Check if user owns the template
        if let Some(created_by) = &template.created_by {
            if created_by != user_id {
                return Err(ApiError::Forbidden(
                    "You can only update your own templates".to_string(),
                ));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Build dynamic update query
        let mut updates = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(name) = &request.name {
            if name.trim().is_empty() {
                return Err(ApiError::ValidationError(
                    "Template name cannot be empty".to_string(),
                ));
            }
            updates.push("name = ?");
            params.push(name.clone());
        }

        // Handle job_data for regular templates
        if let Some(job_data) = &request.job_data {
            let job_data_json = serde_json::to_string(job_data)
                .map_err(|e| ApiError::ValidationError(format!("Invalid job data: {}", e)))?;
            updates.push("job_data = ?");
            params.push(job_data_json);
        }

        // Handle ai_context for AI templates
        if let Some(ai_context) = &request.ai_context {
            let ai_context_json = serde_json::to_string(ai_context)
                .map_err(|e| ApiError::ValidationError(format!("Invalid AI context: {}", e)))?;
            updates.push("ai_context = ?");
            params.push(ai_context_json);
        }

        if updates.is_empty() {
            return self.get_template_by_id(template_id).await;
        }

        updates.push("updated_at = ?");
        params.push(now.clone());
        params.push(template_id.to_string());

        let query = format!(
            "UPDATE job_templates SET {} WHERE id = ?",
            updates.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        query_builder.execute(&self.db).await.map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                ApiError::ValidationError("Template name already exists".to_string())
            } else {
                ApiError::DatabaseError(e)
            }
        })?;

        info!("Updated template: {}", template_id);

        self.get_template_by_id(template_id).await
    }

    // ============================================================================
    // Job Composer Template Operations
    // ============================================================================

    /// Get templates for Job Composer (company-only, excludes system templates)
    /// Returns templates grouped by type (ai templates first, then custom)
    pub async fn get_job_composer_templates(
        &self,
        company_id: &str,
    ) -> Result<Vec<JobTemplate>, ApiError> {
        if company_id.trim().is_empty() {
            return Err(ApiError::ValidationError(
                "Company ID is required".to_string(),
            ));
        }

        let templates = sqlx::query_as::<_, JobTemplate>(
            r#"
            SELECT id, name, company_id, template_type, job_data, ai_context, created_by, created_at, updated_at
            FROM job_templates
            WHERE company_id = ? AND template_type != 'system'
            ORDER BY 
                CASE template_type 
                    WHEN 'ai' THEN 0 
                    ELSE 1 
                END,
                name ASC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        Ok(templates)
    }

    // ============================================================================
    // System Template Definitions
    // ============================================================================

    fn create_physics_faculty_template(&self) -> (String, String) {
        let name = "Physics Faculty for JEE/NEET".to_string();
        let job_data = serde_json::json!({
            "title": "Physics Faculty for JEE/NEET",
            "description": "We are seeking an experienced and passionate Physics Faculty to teach JEE and NEET aspirants. The ideal candidate will have a strong command of Physics concepts and the ability to simplify complex topics for students.",
            "location": "Kota, Rajasthan",
            "job_type": "Full-time",
            "experience_level": "Mid-level",
            "requirements": [
                "M.Sc. or B.Tech in Physics or related field",
                "Minimum 3 years of teaching experience for JEE/NEET",
                "Strong understanding of JEE Main, JEE Advanced, and NEET Physics syllabus",
                "Excellent communication and presentation skills",
                "Ability to motivate and inspire students"
            ],
            "benefits": [
                "Competitive salary package",
                "Performance-based incentives",
                "Professional development opportunities",
                "Collaborative work environment",
                "Health insurance"
            ],
            "educational_qualifications": [
                {
                    "degree_level": "Post-Graduate",
                    "preferred_iitian": true,
                    "preferred_institutions": "IIT, NIT, or equivalent institutions",
                    "additional_notes": "Candidates with IIT background preferred"
                }
            ]
        });
        (name, job_data.to_string())
    }

    fn create_chemistry_faculty_template(&self) -> (String, String) {
        let name = "Chemistry Faculty for JEE/NEET".to_string();
        let job_data = serde_json::json!({
            "title": "Chemistry Faculty for JEE/NEET",
            "description": "We are looking for a dedicated Chemistry Faculty to teach JEE and NEET aspirants. The candidate should have expertise in Organic, Inorganic, and Physical Chemistry with proven teaching experience.",
            "location": "Kota, Rajasthan",
            "job_type": "Full-time",
            "experience_level": "Mid-level",
            "requirements": [
                "M.Sc. in Chemistry or related field",
                "Minimum 3 years of teaching experience for JEE/NEET",
                "Expertise in Organic, Inorganic, and Physical Chemistry",
                "Strong problem-solving and analytical skills",
                "Ability to create engaging learning materials"
            ],
            "benefits": [
                "Competitive salary package",
                "Performance-based bonuses",
                "Professional growth opportunities",
                "Modern teaching facilities",
                "Health and wellness benefits"
            ],
            "educational_qualifications": [
                {
                    "degree_level": "Post-Graduate",
                    "preferred_iitian": true,
                    "preferred_institutions": "IIT, NIT, or premier institutions",
                    "additional_notes": "Strong academic background required"
                }
            ]
        });
        (name, job_data.to_string())
    }

    fn create_biology_faculty_template(&self) -> (String, String) {
        let name = "Biology Faculty for JEE/NEET".to_string();
        let job_data = serde_json::json!({
            "title": "Biology Faculty for NEET",
            "description": "We are seeking an enthusiastic Biology Faculty to teach NEET aspirants. The ideal candidate will have comprehensive knowledge of Botany and Zoology with a track record of producing successful results.",
            "location": "Kota, Rajasthan",
            "job_type": "Full-time",
            "experience_level": "Mid-level",
            "requirements": [
                "M.Sc. in Biology, Botany, or Zoology",
                "Minimum 3 years of teaching experience for NEET",
                "In-depth knowledge of NEET Biology syllabus",
                "Ability to explain complex biological concepts clearly",
                "Experience with modern teaching methodologies"
            ],
            "benefits": [
                "Attractive compensation package",
                "Result-based incentives",
                "Career advancement opportunities",
                "Supportive teaching environment",
                "Medical insurance coverage"
            ],
            "educational_qualifications": [
                {
                    "degree_level": "Post-Graduate",
                    "preferred_iitian": false,
                    "preferred_institutions": "Top universities with strong Biology programs",
                    "additional_notes": "MBBS graduates also encouraged to apply"
                }
            ]
        });
        (name, job_data.to_string())
    }

    fn create_math_faculty_template(&self) -> (String, String) {
        let name = "Mathematics Faculty for JEE/NEET".to_string();
        let job_data = serde_json::json!({
            "title": "Mathematics Faculty for JEE",
            "description": "We are hiring a skilled Mathematics Faculty to teach JEE aspirants. The candidate should have strong mathematical aptitude and the ability to teach advanced concepts effectively.",
            "location": "Kota, Rajasthan",
            "job_type": "Full-time",
            "experience_level": "Mid-level",
            "requirements": [
                "M.Sc. or B.Tech in Mathematics or related field",
                "Minimum 3 years of teaching experience for JEE",
                "Expertise in Calculus, Algebra, Trigonometry, and Coordinate Geometry",
                "Strong analytical and problem-solving skills",
                "Passion for teaching and mentoring students"
            ],
            "benefits": [
                "Competitive salary structure",
                "Performance incentives",
                "Professional development programs",
                "Collaborative faculty team",
                "Comprehensive health benefits"
            ],
            "educational_qualifications": [
                {
                    "degree_level": "Post-Graduate",
                    "preferred_iitian": true,
                    "preferred_institutions": "IIT, ISI, or top mathematical institutions",
                    "additional_notes": "Candidates with strong JEE background preferred"
                }
            ]
        });
        (name, job_data.to_string())
    }

    fn create_academic_officer_template(&self) -> (String, String) {
        let name = "Academic Officer".to_string();
        let job_data = serde_json::json!({
            "title": "Academic Officer",
            "description": "We are looking for an organized and detail-oriented Academic Officer to manage academic operations, coordinate with faculty, and ensure smooth functioning of academic programs.",
            "location": "Kota, Rajasthan",
            "job_type": "Full-time",
            "experience_level": "Entry-level",
            "requirements": [
                "Bachelor's degree in any discipline",
                "1-2 years of experience in academic administration",
                "Strong organizational and multitasking skills",
                "Excellent communication and interpersonal abilities",
                "Proficiency in MS Office and academic management software"
            ],
            "benefits": [
                "Competitive salary",
                "Regular working hours",
                "Growth opportunities in education sector",
                "Friendly work environment",
                "Health insurance"
            ],
            "educational_qualifications": [
                {
                    "degree_level": "Graduate",
                    "preferred_iitian": false,
                    "preferred_institutions": "Any recognized university",
                    "additional_notes": "MBA or education-related qualifications are a plus"
                }
            ]
        });
        (name, job_data.to_string())
    }
}
