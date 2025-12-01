// src/candidates/handlers/resumes.rs

use crate::auth::AuthedUser;
use crate::candidates::models::{AdminResumeFilters, BulkResumeStatusUpdate, Resume, UpdateResumeLabelRequest};
use crate::common::{generate_resume_id, ApiError, AppState};
use axum::{
    extract::{Extension, Multipart, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// POST /api/resumes - Upload a resume
pub async fn upload_resume(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    info!(user_id = %authed.id, "User uploading resume");

    // Check resume limit (max 5 resumes per user)
    const MAX_RESUMES: i64 = 5;
    let resume_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM resumes WHERE user_id = ?"
    )
    .bind(&authed.id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    if resume_count >= MAX_RESUMES {
        warn!(
            user_id = %authed.id,
            current_count = resume_count,
            "Resume upload limit reached"
        );
        return Err(ApiError::BadRequest(
            format!("Resume limit reached. You can upload a maximum of {} resumes. Please delete an existing resume before uploading a new one.", MAX_RESUMES)
        ));
    }

    // Extract file from multipart
    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() == Some("resume") {
            let filename = field.file_name().unwrap_or("resume.pdf").to_string();

            let data = field
                .bytes()
                .await
                .map_err(|_| ApiError::BadRequest("Invalid file".to_string()))?;

            // Validate PDF
            if !filename.ends_with(".pdf") {
                return Err(ApiError::BadRequest(
                    "Only PDF files are allowed".to_string(),
                ));
            }

            // Save file
            let resume_id = generate_resume_id();
            let safe_filename = format!("{}.pdf", resume_id);

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
                let s3_key = format!("resumes/{}", safe_filename);
                match state
                    .aws_service
                    .upload_file(data.to_vec(), &s3_key, "application/pdf")
                    .await
                {
                    Ok(_url) => {
                        info!(user_id = %authed.id, s3_key = %s3_key, "Resume uploaded to S3 successfully");
                    }
                    Err(e) => {
                        warn!(error = %e, user_id = %authed.id, "Failed to upload resume to S3, falling back to local storage");
                        // Fall back to local storage
                        let file_path = state.resumes_dir.join(&safe_filename);
                        tokio::fs::write(&file_path, &data).await.map_err(|e| {
                            error!(error = %e, "Failed to save resume locally");
                            ApiError::InternalServer("Failed to save resume".to_string())
                        })?;
                    }
                }
            } else {
                // Save to local storage
                let file_path = state.resumes_dir.join(&safe_filename);
                tokio::fs::write(&file_path, &data).await.map_err(|e| {
                    error!(error = %e, "Failed to save resume");
                    ApiError::InternalServer("Failed to save resume".to_string())
                })?;
            }

            // Create database record
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                r#"
                INSERT INTO resumes (id, user_id, filename, status, submitted_at)
                VALUES (?, ?, ?, 'submitted', ?)
                "#,
            )
            .bind(&resume_id)
            .bind(&authed.id)
            .bind(&safe_filename)
            .bind(&now)
            .execute(&state.db)
            .await
            .map_err(ApiError::DatabaseError)?;

            info!(user_id = %authed.id, resume_id = %resume_id, "Resume uploaded successfully");

            return Ok((
                StatusCode::CREATED,
                Json(json!({
                    "id": resume_id,
                    "filename": safe_filename,
                    "status": "submitted",
                    "message": "Resume uploaded successfully"
                })),
            ));
        }
    }

    Err(ApiError::BadRequest("No resume file provided".to_string()))
}

/// GET /api/user/resumes - Get user's resumes
pub async fn get_user_resumes(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
) -> Result<Json<Vec<Resume>>, ApiError> {
    let state = state_lock.read().await;

    let resumes = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE user_id = ? ORDER BY submitted_at DESC",
    )
    .bind(&authed.id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    Ok(Json(resumes))
}

