// src/candidates/handlers/ai.rs
//! AI-powered candidate communication handlers

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::common::error::ApiError;
use crate::common::state::AppState;
use crate::services::openai::TextGenerationPurpose;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateCandidateEmailRequest {
    pub candidate_name: String,
    pub job_title: String,
    pub company_name: String,
    pub stage: String,
    pub additional_context: Option<String>,
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

// ============================================================================
// Handlers
// ============================================================================

/// Generate candidate email using AI
/// POST /api/admin/candidates/ai/generate-email
pub async fn generate_candidate_email(
    Extension(state_lock): Extension<Arc<RwLock<AppState>>>,
    Json(req): Json<GenerateCandidateEmailRequest>,
) -> Result<Json<AIGenerationResponse>, ApiError> {
    info!(
        candidate = %req.candidate_name,
        stage = %req.stage,
        "Generating candidate email with AI"
    );
    let state = state_lock.read().await;

    let stage_description = match req.stage.as_str() {
        "interview" => "invitation to interview",
        "offer" => "job offer",
        "rejection" => "application rejection (be empathetic and encouraging)",
        "welcome" => "welcome to the team",
        "follow_up" => "follow-up after interview",
        "screening" => "initial screening invitation",
        _ => "general communication",
    };

    let context = serde_json::json!({
        "candidate_name": req.candidate_name,
        "job_title": req.job_title,
        "company_name": req.company_name,
        "stage": req.stage,
        "additional_context": req.additional_context,
    });

    let prompt = format!(
        "Write a brief professional email (under 100 words, excluding subject line).\n\n\
        DETAILS (use these exact values, NO placeholders):\n\
        - Candidate: {}\n\
        - Position: {}\n\
        - Company: {}\n\
        - Purpose: {}\n\n\
        FORMAT:\n\
        Subject: [clear subject line]\n\n\
        [Brief greeting using '{}']\n\
        [2-3 sentences max - get to the point]\n\
        [Clear next step or call to action]\n\
        [Sign off]\n\n\
        RULES:\n\
        - Use actual names: {}, {}, {}\n\
        - NO placeholders like [Name] or [Company]\n\
        - Be warm but concise\n\
        {}",
        req.candidate_name,
        req.job_title,
        req.company_name,
        stage_description,
        req.candidate_name,
        req.candidate_name,
        req.job_title,
        req.company_name,
        req.additional_context.as_deref().unwrap_or("")
    );

    let result = state
        .openai_service
        .generate_text(
            TextGenerationPurpose::EmailGeneration,
            &prompt,
            Some(context),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to generate candidate email");
            ApiError::ServiceUnavailable(format!("AI service error: {}", e))
        })?;

    Ok(Json(AIGenerationResponse {
        content: serde_json::json!(result),
        metadata: Some(AIGenerationMetadata {
            model: "gpt-5-mini".to_string(),
            tokens_used: None,
            generation_time_ms: None,
        }),
    }))
}
