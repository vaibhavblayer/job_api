// src/profile/validators.rs

use super::models::*;
use crate::common::{ValidationResult, Validator};
use chrono::NaiveDate;

// ============================================================================
// Profile Management Validators
// ============================================================================

pub struct ExperienceValidator;

impl Validator<CreateExperienceRequest> for ExperienceValidator {
    fn validate(&self, data: &CreateExperienceRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate company
        if data.company.trim().is_empty() {
            result.add_error("company", "Company name is required");
        } else if data.company.len() > 255 {
            result.add_error("company", "Company name must be less than 255 characters");
        }

        // Validate title
        if data.title.trim().is_empty() {
            result.add_error("title", "Job title is required");
        } else if data.title.len() > 255 {
            result.add_error("title", "Job title must be less than 255 characters");
        }

        // Validate start_date
        if let Err(_) = validate_date_format(&data.start_date) {
            result.add_error("start_date", "Start date must be in YYYY-MM-DD format");
        }

        // Validate end_date if provided
        if let Some(end_date) = &data.end_date {
            if let Err(_) = validate_date_format(end_date) {
                result.add_error("end_date", "End date must be in YYYY-MM-DD format");
            } else {
                // Check if end_date is after start_date
                if let (Ok(start), Ok(end)) = (
                    NaiveDate::parse_from_str(&data.start_date, "%Y-%m-%d"),
                    NaiveDate::parse_from_str(end_date, "%Y-%m-%d"),
                ) {
                    if end < start {
                        result.add_error("end_date", "End date must be after start date");
                    }
                }
            }
        }

        // Validate description length if provided
        if let Some(description) = &data.description {
            if description.len() > 2000 {
                result.add_error(
                    "description",
                    "Description must be less than 2000 characters",
                );
            }
        }

        result
    }
}

impl Validator<UpdateExperienceRequest> for ExperienceValidator {
    fn validate(&self, data: &UpdateExperienceRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check if at least one field is provided
        if data.company.is_none()
            && data.title.is_none()
            && data.start_date.is_none()
            && data.end_date.is_none()
            && data.description.is_none()
        {
            result.add_error("general", "At least one field must be provided for update");
            return result;
        }

        // Validate company if provided
        if let Some(company) = &data.company {
            if company.trim().is_empty() {
                result.add_error("company", "Company name cannot be empty");
            } else if company.len() > 255 {
                result.add_error("company", "Company name must be less than 255 characters");
            }
        }

        // Validate title if provided
        if let Some(title) = &data.title {
            if title.trim().is_empty() {
                result.add_error("title", "Job title cannot be empty");
            } else if title.len() > 255 {
                result.add_error("title", "Job title must be less than 255 characters");
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

        // Validate description length if provided
        if let Some(description) = &data.description {
            if description.len() > 2000 {
                result.add_error(
                    "description",
                    "Description must be less than 2000 characters",
                );
            }
        }

        result
    }
}

pub struct EducationValidator;

impl Validator<CreateEducationRequest> for EducationValidator {
    fn validate(&self, data: &CreateEducationRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate institution
        if data.institution.trim().is_empty() {
            result.add_error("institution", "Institution name is required");
        } else if data.institution.len() > 255 {
            result.add_error(
                "institution",
                "Institution name must be less than 255 characters",
            );
        }

        // Validate degree
        if data.degree.trim().is_empty() {
            result.add_error("degree", "Degree is required");
        } else if data.degree.len() > 255 {
            result.add_error("degree", "Degree must be less than 255 characters");
        }

        // Validate field_of_study if provided
        if let Some(field) = &data.field_of_study {
            if field.len() > 255 {
                result.add_error(
                    "field_of_study",
                    "Field of study must be less than 255 characters",
                );
            }
        }

        // Validate start_date
        if let Err(_) = validate_date_format(&data.start_date) {
            result.add_error("start_date", "Start date must be in YYYY-MM-DD format");
        }

        // Validate end_date if provided
        if let Some(end_date) = &data.end_date {
            if let Err(_) = validate_date_format(end_date) {
                result.add_error("end_date", "End date must be in YYYY-MM-DD format");
            } else {
                // Check if end_date is after start_date
                if let (Ok(start), Ok(end)) = (
                    NaiveDate::parse_from_str(&data.start_date, "%Y-%m-%d"),
                    NaiveDate::parse_from_str(end_date, "%Y-%m-%d"),
                ) {
                    if end < start {
                        result.add_error("end_date", "End date must be after start date");
                    }
                }
            }
        }

        // Validate description length if provided
        if let Some(description) = &data.description {
            if description.len() > 2000 {
                result.add_error(
                    "description",
                    "Description must be less than 2000 characters",
                );
            }
        }

        result
    }
}

impl Validator<UpdateEducationRequest> for EducationValidator {
    fn validate(&self, data: &UpdateEducationRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check if at least one field is provided
        if data.institution.is_none()
            && data.degree.is_none()
            && data.field_of_study.is_none()
            && data.start_date.is_none()
            && data.end_date.is_none()
            && data.description.is_none()
        {
            result.add_error("general", "At least one field must be provided for update");
            return result;
        }

        // Validate institution if provided
        if let Some(institution) = &data.institution {
            if institution.trim().is_empty() {
                result.add_error("institution", "Institution name cannot be empty");
            } else if institution.len() > 255 {
                result.add_error(
                    "institution",
                    "Institution name must be less than 255 characters",
                );
            }
        }

        // Validate degree if provided
        if let Some(degree) = &data.degree {
            if degree.trim().is_empty() {
                result.add_error("degree", "Degree cannot be empty");
            } else if degree.len() > 255 {
                result.add_error("degree", "Degree must be less than 255 characters");
            }
        }

        // Validate field_of_study if provided
        if let Some(field) = &data.field_of_study {
            if field.len() > 255 {
                result.add_error(
                    "field_of_study",
                    "Field of study must be less than 255 characters",
                );
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

        // Validate description length if provided
        if let Some(description) = &data.description {
            if description.len() > 2000 {
                result.add_error(
                    "description",
                    "Description must be less than 2000 characters",
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