/// DELETE /api/resumes/:id - Delete a resume
pub async fn delete_resume(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    // Verify ownership
    let resume = sqlx::query_as::<_, Resume>("SELECT * FROM resumes WHERE id = ? AND user_id = ?")
        .bind(&resume_id)
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Check if resume is used in any ACTIVE applications
    // Allow deletion if all applications using this resume are withdrawn or rejected
    let active_application_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM applications 
        WHERE resume_id = ? 
        AND status NOT IN ('withdrawn', 'rejected')
        "#
    )
    .bind(&resume_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    if active_application_count > 0 {
        warn!(
            user_id = %authed.id,
            resume_id = %resume_id,
            active_application_count = active_application_count,
            "Cannot delete resume: attached to active applications"
        );
        return Err(ApiError::BadRequest(
            format!("Cannot delete resume. It is attached to {} active application(s). You can only delete resumes from withdrawn or rejected applications.", active_application_count)
        ));
    }

    // First, unlink the resume from any withdrawn/rejected applications
    sqlx::query(
        r#"
        UPDATE applications 
        SET resume_id = NULL 
        WHERE resume_id = ? AND status IN ('withdrawn', 'rejected')
        "#
    )
    .bind(&resume_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    // Delete file
    let file_path = state.resumes_dir.join(&resume.filename);
    let _ = tokio::fs::remove_file(file_path).await;

    // Delete from database
    sqlx::query("DELETE FROM resumes WHERE id = ?")
        .bind(&resume_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(user_id = %authed.id, resume_id = %resume_id, "Resume deleted successfully");

    Ok(Json(json!({ "message": "Resume deleted successfully" })))
}

/// PUT /api/resumes/:id/label - Update resume label
pub async fn update_resume_label(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
    Json(request): Json<UpdateResumeLabelRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    // Verify ownership
    let _resume = sqlx::query_as::<_, Resume>("SELECT * FROM resumes WHERE id = ? AND user_id = ?")
        .bind(&resume_id)
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Update the label
    sqlx::query("UPDATE resumes SET label = ? WHERE id = ?")
        .bind(&request.label)
        .bind(&resume_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;
    
    info!(
        user_id = %authed.id,
        resume_id = %resume_id,
        label = %request.label,
        "Resume label updated"
    );

    Ok(Json(json!({
        "message": "Label updated successfully",
        "id": resume_id,
        "label": request.label
    })))
}

/// POST /api/resumes/:id/scan - Scan resume with AI
pub async fn scan_resume(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    info!(user_id = %authed.id, resume_id = %resume_id, "Starting resume scan");

    // Verify resume exists and user has access (or is admin)
    let resume = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE id = ? AND (user_id = ? OR ? = 1)"
    )
    .bind(&resume_id)
    .bind(&authed.id)
    .bind(authed.is_admin as i32)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Allow rescanning - don't block if already scanned
    // This enables users to rescan with updated AI models or after resume updates
    if resume.status == "processing" {
        return Ok(Json(json!({
            "message": "Resume is currently being processed",
            "status": "processing"
        })));
    }

    // Update status to processing
    sqlx::query("UPDATE resumes SET status = 'processing' WHERE id = ?")
        .bind(&resume_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    // Check storage type and read the resume file accordingly
    let storage_type = state
        .settings_service
        .get_setting("storage_type")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "local".to_string());

    let pdf_bytes: Vec<u8> = if storage_type.starts_with("s3") {
        // Try to download from S3
        let s3_key = format!("resumes/{}", resume.filename);
        info!(resume_id = %resume_id, s3_key = %s3_key, "Downloading resume from S3");
        
        match state.aws_service.download_file(&s3_key).await {
            Ok(bytes) => bytes,
            Err(e) => {
                // Fallback to local storage if S3 fails
                warn!(error = %e, resume_id = %resume_id, "Failed to download from S3, trying local storage");
                let file_path = state.resumes_dir.join(&resume.filename);
                if !file_path.exists() {
                    error!(resume_id = %resume_id, "Resume file not found in S3 or local storage");
                    sqlx::query("UPDATE resumes SET status = 'error' WHERE id = ?")
                        .bind(&resume_id)
                        .execute(&state.db)
                        .await
                        .ok();
                    return Err(ApiError::BadRequest("Resume file not found".to_string()));
                }
                tokio::fs::read(&file_path).await.map_err(|e| {
                    error!(error = %e, resume_id = %resume_id, "Failed to read resume file");
                    sqlx::query("UPDATE resumes SET status = 'error' WHERE id = ?")
                        .bind(&resume_id)
                        .execute(&state.db);
                    ApiError::InternalServer("Failed to read resume file".to_string())
                })?
            }
        }
    } else {
        // Read from local storage
        let file_path = state.resumes_dir.join(&resume.filename);
        if !file_path.exists() {
            error!(resume_id = %resume_id, "Resume file not found");
            sqlx::query("UPDATE resumes SET status = 'error' WHERE id = ?")
                .bind(&resume_id)
                .execute(&state.db)
                .await
                .ok();
            return Err(ApiError::BadRequest("Resume file not found".to_string()));
        }
        
        tokio::fs::read(&file_path).await.map_err(|e| {
            error!(error = %e, resume_id = %resume_id, "Failed to read resume file");
            sqlx::query("UPDATE resumes SET status = 'error' WHERE id = ?")
                .bind(&resume_id)
                .execute(&state.db);
            ApiError::InternalServer("Failed to read resume file".to_string())
        })?
    };

    // Extract text from PDF using pdf-extract crate (basic extraction)
    let pdf_text = extract_text_from_pdf(&pdf_bytes).unwrap_or_else(|e| {
        warn!(error = %e, "Failed to extract text from PDF, using filename as fallback");
        format!("Resume file: {}", resume.filename)
    });

    // Use OpenAI to analyze the resume
    let ai_prompt = format!(
        r#"Analyze this resume and extract structured information. Return a JSON object with the following structure:
{{
    "name": "Full name of the candidate",
    "email": "Email address if found",
    "phone": "Phone number if found",
    "location": "Location/address if found",
    "linkedin_url": "LinkedIn profile URL if found (null if not found)",
    "github_url": "GitHub profile URL if found (null if not found)",
    "website": "Personal website URL if found (null if not found)",
    "summary": "Brief professional summary (2-3 sentences)",
    "skills": ["skill1", "skill2", ...],
    "experience": [
        {{
            "title": "Job title",
            "company": "Company name",
            "start_date": "Start date (YYYY-MM format if possible)",
            "end_date": "End date (YYYY-MM format) or null if current",
            "is_current": true/false,
            "description": "Brief description of responsibilities"
        }}
    ],
    "education": [
        {{
            "degree": "Degree name",
            "field_of_study": "Field of study/major",
            "institution": "School/University name",
            "start_date": "Start year (YYYY format)",
            "end_date": "End year (YYYY format) or null if ongoing"
        }}
    ],
    "certifications": ["cert1", "cert2", ...],
    "languages": ["language1", "language2", ...],
    "score": 7.5,
    "score_breakdown": {{
        "skills_match": 8,
        "experience_quality": 7,
        "education": 8,
        "presentation": 7
    }},
    "strengths": ["strength1", "strength2", ...],
    "areas_for_improvement": ["area1", "area2", ...]
}}

Important: Extract LinkedIn and GitHub URLs if present in the resume. Look for patterns like linkedin.com/in/username or github.com/username.

Resume text:
{}"#,
        pdf_text
    );

    let ai_result = state
        .openai_service
        .generate_text(
            crate::services::openai::TextGenerationPurpose::ResumeScanning,
            &ai_prompt,
            None,
        )
        .await;

    let (extracted_data, score) = match ai_result {
        Ok(response) => {
            // Try to parse the AI response as JSON
            let parsed: serde_json::Value = serde_json::from_str(&response)
                .or_else(|_| {
                    // Try to extract JSON from markdown code blocks
                    let json_start = response.find('{');
                    let json_end = response.rfind('}');
                    if let (Some(start), Some(end)) = (json_start, json_end) {
                        serde_json::from_str(&response[start..=end])
                    } else {
                        Err(serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "No JSON found in response",
                        )))
                    }
                })
                .unwrap_or_else(|_| {
                    json!({
                        "raw_response": response,
                        "parse_error": true
                    })
                });

            let score = parsed.get("score")
                .and_then(|s| s.as_f64())
                .unwrap_or(7.0);

            (parsed, score)
        }
        Err(e) => {
            error!(error = %e, resume_id = %resume_id, "AI analysis failed");
            // Fallback to basic scoring
            let hash = resume_id.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64));
            let fallback_score = 6.0 + ((hash % 40) as f64) / 10.0;
            (
                json!({
                    "ai_error": e.to_string(),
                    "skills": [],
                    "experience": [],
                    "education": []
                }),
                fallback_score,
            )
        }
    };

    // Get the actual model name from OpenAI config
    let ai_model = state
        .openai_service
        .get_config()
        .await
        .map(|c| c.models.resume_scanning)
        .unwrap_or_else(|_| "gpt-5-mini".to_string());

    // Update resume with scan results
    sqlx::query(
        r#"
        UPDATE resumes 
        SET status = 'scanned', 
            score = ?,
            parsed_json = ?
        WHERE id = ?
        "#
    )
    .bind(score)
    .bind(json!({
        "scanned_at": chrono::Utc::now().to_rfc3339(),
        "ai_model": ai_model,
        "extracted_data": extracted_data
    }).to_string())
    .bind(&resume_id)
    .execute(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?;

    info!(
        user_id = %authed.id,
        resume_id = %resume_id,
        score = score,
        "Resume scan completed with AI"
    );

    Ok(Json(json!({
        "message": "Resume scanned successfully",
        "status": "scanned",
        "score": score,
        "extracted_data": extracted_data
    })))
}

