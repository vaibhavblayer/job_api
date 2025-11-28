// src/jobs/validators.rs

use super::models::*;
use crate::common::{ValidationResult, Validator};
use chrono::NaiveDate;
use std::collections::HashSet;

// ============================================================================
// Job Validators
// ============================================================================

pub struct JobValidator;

impl Validator<CreateJob> for JobValidator {
    fn validate(&self, data: &CreateJob) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate title
        if data.title.trim().is_empty() {
            result.add_error("title", "Job title is required");
        } else if data.title.len() > 255 {
            result.add_error("title", "Job title must be less than 255 characters");
        }

        // Validate description length if provided
        if let Some(description) = &data.description {
            if description.len() > 10000 {
                result.add_error(
                    "description",
                    "Description must be less than 10000 characters",
                );
            }
        }

        // Validate location length if provided
        if let Some(location) = &data.location {
            if location.len() > 255 {
                result.add_error("location", "Location must be less than 255 characters");
            }
        }

        // Validate company length if provided
        if let Some(company) = &data.company {
            if company.len() > 255 {
                result.add_error("company", "Company name must be less than 255 characters");
            }
        }

        // Validate salary range
        if let (Some(min), Some(max)) = (data.salary_min, data.salary_max) {
            if min < 0 {
                result.add_error("salary_min", "Minimum salary cannot be negative");
            }
            if max < 0 {
                result.add_error("salary_max", "Maximum salary cannot be negative");
            }
            if min > max {
                result.add_error(
                    "salary_range",
                    "Minimum salary cannot be greater than maximum salary",
                );
            }
        }

        // Validate job_type if provided
        if let Some(job_type) = &data.job_type {
            let valid_types = HashSet::from([
                "full-time",
                "part-time",
                "contract",
                "temporary",
                "internship",
            ]);
            if !valid_types.contains(job_type.as_str()) {
                result.add_error("job_type", "Invalid job type");
            }
        }

        // Validate experience_level if provided
        if let Some(level) = &data.experience_level {
            let valid_levels = HashSet::from(["entry", "mid", "senior", "lead", "executive"]);
            if !valid_levels.contains(level.as_str()) {
                result.add_error("experience_level", "Invalid experience level");
            }
        }

        result
    }
}

// ============================================================================
// Bulk Operation Validators
// ============================================================================

pub struct BulkOperationValidator;

impl Validator<BulkJobStatusUpdate> for BulkOperationValidator {
    fn validate(&self, data: &BulkJobStatusUpdate) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate job_ids
        if data.job_ids.is_empty() {
            result.add_error("job_ids", "At least one job ID is required");
        } else if data.job_ids.len() > 100 {
            result.add_error("job_ids", "Cannot update more than 100 jobs at once");
        } else {
            for (index, job_id) in data.job_ids.iter().enumerate() {
                if !is_valid_uuid(job_id) {
                    result.add_error(&format!("job_ids[{}]", index), "Invalid job ID format");
                }
            }
        }

        // Validate status
        let valid_statuses = HashSet::from(["active", "inactive", "closed", "draft"]);
        if !valid_statuses.contains(data.status.as_str()) {
            result.add_error("status", "Invalid job status");
        }

        result
    }
}

// ============================================================================
// Job Analytics Validators
// ============================================================================

pub struct JobAnalyticsValidator;

impl Validator<JobAnalyticsRequest> for JobAnalyticsValidator {
    fn validate(&self, data: &JobAnalyticsRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate job_id if provided
        if let Some(job_id) = &data.job_id {
            if !is_valid_uuid(job_id) {
                result.add_error("job_id", "Job ID must be a valid UUID");
            }
        }

        // Validate start_date if provided
        if let Some(start_date) = &data.start_date {
            if let Err(_) = validate_date_format(start_date) {
                result.add_error("start_date", "Start date must be in YYYY-MM-DD format");
            }
        }

        // Validate end_date if provided
        if let Some(end_date) = &data.end_date {
            if let Err(_) = validate_date_format(end_date) {
                result.add_error("end_date", "End date must be in YYYY-MM-DD format");
            }
        }

        // Validate date range if both dates are provided
        if let (Some(start_date), Some(end_date)) = (&data.start_date, &data.end_date) {
            if let (Ok(start), Ok(end)) = (
                NaiveDate::parse_from_str(start_date, "%Y-%m-%d"),
                NaiveDate::parse_from_str(end_date, "%Y-%m-%d"),
            ) {
                if end < start {
                    result.add_error("date_range", "End date must be after start date");
                }

                // Limit date range to prevent performance issues
                let days_diff = (end - start).num_days();
                if days_diff > 365 {
                    result.add_error("date_range", "Date range cannot exceed 365 days");
                }
            }
        }

        result
    }
}

impl Validator<JobViewRequest> for JobAnalyticsValidator {
    fn validate(&self, data: &JobViewRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate user_agent length if provided
        if let Some(user_agent) = &data.user_agent {
            if user_agent.len() > 500 {
                result.add_error("user_agent", "User agent must be less than 500 characters");
            }
        }

        // Validate referrer length if provided
        if let Some(referrer) = &data.referrer {
            if referrer.len() > 500 {
                result.add_error("referrer", "Referrer must be less than 500 characters");
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

fn is_valid_uuid(uuid_str: &str) -> bool {
    uuid::Uuid::parse_str(uuid_str).is_ok()
}
