// src/candidates/validators.rs

use super::models::*;
use crate::common::{ValidationResult, Validator};
use chrono::NaiveDate;
use std::collections::HashSet;

// ============================================================================
// Application Validators
// ============================================================================

pub struct ApplicationValidator;

impl Validator<CreateApplicationRequest> for ApplicationValidator {
    fn validate(&self, data: &CreateApplicationRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        if data.job_id.trim().is_empty() {
            result.add_error("job_id", "Job ID is required");
        } else if !is_valid_uuid(&data.job_id) {
            result.add_error("job_id", "Job ID must be a valid UUID");
        }

        if let Some(resume_id) = &data.resume_id {
            if !is_valid_uuid(resume_id) {
                result.add_error("resume_id", "Resume ID must be a valid UUID");
            }
        }



        if let Some(cover_letter) = &data.cover_letter {
            if cover_letter.len() > 5000 {
                result.add_error(
                    "cover_letter",
                    "Cover letter must be less than 5000 characters",
                );
            }
        }

        result
    }
}

impl Validator<UpdateApplicationStatusRequest> for ApplicationValidator {
    fn validate(&self, data: &UpdateApplicationStatusRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        let valid_statuses = HashSet::from([
            "submitted",
            "reviewed",
            "shortlisted",
            "interview_scheduled",
            "interviewed",
            "offered",
            "hired",
            "rejected",
            "withdrawn",
        ]);

        if !valid_statuses.contains(data.status.as_str()) {
            result.add_error("status", "Invalid application status");
        }

        if let Some(notes) = &data.notes {
            if notes.len() > 1000 {
                result.add_error("notes", "Notes must be less than 1000 characters");
            }
        }

        result
    }
}

// ============================================================================
// Resume Processing Validators
// ============================================================================

pub struct ResumeProcessingValidator;

impl Validator<AdminResumeFilters> for ResumeProcessingValidator {
    fn validate(&self, data: &AdminResumeFilters) -> ValidationResult {
        let mut result = ValidationResult::new();

        if let Some(status) = &data.status {
            let valid_statuses = HashSet::from([
                "submitted",
                "processing",
                "completed",
                "failed",
                "scanned",
                "pending",
            ]);
            if !valid_statuses.contains(status.as_str()) {
                result.add_error("status", "Invalid resume status");
            }
        }

        if let Some(date_from) = &data.date_from {
            if let Err(_) = validate_date_format(date_from) {
                result.add_error("date_from", "Date from must be in YYYY-MM-DD format");
            }
        }

        if let Some(date_to) = &data.date_to {
            if let Err(_) = validate_date_format(date_to) {
                result.add_error("date_to", "Date to must be in YYYY-MM-DD format");
            }
        }

        if let (Some(date_from), Some(date_to)) = (&data.date_from, &data.date_to) {
            if let (Ok(from), Ok(to)) = (
                NaiveDate::parse_from_str(date_from, "%Y-%m-%d"),
                NaiveDate::parse_from_str(date_to, "%Y-%m-%d"),
            ) {
                if to < from {
                    result.add_error("date_range", "Date to must be after date from");
                }
            }
        }

        if let Some(score_min) = data.score_min {
            if score_min < 0.0 || score_min > 100.0 {
                result.add_error("score_min", "Minimum score must be between 0 and 100");
            }
        }

        if let Some(score_max) = data.score_max {
            if score_max < 0.0 || score_max > 100.0 {
                result.add_error("score_max", "Maximum score must be between 0 and 100");
            }
        }

        if let (Some(score_min), Some(score_max)) = (data.score_min, data.score_max) {
            if score_min > score_max {
                result.add_error(
                    "score_range",
                    "Minimum score cannot be greater than maximum score",
                );
            }
        }

        if let Some(page) = data.page {
            if page < 1 {
                result.add_error("page", "Page must be greater than 0");
            }
        }

        if let Some(limit) = data.limit {
            if limit < 1 || limit > 100 {
                result.add_error("limit", "Limit must be between 1 and 100");
            }
        }

        if let Some(sort_by) = &data.sort_by {
            let valid_sort_fields = HashSet::from([
                "submitted_at",
                "score",
                "status",
                "candidate_name",
                "updated_at",
            ]);
            if !valid_sort_fields.contains(sort_by.as_str()) {
                result.add_error("sort_by", "Invalid sort field");
            }
        }

        if let Some(sort_order) = &data.sort_order {
            let valid_orders = HashSet::from(["asc", "desc"]);
            if !valid_orders.contains(sort_order.as_str()) {
                result.add_error("sort_order", "Sort order must be 'asc' or 'desc'");
            }
        }

        result
    }
}

impl Validator<BulkResumeStatusUpdate> for ResumeProcessingValidator {
    fn validate(&self, data: &BulkResumeStatusUpdate) -> ValidationResult {
        let mut result = ValidationResult::new();

        if data.resume_ids.is_empty() {
            result.add_error("resume_ids", "At least one resume ID is required");
        } else if data.resume_ids.len() > 50 {
            result.add_error("resume_ids", "Cannot update more than 50 resumes at once");
        } else {
            for (index, resume_id) in data.resume_ids.iter().enumerate() {
                if !is_valid_uuid(resume_id) {
                    result.add_error(
                        &format!("resume_ids[{}]", index),
                        "Invalid resume ID format",
                    );
                }
            }
        }

        let valid_statuses = HashSet::from([
            "submitted",
            "processing",
            "completed",
            "failed",
            "scanned",
            "pending",
            "rejected",
        ]);
        if !valid_statuses.contains(data.status.as_str()) {
            result.add_error("status", "Invalid resume status");
        }

        if let Some(notes) = &data.notes {
            if notes.len() > 1000 {
                result.add_error("notes", "Notes must be less than 1000 characters");
            }
        }

        result
    }
}