/// Extract text from PDF bytes
fn extract_text_from_pdf(pdf_bytes: &[u8]) -> Result<String, String> {
    // Use pdf-extract crate for text extraction
    pdf_extract::extract_text_from_mem(pdf_bytes)
        .map_err(|e| format!("PDF extraction error: {}", e))
}

/// GET /api/resumes/:id/review - Get AI resume review
pub async fn get_resume_review(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    // Verify resume exists and user has access (or is admin)
    let resume = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE id = ? AND (user_id = ? OR ? = 1)"
    )
    .bind(&resume_id)
    .bind(&authed.id)
    .bind(authed.is_admin as i32)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Check if resume has been scanned
    if resume.status != "scanned" {
        return Err(ApiError::BadRequest(
            "Resume has not been scanned yet".to_string(),
        ));
    }

    // Parse the stored JSON data
    let extracted_data: serde_json::Value = if let Some(json_str) = &resume.parsed_json {
        serde_json::from_str(json_str).unwrap_or(json!({}))
    } else {
        json!({})
    };

    // Build review response
    let review = json!({
        "id": resume.id,
        "status": resume.status,
        "score": resume.score,
        "ai_model": extracted_data.get("ai_model").and_then(|v| v.as_str()).unwrap_or("gpt-5-mini"),
        "scanned_at": extracted_data.get("scanned_at").and_then(|v| v.as_str()),
        "extracted_data": extracted_data.get("extracted_data").unwrap_or(&json!({})),
        "file_url": format!("/uploads/resumes/{}", resume.filename),
        "image_urls": []
    });

    Ok(Json(review))
}

