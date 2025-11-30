// src/jobs/handlers/images.rs
//! Job image management

use axum::{
    extract::{Extension, Multipart, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::auth::AuthedUser;
use crate::common::{generate_raw_id, ApiError, AppState};
use crate::services::openai::{ImageSize, ImageStyle};

/// POST /api/admin/jobs/upload-image - Upload job image or company logo (admin only)
pub async fn upload_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;
    let mut image_type = String::new();
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        match field.name() {
            Some("type") => {
                image_type = field.text().await.unwrap_or_default();
            }
            Some("image") => {
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| ApiError::BadRequest("Invalid file".to_string()))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let data = file_data.ok_or_else(|| ApiError::BadRequest("No image provided".to_string()))?;

    if !is_valid_image_type(&data) {
        return Err(ApiError::BadRequest("Invalid image type".to_string()));
    }

    let filename = format!("{}.png", generate_raw_id(8));

    // Check storage type setting
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let url = if storage_type.starts_with("s3") {
        // Upload to S3
        let s3_key = format!("job-images/{}/{}", image_type, filename);
        match state
            .aws_service
            .upload_file(data.to_vec(), &s3_key, "image/png")
            .await
        {
            Ok(s3_url) => {
                info!(s3_key = %s3_key, "Job image uploaded to S3 successfully");
                s3_url
            }
            Err(e) => {
                error!(error = %e, "Failed to upload job image to S3, falling back to local storage");
                // Fall back to local storage
                let dir = if image_type == "logo" {
                    &state.job_images_logos_dir
                } else {
                    &state.job_images_jobs_dir
                };
                let file_path = dir.join(&filename);
                tokio::fs::write(&file_path, &data)
                    .await
                    .map_err(|_| ApiError::InternalServer("Failed to save image".to_string()))?;
                format!("/api/job-images/{}/{}", image_type, filename)
            }
        }
    } else {
        // Save to local storage
        let dir = if image_type == "logo" {
            &state.job_images_logos_dir
        } else {
            &state.job_images_jobs_dir
        };
        let file_path = dir.join(&filename);
        tokio::fs::write(&file_path, &data)
            .await
            .map_err(|_| ApiError::InternalServer("Failed to save image".to_string()))?;
        format!("/api/job-images/{}/{}", image_type, filename)
    };

    info!(
        admin_id = %authed.id,
        image_type = %image_type,
        filename = %filename,
        "Job image uploaded"
    );

    Ok((
        StatusCode::OK,
        Json(json!({
            "url": url,
            "message": "Image uploaded successfully"
        })),
    ))
}

/// GET /api/job-images/:type/:filename - Serve job images
pub async fn serve_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Path((img_type, filename)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    let dir = if img_type == "logos" {
        &state.job_images_logos_dir
    } else {
        &state.job_images_jobs_dir
    };

    let file_path = dir.join(&filename);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("Image not found".to_string()));
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read image".to_string()))?;

    let content_type = get_content_type_from_extension(&filename);

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, content_type)],
        content,
    ))
}

/// DELETE /api/admin/jobs/images/:filename - Delete a job image (admin only)
pub async fn delete_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Try both directories
    let logo_path = state.job_images_logos_dir.join(&filename);
    let job_path = state.job_images_jobs_dir.join(&filename);

    if logo_path.exists() {
        tokio::fs::remove_file(&logo_path)
            .await
            .map_err(|_| ApiError::InternalServer("Failed to delete image".to_string()))?;
    } else if job_path.exists() {
        tokio::fs::remove_file(&job_path)
            .await
            .map_err(|_| ApiError::InternalServer("Failed to delete image".to_string()))?;
    } else {
        return Err(ApiError::BadRequest("Image not found".to_string()));
    }

    info!(
        admin_id = %authed.id,
        filename = %filename,
        "Job image deleted"
    );

    Ok(Json(json!({
        "message": "Image deleted successfully"
    })))
}

// Helper functions