impl Validator<RetryResumeProcessingRequest> for ResumeProcessingValidator {
    fn validate(&self, data: &RetryResumeProcessingRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        if let Some(priority) = &data.priority {
            let valid_priorities = HashSet::from(["high", "normal", "low"]);
            if !valid_priorities.contains(priority.as_str()) {
                result.add_error("priority", "Priority must be 'high', 'normal', or 'low'");
            }
        }

        result
    }
}

// ============================================================================
// Bulk Operation Validators
// ============================================================================

pub struct BulkOperationValidator;

impl Validator<BulkApplicationStatusUpdate> for BulkOperationValidator {
    fn validate(&self, data: &BulkApplicationStatusUpdate) -> ValidationResult {
        let mut result = ValidationResult::new();

        if data.application_ids.is_empty() {
            result.add_error("application_ids", "At least one application ID is required");
        } else if data.application_ids.len() > 100 {
            result.add_error(
                "application_ids",
                "Cannot update more than 100 applications at once",
            );
        } else {
            for (index, app_id) in data.application_ids.iter().enumerate() {
                if !is_valid_uuid(app_id) {
                    result.add_error(
                        &format!("application_ids[{}]", index),
                        "Invalid application ID format",
                    );
                }
            }
        }

        let valid_statuses = HashSet::from([
            "submitted",
            "reviewed",
            "shortlisted",
            "interview_scheduled",
            "interviewed",
            "offered",
            "hired",
            "rejected",
            "withdrawn",
        ]);
        if !valid_statuses.contains(data.status.as_str()) {
            result.add_error("status", "Invalid application status");
        }

        if let Some(notes) = &data.notes {
            if notes.len() > 1000 {
                result.add_error("notes", "Notes must be less than 1000 characters");
            }
        }

        result
    }
}

// ============================================================================
// Interview Validators
// ============================================================================

pub struct InterviewValidator;

impl Validator<CreateInterviewRequest> for InterviewValidator {
    fn validate(&self, data: &CreateInterviewRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        if data.application_id.trim().is_empty() {
            result.add_error("application_id", "Application ID is required");
        }

        if data.scheduled_date.trim().is_empty() {
            result.add_error("scheduled_date", "Scheduled date is required");
        }

        if data.duration_minutes <= 0 {
            result.add_error("duration_minutes", "Duration must be greater than 0");
        }

        if data.duration_minutes > 480 {
            result.add_error(
                "duration_minutes",
                "Duration must not exceed 480 minutes (8 hours)",
            );
        }

        if data.interview_type.trim().is_empty() {
            result.add_error("interview_type", "Interview type is required");
        }

        // Panel members are optional, but if provided, validate them
        for (idx, member) in data.panel_members.iter().enumerate() {
            if member.email.trim().is_empty() {
                result.add_error(
                    &format!("panel_members[{}].email", idx),
                    "Panel member email is required",
                );
            } else if !member.email.contains('@') {
                result.add_error(
                    &format!("panel_members[{}].email", idx),
                    "Panel member email must be valid",
                );
            }
        }

        result
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_date_format(date_str: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
}

/// Validates ID format - accepts both UUID and custom prefixed IDs (e.g., J_XXXXXX, R_XXXXXX)
fn is_valid_uuid(id_str: &str) -> bool {
    // Accept standard UUIDs
    if uuid::Uuid::parse_str(id_str).is_ok() {
        return true;
    }
    
    // Accept custom prefixed IDs (format: X_XXXXXX where X is a letter and XXXXXX is Crockford Base32)
    if id_str.len() >= 3 && id_str.chars().nth(1) == Some('_') {
        let prefix = id_str.chars().next().unwrap();
        let suffix = &id_str[2..];
        
        // Valid prefixes: J (job), R (resume), C (candidate), A (application), U (user), etc.
        let valid_prefixes = ['J', 'R', 'C', 'A', 'U', 'I', 'M', 'V', 'T', 'E', 'X', 'S', 'P', 'H', 'F', 'G', 'K', 'W', 'N'];
        if valid_prefixes.contains(&prefix) && !suffix.is_empty() {
            // Crockford Base32 alphabet (excludes I, L, O, U)
            let crockford_chars = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
            return suffix.chars().all(|c| crockford_chars.contains(c.to_ascii_uppercase()));
        }
    }
    
    false
}

// Stage validation
pub fn validate_stage_transition(current_stage: &str, new_stage: &str) -> Result<(), String> {
    let valid_stages = vec![
        "Applied",
        "Resume Review",
        "Shortlisted",
        "Interview Scheduled",
        "Interview Completed",
        "Offer Extended",
        "Hired",
        "Rejected",
    ];

    if !valid_stages.contains(&new_stage) {
        return Err(format!("Invalid stage: {}", new_stage));
    }

    if new_stage == "Rejected" {
        return Ok(());
    }

    if new_stage == "Hired" && current_stage != "Offer Extended" {
        return Err("Can only move to Hired from Offer Extended stage".to_string());
    }

    Ok(())
}