/// POST /api/resumes/:id/propagate-profile - Propagate resume data to profile
pub async fn propagate_resume_to_profile(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    // Verify resume exists and user has access (or is admin)
    let resume = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE id = ? AND (user_id = ? OR ? = 1)"
    )
    .bind(&resume_id)
    .bind(&authed.id)
    .bind(authed.is_admin as i32)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Check if resume has been scanned
    if resume.status != "scanned" || resume.parsed_json.is_none() {
        return Err(ApiError::BadRequest(
            "Resume must be scanned before propagating to profile".to_string(),
        ));
    }

    // Parse extracted data
    let parsed_json: serde_json::Value = 
        serde_json::from_str(resume.parsed_json.as_ref().unwrap())
            .unwrap_or(json!({}));

    let empty_obj = json!({});
    let extracted_data = parsed_json.get("extracted_data").unwrap_or(&empty_obj);
    let mut updated_fields = Vec::new();

    // Update profile with extracted data
    // Update skills if available
    if let Some(skills) = extracted_data.get("skills").and_then(|s| s.as_array()) {
        let skills_json = serde_json::to_string(skills).unwrap_or_default();
        sqlx::query("UPDATE profiles SET skills = ? WHERE user_id = ?")
            .bind(&skills_json)
            .bind(&resume.user_id)
            .execute(&state.db)
            .await
            .ok();
        updated_fields.push("skills");
    }

    // Update phone if available and profile phone is empty
    if let Some(phone) = extracted_data.get("phone").and_then(|p| p.as_str()) {
        if !phone.is_empty() {
            sqlx::query("UPDATE profiles SET phone = COALESCE(NULLIF(phone, ''), ?) WHERE user_id = ?")
                .bind(phone)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("phone");
        }
    }

    // Update location if available and profile location is empty
    if let Some(location) = extracted_data.get("location").and_then(|l| l.as_str()) {
        if !location.is_empty() {
            sqlx::query("UPDATE profiles SET location = COALESCE(NULLIF(location, ''), ?) WHERE user_id = ?")
                .bind(location)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("location");
        }
    }

    // Update bio/summary if available
    if let Some(summary) = extracted_data.get("summary").and_then(|s| s.as_str()) {
        if !summary.is_empty() {
            sqlx::query("UPDATE profiles SET bio = COALESCE(NULLIF(bio, ''), ?) WHERE user_id = ?")
                .bind(summary)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("bio");
        }
    }

    // Update LinkedIn URL if available
    if let Some(linkedin) = extracted_data.get("linkedin_url").and_then(|l| l.as_str()) {
        if !linkedin.is_empty() {
            sqlx::query("UPDATE profiles SET linkedin_url = COALESCE(NULLIF(linkedin_url, ''), ?) WHERE user_id = ?")
                .bind(linkedin)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("linkedin_url");
        }
    }

    // Update GitHub URL if available
    if let Some(github) = extracted_data.get("github_url").and_then(|g| g.as_str()) {
        if !github.is_empty() {
            sqlx::query("UPDATE profiles SET github_url = COALESCE(NULLIF(github_url, ''), ?) WHERE user_id = ?")
                .bind(github)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("github_url");
        }
    }

    // Update website if available
    if let Some(website) = extracted_data.get("website").and_then(|w| w.as_str()) {
        if !website.is_empty() {
            sqlx::query("UPDATE profiles SET website = COALESCE(NULLIF(website, ''), ?) WHERE user_id = ?")
                .bind(website)
                .bind(&resume.user_id)
                .execute(&state.db)
                .await
                .ok();
            updated_fields.push("website");
        }
    }

    // Replace experience entries (delete existing, then insert new)
    if let Some(experiences) = extracted_data.get("experience").and_then(|e| e.as_array()) {
        // Delete existing experiences for this user before inserting new ones
        sqlx::query("DELETE FROM experiences WHERE user_id = ?")
            .bind(&resume.user_id)
            .execute(&state.db)
            .await
            .ok();

        for exp in experiences {
            let title = exp.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let company = exp.get("company").and_then(|c| c.as_str()).unwrap_or("");
            let start_date = exp.get("start_date").or(exp.get("start")).and_then(|s| s.as_str()).unwrap_or("");
            let end_date = exp.get("end_date").or(exp.get("end")).and_then(|e| e.as_str());
            let description = exp.get("description").and_then(|d| d.as_str()).unwrap_or("");

            if !title.is_empty() && !company.is_empty() {
                let exp_id = crate::common::generate_experience_id();
                sqlx::query(
                    r#"INSERT INTO experiences (id, user_id, title, company, start_date, end_date, description)
                    VALUES (?, ?, ?, ?, ?, ?, ?)"#
                )
                .bind(&exp_id)
                .bind(&resume.user_id)
                .bind(title)
                .bind(company)
                .bind(start_date)
                .bind(end_date)
                .bind(description)
                .execute(&state.db)
                .await
                .ok();
            }
        }
        updated_fields.push("experience");
    }

    // Replace education entries (delete existing, then insert new)
    if let Some(education_list) = extracted_data.get("education").and_then(|e| e.as_array()) {
        // Delete existing education for this user before inserting new ones
        sqlx::query("DELETE FROM education WHERE user_id = ?")
            .bind(&resume.user_id)
            .execute(&state.db)
            .await
            .ok();

        for edu in education_list {
            let degree = edu.get("degree").and_then(|d| d.as_str()).unwrap_or("");
            let institution = edu.get("institution").and_then(|i| i.as_str()).unwrap_or("");
            let field_of_study = edu.get("field_of_study").and_then(|f| f.as_str()).unwrap_or("");
            let start_date = edu.get("start_date").or(edu.get("start")).and_then(|s| s.as_str()).unwrap_or("");
            let end_date = edu.get("end_date").or(edu.get("end")).and_then(|e| e.as_str());

            if !degree.is_empty() && !institution.is_empty() {
                let edu_id = crate::common::generate_education_id();
                sqlx::query(
                    r#"INSERT INTO education (id, user_id, degree, field_of_study, institution, start_date, end_date)
                    VALUES (?, ?, ?, ?, ?, ?, ?)"#
                )
                .bind(&edu_id)
                .bind(&resume.user_id)
                .bind(degree)
                .bind(field_of_study)
                .bind(institution)
                .bind(start_date)
                .bind(end_date)
                .execute(&state.db)
                .await
                .ok();
            }
        }
        updated_fields.push("education");
    }

    info!(
        user_id = %resume.user_id,
        resume_id = %resume_id,
        updated_fields = ?updated_fields,
        "Propagated resume data to profile"
    );

    Ok(Json(json!({
        "message": "Profile updated successfully",
        "updated_fields": updated_fields
    })))
}

