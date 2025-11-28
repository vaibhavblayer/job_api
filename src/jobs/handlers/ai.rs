// src/jobs/handlers/ai.rs
//! AI-powered job content generation handlers

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::auth::AuthedUser;
use crate::common::error::ApiError;
use crate::common::state::AppState;
use crate::companies::services::CompaniesService;
use crate::services::job_templates::JobTemplatesService;
use crate::services::openai::{ImageStyle, SocialPlatform, TextGenerationPurpose};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateJobDescriptionRequest {
    pub job_title: String,
    pub experience_level: Option<String>,
    pub company_name: Option<String>,
    pub industry: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateJobBenefitsRequest {
    pub job_title: String,
    pub level: Option<String>,
    pub company_name: Option<String>,
    pub company_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateJobRequirementsRequest {
    pub job_title: String,
    pub seniority_level: Option<String>,
    pub industry: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuggestSkillsRequest {
    pub job_title: String,
    pub industry: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeBiasRequest {
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct ReadabilityScoreRequest {
    pub text: String,
}


#[derive(Debug, Deserialize)]
pub struct GenerateSocialPostRequest {
    pub job_id: String,
    pub platform: String,
    pub style: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AIGenerationResponse {
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AIGenerationMetadata>,
}

#[derive(Debug, Serialize)]
pub struct AIGenerationMetadata {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_time_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SocialPostResponse {
    pub image_url: String,
    pub download_url: String,
    pub platform: String,
    pub dimensions: SocialPostDimensions,
}

#[derive(Debug, Serialize)]
pub struct SocialPostDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct ReadabilityScoreResponse {
    pub score: f32,
    pub level: String,
    pub suggestions: Vec<String>,
}

// ============================================================================
// Generate from AI Template Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateFromTemplateRequest {
    pub template_id: String,
    pub company_id: String,
    pub job_title: Option<String>, // Override if provided
}

#[derive(Debug, Serialize)]
pub struct GenerateFromTemplateResponse {
    pub title: Option<String>,
    pub description: Option<String>,
    pub requirements: Option<Vec<String>>,
    pub benefits: Option<Vec<String>>,
    pub educational_qualifications: Option<Vec<EducationalQualification>>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EducationalQualification {
    pub degree_level: Option<String>,
    pub preferred_iitian: Option<bool>,
    pub preferred_institutions: Option<String>,
    pub additional_notes: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Generate job description using AI
/// POST /api/admin/jobs/ai/generate-description
pub async fn generate_job_description(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateJobDescriptionRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    info!(job_title = %req.job_title, "Generating job description with AI");
    let state = state_lock.read().await;

    let mut context = serde_json::json!({
        "job_title": req.job_title,
    });

    if let Some(level) = &req.experience_level {
        context["experience_level"] = serde_json::json!(level);
    }
    if let Some(company) = &req.company_name {
        context["company_name"] = serde_json::json!(company);
    }
    if let Some(industry) = &req.industry {
        context["industry"] = serde_json::json!(industry);
    }
    if let Some(location) = &req.location {
        context["location"] = serde_json::json!(location);
    }

    let prompt = format!(
        "Generate a comprehensive and engaging job description for the position of '{}'. \
        Include sections for: Overview, Key Responsibilities, and What We Offer. \
        Make it professional yet approachable.",
        req.job_title
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            Some(context),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate job description");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    Ok(Json(AIGenerationResponse {
        content: serde_json::json!(result),
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}


/// Generate job benefits using AI
/// POST /api/admin/jobs/ai/generate-benefits
/// If company_id is provided and company has benefits, returns those directly
pub async fn generate_job_benefits(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateJobBenefitsRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    info!(job_title = %req.job_title, "Generating job benefits with AI");
    let state = state_lock.read().await;

    // Try to get benefits from company first
    if let Some(company_id) = &req.company_id {
        let company_benefits: Option<String> = sqlx::query_scalar(
            "SELECT benefits FROM companies WHERE id = ?"
        )
        .bind(company_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::InternalServer(format!("Database error: {}", e)))?
        .flatten();

        if let Some(benefits_json) = company_benefits {
            if let Ok(benefits) = serde_json::from_str::<Vec<String>>(&benefits_json) {
                if !benefits.is_empty() {
                    info!(company_id = %company_id, "Using company benefits");
                    return Ok(Json(AIGenerationResponse {
                        content: serde_json::json!(benefits),
                        metadata: Some(AIGenerationMetadata {
                            model: "company-data".to_string(),
                            tokens_used: None,
                            generation_time_ms: None,
                        }),
                    }));
                }
            }
        }
    }

    // Fallback to AI generation if no company benefits
    let mut context = serde_json::json!({
        "job_title": req.job_title,
    });

    if let Some(level) = &req.level {
        context["level"] = serde_json::json!(level);
    }
    if let Some(company) = &req.company_name {
        context["company_name"] = serde_json::json!(company);
    }

    let prompt = format!(
        "Generate 4-5 key job benefits for a '{}' position. \
        Keep each benefit brief (under 8 words). \
        Return as a JSON array of strings.",
        req.job_title
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            Some(context),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate job benefits");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    let content = serde_json::from_str::<Vec<String>>(&result)
        .map(|v| serde_json::json!(v))
        .unwrap_or_else(|_| serde_json::json!(result));

    Ok(Json(AIGenerationResponse {
        content,
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}

/// Generate job requirements using AI
/// POST /api/admin/jobs/ai/generate-requirements
pub async fn generate_job_requirements(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateJobRequirementsRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    info!(job_title = %req.job_title, "Generating job requirements with AI");
    let state = state_lock.read().await;

    let mut context = serde_json::json!({
        "job_title": req.job_title,
    });

    if let Some(level) = &req.seniority_level {
        context["seniority_level"] = serde_json::json!(level);
    }
    if let Some(industry) = &req.industry {
        context["industry"] = serde_json::json!(industry);
    }

    let prompt = format!(
        "Generate job requirements for a '{}' position. \
        Return a JSON object with 'required' (3-4 must-haves) and 'preferred' (2-3 nice-to-haves). \
        Keep each item concise (under 10 words).",
        req.job_title
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            Some(context),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate job requirements");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    let content = serde_json::from_str::<serde_json::Value>(&result)
        .unwrap_or_else(|_| serde_json::json!({ "required": [], "preferred": [], "raw": result }));

    Ok(Json(AIGenerationResponse {
        content,
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}


/// Suggest skills for a job using AI
/// POST /api/admin/jobs/ai/suggest-skills
pub async fn suggest_skills(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<SuggestSkillsRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    info!(job_title = %req.job_title, "Suggesting skills with AI");
    let state = state_lock.read().await;

    let mut context = serde_json::json!({
        "job_title": req.job_title,
    });

    if let Some(industry) = &req.industry {
        context["industry"] = serde_json::json!(industry);
    }

    let prompt = format!(
        "Suggest 10-15 relevant skills for a '{}' position. \
        Include both technical and soft skills. \
        Return as a JSON array of strings.",
        req.job_title
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            Some(context),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to suggest skills");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    let content = serde_json::from_str::<Vec<String>>(&result)
        .map(|v| serde_json::json!(v))
        .unwrap_or_else(|_| serde_json::json!(result));

    Ok(Json(AIGenerationResponse {
        content,
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}

/// Analyze job description for bias
/// POST /api/admin/jobs/ai/analyze-bias
pub async fn analyze_bias(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<AnalyzeBiasRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    debug!("Analyzing job description for bias");
    let state = state_lock.read().await;

    let prompt = format!(
        "Analyze the following job description for potential bias (gender, age, cultural, etc.). \
        Provide specific suggestions for more inclusive language. \
        Return a JSON object with 'issues' array and 'suggestions' array.\n\n\
        Job Description:\n{}",
        req.description
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            None,
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to analyze bias");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    let content = serde_json::from_str::<serde_json::Value>(&result)
        .unwrap_or_else(|_| serde_json::json!({ "analysis": result }));

    Ok(Json(AIGenerationResponse {
        content,
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}


/// Calculate readability score for text
/// POST /api/admin/jobs/ai/readability-score
pub async fn calculate_readability_score(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<ReadabilityScoreRequest>,
) -> Result<Json<ReadabilityScoreResponse>, ApiError> {
    debug!("Calculating readability score");
    let state = state_lock.read().await;

    let prompt = format!(
        "Analyze the readability of the following text. \
        Return a JSON object with: \
        - 'score': a number from 0-100 (higher = more readable) \
        - 'level': reading level (e.g., 'Easy', 'Moderate', 'Complex') \
        - 'suggestions': array of specific improvements\n\n\
        Text:\n{}",
        req.text
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &prompt,
            None,
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to calculate readability");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result) {
        let score = parsed["score"].as_f64().unwrap_or(70.0) as f32;
        let level = parsed["level"].as_str().unwrap_or("Moderate").to_string();
        let suggestions = parsed["suggestions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Json(ReadabilityScoreResponse {
            score,
            level,
            suggestions,
        }))
    } else {
        Ok(Json(ReadabilityScoreResponse {
            score: 70.0,
            level: "Moderate".to_string(),
            suggestions: vec!["Unable to parse detailed analysis".to_string()],
        }))
    }
}

/// Generate social media post image for a job
/// POST /api/admin/jobs/ai/generate-social-post
pub async fn generate_social_post(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateSocialPostRequest>,
) -> Result<Json<SocialPostResponse>, ApiError> {
    info!(job_id = %req.job_id, platform = %req.platform, "Generating social media post");
    let state = state_lock.read().await;

    // Fetch job details from database
    let job = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>)>(
        "SELECT title, company_id, location, salary_min, salary_max FROM jobs WHERE id = ?"
    )
    .bind(&req.job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::InternalServer(format!("Database error: {}", e)))?
    .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", req.job_id)))?;

    let (job_title, company_id, location, salary_min, salary_max) = job;

    // Fetch company name
    let company_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM companies WHERE id = ?"
    )
    .bind(&company_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::InternalServer(format!("Database error: {}", e)))?
    .unwrap_or_else(|| "Company".to_string());

    // Format salary range if available
    let salary_range = match (salary_min, salary_max) {
        (Some(min), Some(max)) => Some(format!("${} - ${}", min, max)),
        (Some(min), None) => Some(format!("From ${}", min)),
        (None, Some(max)) => Some(format!("Up to ${}", max)),
        _ => None,
    };

    // Parse platform
    let platform = match req.platform.as_str() {
        "instagram_square" => SocialPlatform::InstagramSquare,
        "instagram_story" => SocialPlatform::InstagramStory,
        "linkedin_post" => SocialPlatform::LinkedIn,
        "twitter_post" => SocialPlatform::Twitter,
        "facebook_post" => SocialPlatform::Facebook,
        _ => SocialPlatform::LinkedIn,
    };

    // Parse style
    let style = match req.style.as_deref() {
        Some("modern") => ImageStyle::Modern,
        Some("creative") => ImageStyle::Creative,
        Some("minimalist") => ImageStyle::Minimalist,
        Some("vibrant") => ImageStyle::Vibrant,
        _ => ImageStyle::Professional,
    };

    let dimensions = platform.to_dimensions();

    // Generate the image
    let image_url = state
        .openai_service
        .generate_social_media_post(
            &job_title,
            &company_name,
            location.as_deref(),
            salary_range.as_deref(),
            platform,
            style,
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate social post image");
            ApiError::ServiceUnavailable(format!("AI image generation error: {}", e))
        })?;

    Ok(Json(SocialPostResponse {
        image_url: image_url.clone(),
        download_url: image_url,
        platform: req.platform,
        dimensions: SocialPostDimensions {
            width: dimensions.0,
            height: dimensions.1,
        },
    }))
}


// ============================================================================
// Generate All Job Content
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateAllJobContentRequest {
    pub job_title: String,
    pub experience_level: Option<String>,
    pub company_id: Option<String>,
    pub company_name: Option<String>,
    pub company_industry: Option<String>,
    pub company_description: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenerateAllJobContentResponse {
    pub description: Option<String>,
    pub requirements: Option<Vec<String>>,
    pub benefits: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub errors: Vec<String>,
}

/// Generate all job content fields at once
/// POST /api/admin/jobs/ai/generate-all
pub async fn generate_all_job_content(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateAllJobContentRequest>,
) -> Result<Json<GenerateAllJobContentResponse>, ApiError> {
    info!(job_title = %req.job_title, "Generating all job content with AI");
    let state = state_lock.read().await;

    let mut response = GenerateAllJobContentResponse {
        description: None,
        requirements: None,
        benefits: None,
        skills: None,
        errors: Vec::new(),
    };

    // Build context with company info
    let context = serde_json::json!({
        "job_title": req.job_title,
        "experience_level": req.experience_level,
        "company_name": req.company_name,
        "company_industry": req.company_industry,
        "company_description": req.company_description,
        "location": req.location,
    });

    // Generate description
    let desc_prompt = format!(
        "Generate a comprehensive job description for '{}' at {}. \
        Include: Overview, Key Responsibilities, and What We Offer. \
        Make it professional and engaging. \
        Company industry: {}. Location: {}.",
        req.job_title,
        req.company_name.as_deref().unwrap_or("a leading company"),
        req.company_industry.as_deref().unwrap_or("technology"),
        req.location.as_deref().unwrap_or("flexible")
    );

    match state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &desc_prompt,
            Some(context.clone()),
        )
        .await
    {
        Ok(desc) => response.description = Some(desc),
        Err(e) => response.errors.push(format!("Description: {}", e)),
    }

    // Generate requirements
    let req_prompt = format!(
        "Generate 4-5 job requirements for '{}' ({} level). \
        Return ONLY a JSON array. Keep each under 10 words. \
        Example: [\"5+ years experience\", \"Bachelor's degree\"]",
        req.job_title,
        req.experience_level.as_deref().unwrap_or("mid")
    );

    match state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &req_prompt,
            None,
        )
        .await
    {
        Ok(reqs) => {
            response.requirements = serde_json::from_str::<Vec<String>>(&reqs)
                .ok()
                .or_else(|| extract_list_from_text(&reqs));
        }
        Err(e) => response.errors.push(format!("Requirements: {}", e)),
    }

    // Generate benefits - try company benefits first
    let mut benefits_from_company = false;
    if let Some(company_id) = &req.company_id {
        let company_benefits: Option<String> = sqlx::query_scalar(
            "SELECT benefits FROM companies WHERE id = ?"
        )
        .bind(company_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten();

        if let Some(benefits_json) = company_benefits {
            if let Ok(benefits) = serde_json::from_str::<Vec<String>>(&benefits_json) {
                if !benefits.is_empty() {
                    response.benefits = Some(benefits);
                    benefits_from_company = true;
                }
            }
        }
    }

    // Fallback to AI if no company benefits
    if !benefits_from_company {
        let benefits_prompt = format!(
            "Generate 4-5 job benefits for '{}' at {}. \
            Return ONLY a JSON array. Keep each under 8 words. \
            Example: [\"Health insurance\", \"401k matching\"]",
            req.job_title,
            req.company_name.as_deref().unwrap_or("our company")
        );

        match state
            .openai_service
            .generate_text(
                TextGenerationPurpose::JobDescriptionGeneration,
                &benefits_prompt,
                None,
            )
            .await
        {
            Ok(benefits) => {
                response.benefits = serde_json::from_str::<Vec<String>>(&benefits)
                    .ok()
                    .or_else(|| extract_list_from_text(&benefits));
            }
            Err(e) => response.errors.push(format!("Benefits: {}", e)),
        }
    }

    // Suggest skills
    let skills_prompt = format!(
        "Suggest 10-12 relevant skills for '{}'. \
        Include technical and soft skills. \
        Return ONLY a JSON array of skill strings. \
        Example: [\"Python\", \"Communication\", \"Problem Solving\"]",
        req.job_title
    );

    match state
        .openai_service
        .generate_text(
            TextGenerationPurpose::JobDescriptionGeneration,
            &skills_prompt,
            None,
        )
        .await
    {
        Ok(skills) => {
            response.skills = serde_json::from_str::<Vec<String>>(&skills)
                .ok()
                .or_else(|| extract_list_from_text(&skills));
        }
        Err(e) => response.errors.push(format!("Skills: {}", e)),
    }

    Ok(Json(response))
}

/// Helper to extract list items from text if JSON parsing fails
fn extract_list_from_text(text: &str) -> Option<Vec<String>> {
    // Try to find JSON array in text
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(&text[start..=end]) {
                return Some(arr);
            }
        }
    }
    
    // Fallback: split by newlines and clean up
    let items: Vec<String> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| l.starts_with('-') || l.starts_with('•') || l.starts_with('*') || l.chars().next().map(|c| c.is_numeric()).unwrap_or(false))
        .map(|l| l.trim_start_matches(|c: char| c == '-' || c == '•' || c == '*' || c == '.' || c.is_numeric() || c.is_whitespace()))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

// ============================================================================
// Generate from AI Template
// ============================================================================

/// Generate job content from an AI template
/// POST /api/admin/jobs/ai/generate-from-template
/// 
/// This endpoint fetches the AI template context and company information,
/// combines them into a comprehensive prompt, and generates job content using AI.
pub async fn generate_from_ai_template(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(req): Json<GenerateFromTemplateRequest>,
) -> Result<Json<GenerateFromTemplateResponse>, ApiError> {
    // Verify admin access
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    info!(
        template_id = %req.template_id,
        company_id = %req.company_id,
        user_id = %authed.id,
        "Generating job content from AI template"
    );

    let state = state_lock.read().await;
    
    // Initialize services
    let templates_service = JobTemplatesService::new(state.db.clone());
    let companies_service = CompaniesService::new(state.db.clone());

    // Fetch AI template context
    let ai_context = templates_service
        .get_ai_template_context(&req.template_id)
        .await
        .map_err(|e| {
            error!(error = %e, template_id = %req.template_id, "Failed to fetch AI template context");
            e
        })?;

    // Fetch company information (gracefully handle missing data)
    let company = companies_service
        .get_company_by_id(&req.company_id)
        .await
        .ok();

    // Extract company info for context (handle missing fields gracefully per Requirement 4.3)
    let company_name = company.as_ref().map(|c| c.name.clone());
    let company_industry = company.as_ref().and_then(|c| c.industry.clone());
    let company_description = company.as_ref().and_then(|c| c.description.clone());

    // Initialize response
    let mut response = GenerateFromTemplateResponse {
        title: None,
        description: None,
        requirements: None,
        benefits: None,
        educational_qualifications: None,
        errors: Vec::new(),
    };

    // Build comprehensive context for AI generation
    let mut context = serde_json::json!({});
    
    if let Some(name) = &company_name {
        context["company_name"] = serde_json::json!(name);
    }
    if let Some(industry) = &company_industry {
        context["company_industry"] = serde_json::json!(industry);
    }
    if let Some(description) = &company_description {
        context["company_description"] = serde_json::json!(description);
    }
    if let Some(title) = &req.job_title {
        context["job_title"] = serde_json::json!(title);
    }

    // Generate title if context provided
    if let Some(title_context) = &ai_context.title_context {
        if !title_context.trim().is_empty() {
            let prompt = build_generation_prompt(
                title_context,
                &company_name,
                &company_industry,
                &company_description,
                &req.job_title,
                "job title",
            );

            match state
                .openai_service
                .generate_text(
                    TextGenerationPurpose::JobDescriptionGeneration,
                    &prompt,
                    Some(context.clone()),
                )
                .await
            {
                Ok(title) => {
                    // Clean up the title (remove quotes, trim)
                    let cleaned_title = title.trim().trim_matches('"').trim().to_string();
                    response.title = Some(cleaned_title);
                }
                Err(e) => {
                    error!(error = %e, "Failed to generate job title");
                    response.errors.push(format!("Title generation failed: {}", e));
                }
            }
        }
    }

    // Generate description if context provided
    if let Some(desc_context) = &ai_context.description_context {
        if !desc_context.trim().is_empty() {
            let prompt = build_generation_prompt(
                desc_context,
                &company_name,
                &company_industry,
                &company_description,
                &req.job_title,
                "job description",
            );

            match state
                .openai_service
                .generate_text(
                    TextGenerationPurpose::JobDescriptionGeneration,
                    &prompt,
                    Some(context.clone()),
                )
                .await
            {
                Ok(desc) => response.description = Some(desc),
                Err(e) => {
                    error!(error = %e, "Failed to generate job description");
                    response.errors.push(format!("Description generation failed: {}", e));
                }
            }
        }
    }

    // Generate requirements if context provided
    if let Some(req_context) = &ai_context.requirements_context {
        if !req_context.trim().is_empty() {
            let prompt = format!(
                "{}\n\nGenerate 4-5 job requirements. Keep each under 10 words. \
                Return ONLY a JSON array. \
                Example: [\"5+ years experience\", \"Bachelor's degree\"]",
                build_generation_prompt(
                    req_context,
                    &company_name,
                    &company_industry,
                    &company_description,
                    &req.job_title,
                    "job requirements",
                )
            );

            match state
                .openai_service
                .generate_text(
                    TextGenerationPurpose::JobDescriptionGeneration,
                    &prompt,
                    Some(context.clone()),
                )
                .await
            {
                Ok(reqs) => {
                    response.requirements = serde_json::from_str::<Vec<String>>(&reqs)
                        .ok()
                        .or_else(|| extract_list_from_text(&reqs));
                }
                Err(e) => {
                    error!(error = %e, "Failed to generate job requirements");
                    response.errors.push(format!("Requirements generation failed: {}", e));
                }
            }
        }
    }

    // Generate benefits - try company benefits first
    let mut benefits_from_company = false;
    if let Some(company_ref) = &company {
        if let Some(benefits_json) = &company_ref.benefits {
            if let Ok(benefits) = serde_json::from_str::<Vec<String>>(benefits_json) {
                if !benefits.is_empty() {
                    response.benefits = Some(benefits);
                    benefits_from_company = true;
                    info!(company_id = %req.company_id, "Using company benefits for template generation");
                }
            }
        }
    }

    // Fallback to AI generation if no company benefits and context provided
    if !benefits_from_company {
        if let Some(benefits_context) = &ai_context.benefits_context {
            if !benefits_context.trim().is_empty() {
                let prompt = format!(
                    "{}\n\nGenerate 4-5 job benefits. Keep each under 8 words. \
                    Return ONLY a JSON array. \
                    Example: [\"Health insurance\", \"401k matching\"]",
                    build_generation_prompt(
                        benefits_context,
                        &company_name,
                        &company_industry,
                        &company_description,
                        &req.job_title,
                        "job benefits",
                    )
                );

                match state
                    .openai_service
                    .generate_text(
                        TextGenerationPurpose::JobDescriptionGeneration,
                        &prompt,
                        Some(context.clone()),
                    )
                    .await
                {
                    Ok(benefits) => {
                        response.benefits = serde_json::from_str::<Vec<String>>(&benefits)
                            .ok()
                            .or_else(|| extract_list_from_text(&benefits));
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to generate job benefits");
                        response.errors.push(format!("Benefits generation failed: {}", e));
                    }
                }
            }
        }
    }

    // Generate educational qualifications if context provided
    if let Some(edu_context) = &ai_context.educational_qualifications_context {
        if !edu_context.trim().is_empty() {
            let prompt = format!(
                "{}\n\nGenerate educational qualification requirements based on the above context. \
                Return ONLY a JSON array of objects with these fields: \
                degree_level (string), preferred_iitian (boolean), preferred_institutions (string), additional_notes (string). \
                Example: [{{\"degree_level\": \"Post-Graduate\", \"preferred_iitian\": true, \"preferred_institutions\": \"IIT, NIT\", \"additional_notes\": \"PhD preferred\"}}]",
                build_generation_prompt(
                    edu_context,
                    &company_name,
                    &company_industry,
                    &company_description,
                    &req.job_title,
                    "educational qualifications",
                )
            );

            match state
                .openai_service
                .generate_text(
                    TextGenerationPurpose::JobDescriptionGeneration,
                    &prompt,
                    Some(context.clone()),
                )
                .await
            {
                Ok(edu) => {
                    response.educational_qualifications = 
                        serde_json::from_str::<Vec<EducationalQualification>>(&edu)
                            .ok()
                            .or_else(|| extract_educational_qualifications(&edu));
                }
                Err(e) => {
                    error!(error = %e, "Failed to generate educational qualifications");
                    response.errors.push(format!("Educational qualifications generation failed: {}", e));
                }
            }
        }
    }

    info!(
        template_id = %req.template_id,
        company_id = %req.company_id,
        errors_count = response.errors.len(),
        "AI template generation completed"
    );

    Ok(Json(response))
}

/// Build a comprehensive prompt combining template context with company information
fn build_generation_prompt(
    template_context: &str,
    company_name: &Option<String>,
    company_industry: &Option<String>,
    company_description: &Option<String>,
    job_title: &Option<String>,
    field_type: &str,
) -> String {
    let mut prompt_parts = vec![format!("Template Context: {}", template_context)];

    if let Some(name) = company_name {
        prompt_parts.push(format!("Company Name: {}", name));
    }
    if let Some(industry) = company_industry {
        prompt_parts.push(format!("Industry: {}", industry));
    }
    if let Some(description) = company_description {
        prompt_parts.push(format!("Company Description: {}", description));
    }
    if let Some(title) = job_title {
        prompt_parts.push(format!("Job Title: {}", title));
    }

    prompt_parts.push(format!(
        "\nBased on the above context, generate a compelling and professional {}.",
        field_type
    ));

    prompt_parts.join("\n")
}

/// Extract educational qualifications from text if JSON parsing fails
fn extract_educational_qualifications(text: &str) -> Option<Vec<EducationalQualification>> {
    // Try to find JSON array in text
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if let Ok(arr) = serde_json::from_str::<Vec<EducationalQualification>>(&text[start..=end]) {
                return Some(arr);
            }
        }
    }
    
    // If we can't parse, return None - educational qualifications need structured data
    None
}