fn is_valid_image_type(data: &[u8]) -> bool {
    let infer = infer::Infer::new();
    if let Some(info) = infer.get(data) {
        matches!(
            info.mime_type(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        )
    } else {
        false
    }
}

fn get_content_type_from_extension(filename: &str) -> &'static str {
    match filename.split('.').last() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

// ============================================================================
// AI Image Generation
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateJobImageRequest {
    pub job_id: String,
    pub prompt: Option<String>,
    pub style: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenerateJobImageResponse {
    pub success: bool,
    pub image_url: String,
    pub message: String,
}

/// POST /api/admin/jobs/:job_id/generate-image - Generate AI image for job
pub async fn generate_job_image(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(job_id): Path<String>,
    Json(request): Json<GenerateJobImageRequest>,
) -> Result<Json<GenerateJobImageResponse>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Get job details for context (including company_id)
    let job = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT title, description, company, location, job_type, company_id FROM jobs WHERE id = ?"
    )
    .bind(&job_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::NotFound(format!("Job {} not found", job_id)))?;

    let (title, description, company, location, job_type, company_id) = job;

    // Get company details if company_id exists
    let (company_name, company_industry, company_description) = if let Some(cid) = &company_id {
        let company_details = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT name, industry, description FROM companies WHERE id = ?"
        )
        .bind(cid)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

        if let Some((name, industry, desc)) = company_details {
            (Some(name), industry, desc)
        } else {
            (company.clone(), None, None)
        }
    } else {
        (company.clone(), None, None)
    };

    // Build the image prompt with full context
    let base_prompt = build_job_image_prompt(
        &title,
        description.as_deref(),
        company_name.as_deref(),
        company_industry.as_deref(),
        company_description.as_deref(),
        location.as_deref(),
        job_type.as_deref(),
        request.prompt.as_deref(),
    );

    // Parse style
    let style = match request.style.as_deref() {
        Some("modern") => ImageStyle::Modern,
        Some("creative") => ImageStyle::Creative,
        Some("minimalist") => ImageStyle::Minimalist,
        Some("vibrant") => ImageStyle::Vibrant,
        _ => ImageStyle::Professional,
    };

    info!(
        job_id = %job_id,
        admin_id = %authed.id,
        style = ?style,
        custom_prompt = ?request.prompt,
        "Generating AI image for job"
    );

    // Log the full prompt being sent to OpenAI
    info!(
        job_id = %job_id,
        full_prompt = %base_prompt,
        "Full image generation prompt"
    );

    // Generate image using OpenAI
    let image_url = state.openai_service
        .generate_image(&base_prompt, ImageSize::LinkedIn, style)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate job image");
            ApiError::ServiceUnavailable(format!("AI image generation failed: {}", e))
        })?;

    // Download and save the image locally
    let saved_url = download_and_save_image(&state, &image_url, &job_id).await?;

    let now = chrono::Utc::now().to_rfc3339();

    // Save as a content version for history tracking
    let version_id = crate::common::generate_content_version_id();
    let prompt_used = request.prompt.clone().unwrap_or_else(|| format!("AI generated with style: {}", request.style.as_deref().unwrap_or("professional")));

    // Get next version number
    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM job_content_versions WHERE job_id = ? AND component_type = 'image'"
    )
    .bind(&job_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Deactivate all existing image versions
    sqlx::query(
        "UPDATE job_content_versions SET is_active = 0 WHERE job_id = ? AND component_type = 'image'"
    )
    .bind(&job_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Create new version (active by default)
    sqlx::query(
        r#"
        INSERT INTO job_content_versions (id, job_id, component_type, content, prompt_used, is_active, version_number, created_by, created_at)
        VALUES (?, ?, 'image', ?, ?, 1, ?, ?, ?)
        "#,
    )
    .bind(&version_id)
    .bind(&job_id)
    .bind(&saved_url)
    .bind(&prompt_used)
    .bind(next_version)
    .bind(&authed.id)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Update job with new image URL
    sqlx::query("UPDATE jobs SET job_image_url = ?, updated_at = ? WHERE id = ?")
        .bind(&saved_url)
        .bind(&now)
        .bind(&job_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Cleanup old versions (keep last 5)
    sqlx::query(
        r#"
        DELETE FROM job_content_versions
        WHERE job_id = ? AND component_type = 'image' AND is_active = 0
        AND id NOT IN (
            SELECT id FROM job_content_versions
            WHERE job_id = ? AND component_type = 'image'
            ORDER BY version_number DESC
            LIMIT 5
        )
        "#,
    )
    .bind(&job_id)
    .bind(&job_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        job_id = %job_id,
        image_url = %saved_url,
        version_id = %version_id,
        version_number = next_version,
        "AI job image generated and saved with version history"
    );

    Ok(Json(GenerateJobImageResponse {
        success: true,
        image_url: saved_url,
        message: "Image generated successfully".to_string(),
    }))
}

/// Build a detailed prompt for job image generation
fn build_job_image_prompt(
    title: &str,
    description: Option<&str>,
    company_name: Option<&str>,
    company_industry: Option<&str>,
    company_description: Option<&str>,
    location: Option<&str>,
    job_type: Option<&str>,
    custom_prompt: Option<&str>,
) -> String {
    let mut prompt_parts = vec![];

    // Determine the work environment based on job details and company industry
    let environment = determine_work_environment(title, description, job_type, company_industry);

    prompt_parts.push(format!(
        "Create a photorealistic image of a {} work environment",
        environment
    ));

    // Add company context if available
    if let Some(company) = company_name {
        if !company.is_empty() {
            prompt_parts.push(format!("for a company called '{}'", company));
        }
    }

    // Add industry context from company
    if let Some(industry) = company_industry {
        if !industry.is_empty() {
            prompt_parts.push(format!("in the {} industry", industry));
        }
    }

    // Add context based on job type
    if let Some(jt) = job_type {
        match jt.to_lowercase().as_str() {
            "remote" => prompt_parts.push("showing a modern home office setup with natural lighting".to_string()),
            "hybrid" => prompt_parts.push("showing a flexible modern workspace".to_string()),
            _ => prompt_parts.push("showing a professional office setting".to_string()),
        }
    }

    // Add industry-specific elements (from job title/description and company industry)
    let industry_elements = get_industry_elements(title, description, company_industry, company_description);
    if !industry_elements.is_empty() {
        prompt_parts.push(format!("Include elements like: {}", industry_elements));
    }

    // Add location context if available
    if let Some(loc) = location {
        if loc.to_lowercase().contains("india") {
            prompt_parts.push("Set in an Indian professional context".to_string());
        }
    }

    // Add custom prompt FIRST if provided (higher priority)
    if let Some(custom) = custom_prompt {
        if !custom.trim().is_empty() {
            prompt_parts.insert(1, format!("IMPORTANT: {}", custom));
        }
    }

    // Style guidelines
    prompt_parts.push("Professional photography style".to_string());
    prompt_parts.push("Warm, inviting atmosphere".to_string());
    prompt_parts.push("High quality, 4K resolution".to_string());
    prompt_parts.push("No text or logos in the image".to_string());
    prompt_parts.push("Diverse, inclusive workplace representation".to_string());
    // Request landscape/wide aspect ratio for banner display
    prompt_parts.push("Wide landscape composition with 16:9 or wider aspect ratio".to_string());

    prompt_parts.join(". ")
}

/// Determine the work environment type based on job details and company industry
fn determine_work_environment(
    title: &str,
    description: Option<&str>,
    job_type: Option<&str>,
    company_industry: Option<&str>,
) -> &'static str {
    let title_lower = title.to_lowercase();
    let desc_lower = description.map(|d| d.to_lowercase()).unwrap_or_default();
    let industry_lower = company_industry.map(|i| i.to_lowercase()).unwrap_or_default();
    let combined = format!("{} {} {}", title_lower, desc_lower, industry_lower);

    // Education/Academic
    if combined.contains("teacher") || combined.contains("tutor") || combined.contains("education")
        || combined.contains("school") || combined.contains("coaching") || combined.contains("instructor")
        || combined.contains("professor") || combined.contains("faculty") || combined.contains("jee")
        || combined.contains("neet") || combined.contains("academic") || combined.contains("edtech")
    {
        return "modern classroom or educational institution";
    }

    // Healthcare
    if combined.contains("doctor") || combined.contains("nurse") || combined.contains("medical")
        || combined.contains("hospital") || combined.contains("healthcare") || combined.contains("clinic")
        || combined.contains("pharma") || combined.contains("biotech")
    {
        return "modern hospital or healthcare facility";
    }

    // Technology/Software
    if combined.contains("software") || combined.contains("developer") || combined.contains("engineer")
        || combined.contains("tech") || combined.contains("programming") || combined.contains("data")
        || combined.contains("it services") || combined.contains("saas")
    {
        return "modern tech office with computers and collaborative spaces";
    }

    // Finance
    if combined.contains("finance") || combined.contains("banking") || combined.contains("accountant")
        || combined.contains("investment") || combined.contains("fintech") || combined.contains("insurance")
    {
        return "professional financial services office";
    }

    // Creative/Design
    if combined.contains("design") || combined.contains("creative") || combined.contains("artist")
        || combined.contains("marketing") || combined.contains("content") || combined.contains("media")
        || combined.contains("advertising")
    {
        return "creative studio or design agency";
    }

    // Manufacturing/Industrial
    if combined.contains("manufacturing") || combined.contains("factory") || combined.contains("production")
        || combined.contains("industrial") || combined.contains("plant") || combined.contains("automotive")
    {
        return "modern manufacturing facility";
    }

    // Retail/Sales
    if combined.contains("sales") || combined.contains("retail") || combined.contains("store")
        || combined.contains("customer service") || combined.contains("e-commerce")
    {
        return "modern retail or customer service environment";
    }

    // Consulting
    if combined.contains("consulting") || combined.contains("advisory") || combined.contains("strategy") {
        return "professional consulting firm office";
    }

    // Default to generic office
    "professional modern office"
}

/// Get industry-specific visual elements
fn get_industry_elements(
    title: &str,
    description: Option<&str>,
    company_industry: Option<&str>,
    company_description: Option<&str>,
) -> String {
    let title_lower = title.to_lowercase();
    let desc_lower = description.map(|d| d.to_lowercase()).unwrap_or_default();
    let industry_lower = company_industry.map(|i| i.to_lowercase()).unwrap_or_default();
    let company_desc_lower = company_description.map(|d| d.to_lowercase()).unwrap_or_default();
    let combined = format!("{} {} {} {}", title_lower, desc_lower, industry_lower, company_desc_lower);

    let mut elements = vec![];

    if combined.contains("education") || combined.contains("teacher") || combined.contains("tutor")
        || combined.contains("edtech") || combined.contains("coaching")
    {
        elements.push("whiteboards");
        elements.push("books");
        elements.push("students learning");
        elements.push("digital learning tools");
    }

    if combined.contains("tech") || combined.contains("software") || combined.contains("developer")
        || combined.contains("it services") || combined.contains("saas")
    {
        elements.push("multiple monitors");
        elements.push("standing desks");
        elements.push("collaborative spaces");
        elements.push("modern tech equipment");
    }

    if combined.contains("healthcare") || combined.contains("medical") || combined.contains("pharma") {
        elements.push("medical equipment");
        elements.push("clean clinical environment");
        elements.push("healthcare professionals");
    }

    if combined.contains("creative") || combined.contains("design") || combined.contains("marketing")
        || combined.contains("advertising")
    {
        elements.push("design tools");
        elements.push("mood boards");
        elements.push("creative materials");
        elements.push("brainstorming spaces");
    }

    if combined.contains("finance") || combined.contains("banking") || combined.contains("fintech") {
        elements.push("financial charts");
        elements.push("professional meeting rooms");
        elements.push("modern trading floor");
    }

    if combined.contains("manufacturing") || combined.contains("industrial") {
        elements.push("modern machinery");
        elements.push("safety equipment");
        elements.push("production lines");
    }

    elements.join(", ")
}

/// Download image from URL and save to storage (S3 or local)
async fn download_and_save_image(
    state: &crate::common::AppState,
    image_url: &str,
    job_id: &str,
) -> Result<String, ApiError> {
    use tracing::warn;

    let image_data: Vec<u8>;

    // If it's already a base64 data URI, decode it
    if image_url.starts_with("data:image/") {
        let base64_data = image_url
            .split(',')
            .nth(1)
            .ok_or_else(|| ApiError::InternalServer("Invalid base64 image data".to_string()))?;

        image_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            base64_data,
        )
        .map_err(|e| ApiError::InternalServer(format!("Failed to decode base64: {}", e)))?;
    } else {
        // Download from URL
        let client = reqwest::Client::new();
        let response = client
            .get(image_url)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| ApiError::InternalServer(format!("Failed to download image: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::InternalServer(format!(
                "Failed to download image: HTTP {}",
                response.status()
            )));
        }

        image_data = response
            .bytes()
            .await
            .map_err(|e| ApiError::InternalServer(format!("Failed to read image data: {}", e)))?
            .to_vec();
    }

    let filename = format!("ai_{}_{}.png", job_id, generate_raw_id(6));

    // Check storage type setting
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    if storage_type.starts_with("s3") {
        // Upload to S3
        let s3_key = format!("job-images/jobs/{}", filename);
        match state
            .aws_service
            .upload_file(image_data.clone(), &s3_key, "image/png")
            .await
        {
            Ok(s3_url) => {
                info!(s3_key = %s3_key, job_id = %job_id, "AI-generated job image uploaded to S3");
                return Ok(s3_url);
            }
            Err(e) => {
                warn!(error = %e, job_id = %job_id, "Failed to upload AI image to S3, falling back to local storage");
                // Fall through to local storage
            }
        }
    }

    // Save to local storage
    let file_path = state.job_images_jobs_dir.join(&filename);
    tokio::fs::write(&file_path, &image_data)
        .await
        .map_err(|e| ApiError::InternalServer(format!("Failed to save image: {}", e)))?;

    Ok(format!("/api/job-images/jobs/{}", filename))
}