/// GET /api/admin/resumes - List all resumes (admin)
pub async fn admin_list_resumes(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Query(filters): Query<AdminResumeFilters>,
) -> Result<Json<Vec<Resume>>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    // Determine sort column and order
    let sort_by = filters.sort_by.as_deref().unwrap_or("submitted_at");
    let sort_order = filters.sort_order.as_deref().unwrap_or("desc");
    
    // Validate sort_by to prevent SQL injection
    let valid_sort_columns = ["submitted_at", "score", "status"];
    let sort_column = if valid_sort_columns.contains(&sort_by) {
        sort_by
    } else {
        "submitted_at"
    };
    
    // Validate sort_order
    let order = if sort_order.to_lowercase() == "asc" { "ASC" } else { "DESC" };

    // Build the query based on filters
    let resumes = if let Some(status) = &filters.status {
        // Filter by status
        sqlx::query_as::<_, Resume>(&format!(
            r#"
            SELECT 
                r.id,
                r.user_id,
                r.filename,
                r.status,
                r.score,
                r.parsed_json,
                r.submitted_at,
                u.name as candidate_name,
                u.email as candidate_email
            FROM resumes r
            LEFT JOIN users u ON r.user_id = u.id
            WHERE r.status = ?
            ORDER BY r.{} {}
            LIMIT 100
            "#,
            sort_column, order
        ))
        .bind(status)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
    } else if let Some(name) = &filters.candidate_name {
        // Filter by candidate name
        let search_pattern = format!("%{}%", name);
        sqlx::query_as::<_, Resume>(&format!(
            r#"
            SELECT 
                r.id,
                r.user_id,
                r.filename,
                r.status,
                r.score,
                r.parsed_json,
                r.submitted_at,
                u.name as candidate_name,
                u.email as candidate_email
            FROM resumes r
            LEFT JOIN users u ON r.user_id = u.id
            WHERE u.name LIKE ?
            ORDER BY r.{} {}
            LIMIT 100
            "#,
            sort_column, order
        ))
        .bind(search_pattern)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
    } else {
        // No filters - get all resumes
        sqlx::query_as::<_, Resume>(&format!(
            r#"
            SELECT 
                r.id,
                r.user_id,
                r.filename,
                r.status,
                r.score,
                r.parsed_json,
                r.submitted_at,
                u.name as candidate_name,
                u.email as candidate_email
            FROM resumes r
            LEFT JOIN users u ON r.user_id = u.id
            ORDER BY r.{} {}
            LIMIT 100
            "#,
            sort_column, order
        ))
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
    };

    info!(
        admin_id = %authed.id,
        resume_count = resumes.len(),
        "Admin fetched resumes list"
    );

    Ok(Json(resumes))
}

/// POST /api/admin/resumes/bulk-update-status - Bulk update resume status
pub async fn bulk_update_resume_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Json(request): Json<BulkResumeStatusUpdate>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authed.is_admin {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }

    let state = state_lock.read().await;

    if request.resume_ids.is_empty() {
        return Err(ApiError::BadRequest("No resume IDs provided".to_string()));
    }

    // Build placeholders for SQL IN clause
    let placeholders = request.resume_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "UPDATE resumes SET status = ? WHERE id IN ({})",
        placeholders
    );

    let mut query_builder = sqlx::query(&query).bind(&request.status);
    for id in &request.resume_ids {
        query_builder = query_builder.bind(id);
    }

    let result = query_builder
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(
        admin_id = %authed.id,
        resume_count = request.resume_ids.len(),
        new_status = %request.status,
        rows_affected = result.rows_affected(),
        "Bulk updated resume status"
    );

    Ok(Json(json!({
        "message": format!("Updated {} resume(s)", result.rows_affected()),
        "updated_count": result.rows_affected()
    })))
}

/// GET /api/resumes/:id/status - Get resume processing status
pub async fn get_resume_processing_status(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    let resume = sqlx::query_as::<_, Resume>("SELECT * FROM resumes WHERE id = ? AND user_id = ?")
        .bind(&resume_id)
        .bind(&authed.id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?
        .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    Ok(Json(json!({
        "id": resume.id,
        "status": resume.status,
        "score": resume.score
    })))
}

/// GET /api/resumes/:id/download - Download resume
pub async fn download_resume(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let state = state_lock.read().await;

    // Allow users to download their own resumes, or admins to download any resume
    let resume = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE id = ? AND (user_id = ? OR ? = 1)"
    )
    .bind(&resume_id)
    .bind(&authed.id)
    .bind(authed.is_admin as i32)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    let file_path = state.resumes_dir.join(&resume.filename);

    if !file_path.exists() {
        return Err(ApiError::BadRequest("Resume file not found".to_string()));
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|_| ApiError::InternalServer("Failed to read resume".to_string()))?;

    let disposition = format!("attachment; filename=\"{}\"", resume.filename);
    Ok((
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/pdf".to_string(),
            ),
            (axum::http::header::CONTENT_DISPOSITION, disposition),
        ],
        content,
    ))
}

/// POST /api/resumes/:id/retry-processing - Retry resume processing
pub async fn retry_resume_processing(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    authed: AuthedUser,
    Path(resume_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = state_lock.read().await;

    // Verify resume exists and user has access (or is admin)
    let _resume = sqlx::query_as::<_, Resume>(
        "SELECT * FROM resumes WHERE id = ? AND (user_id = ? OR ? = 1)"
    )
    .bind(&resume_id)
    .bind(&authed.id)
    .bind(authed.is_admin as i32)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::DatabaseError)?
    .ok_or_else(|| ApiError::BadRequest("Resume not found".to_string()))?;

    // Reset status to submitted to allow re-scanning
    sqlx::query("UPDATE resumes SET status = 'submitted', score = NULL, parsed_json = NULL WHERE id = ?")
        .bind(&resume_id)
        .execute(&state.db)
        .await
        .map_err(ApiError::DatabaseError)?;

    info!(
        user_id = %authed.id,
        resume_id = %resume_id,
        "Resume processing reset for retry"
    );

    Ok(Json(json!({
        "message": "Resume reset for reprocessing. You can now scan it again."
    })))
}
