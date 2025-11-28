//! Tests for profile module
//!
//! These tests verify core profile functionality including:
//! - Profile models structure
//! - Experience and Education validators
//! - Testimonial models

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::common::{ValidationResult, Validator};

    // ============================================================================
    // Model Tests
    // ============================================================================

    #[test]
    fn test_experience_model_structure() {
        // Test that Experience model can be created
        let experience = models::Experience {
            id: "test-id".to_string(),
            user_id: "user-123".to_string(),
            company: "Test Company".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2020-01-01".to_string(),
            end_date: Some("2022-12-31".to_string()),
            description: Some("Test description".to_string()),
            created_at: None,
            updated_at: None,
        };

        assert_eq!(experience.company, "Test Company");
        assert_eq!(experience.title, "Software Engineer");
    }

    #[test]
    fn test_education_model_structure() {
        // Test that Education model can be created
        let education = models::Education {
            id: "test-id".to_string(),
            user_id: "user-123".to_string(),
            institution: "Test University".to_string(),
            degree: "Bachelor of Science".to_string(),
            field_of_study: Some("Computer Science".to_string()),
            start_date: "2016-09-01".to_string(),
            end_date: Some("2020-05-31".to_string()),
            description: None,
            created_at: None,
            updated_at: None,
        };

        assert_eq!(education.institution, "Test University");
        assert_eq!(education.degree, "Bachelor of Science");
    }

    // ============================================================================
    // Validator Tests
    // ============================================================================

    #[test]
    fn test_experience_validator_valid_data() {
        let request = models::CreateExperienceRequest {
            company: "Test Company".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2020-01-01".to_string(),
            end_date: Some("2022-12-31".to_string()),
            description: Some("Test description".to_string()),
        };

        let validator = validators::ExperienceValidator;
        let result = validator.validate(&request);

        assert!(
            result.is_valid,
            "Valid experience data should pass validation"
        );
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_experience_validator_missing_company() {
        let request = models::CreateExperienceRequest {
            company: "".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2020-01-01".to_string(),
            end_date: None,
            description: None,
        };

        let validator = validators::ExperienceValidator;
        let result = validator.validate(&request);

        assert!(!result.is_valid, "Empty company should fail validation");
        assert!(result.errors.iter().any(|e| e.field == "company"));
    }

    #[test]
    fn test_experience_validator_invalid_date_format() {
        let request = models::CreateExperienceRequest {
            company: "Test Company".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "invalid-date".to_string(),
            end_date: None,
            description: None,
        };

        let validator = validators::ExperienceValidator;
        let result = validator.validate(&request);

        assert!(
            !result.is_valid,
            "Invalid date format should fail validation"
        );
        assert!(result.errors.iter().any(|e| e.field == "start_date"));
    }

    #[test]
    fn test_education_validator_valid_data() {
        let request = models::CreateEducationRequest {
            institution: "Test University".to_string(),
            degree: "Bachelor of Science".to_string(),
            field_of_study: Some("Computer Science".to_string()),
            start_date: "2016-09-01".to_string(),
            end_date: Some("2020-05-31".to_string()),
            description: None,
        };

        let validator = validators::EducationValidator;
        let result = validator.validate(&request);

        assert!(
            result.is_valid,
            "Valid education data should pass validation"
        );
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_education_validator_missing_institution() {
        let request = models::CreateEducationRequest {
            institution: "".to_string(),
            degree: "Bachelor of Science".to_string(),
            field_of_study: None,
            start_date: "2016-09-01".to_string(),
            end_date: None,
            description: None,
        };

        let validator = validators::EducationValidator;
        let result = validator.validate(&request);

        assert!(!result.is_valid, "Empty institution should fail validation");
        assert!(result.errors.iter().any(|e| e.field == "institution"));
    }

    #[test]
    fn test_update_experience_validator_at_least_one_field() {
        let request = models::UpdateExperienceRequest {
            company: None,
            title: None,
            start_date: None,
            end_date: None,
            description: None,
        };

        let validator = validators::ExperienceValidator;
        let result = validator.validate(&request);

        assert!(
            !result.is_valid,
            "Update with no fields should fail validation"
        );
        assert!(result.errors.iter().any(|e| e.field == "general"));
    }

    #[test]
    fn test_update_education_validator_valid_partial_update() {
        let request = models::UpdateEducationRequest {
            institution: Some("Updated University".to_string()),
            degree: None,
            field_of_study: None,
            start_date: None,
            end_date: None,
            description: None,
        };

        let validator = validators::EducationValidator;
        let result = validator.validate(&request);

        assert!(
            result.is_valid,
            "Partial update with valid data should pass validation"
        );
        assert_eq!(result.errors.len(), 0);
    }

    // ============================================================================
    // Testimonial Model Tests
    // ============================================================================

    #[test]
    fn test_testimonial_model_structure() {
        let testimonial = models::Testimonial {
            id: "test-id".to_string(),
            user_id: "user-123".to_string(),
            content: "Great experience!".to_string(),
            rating: Some(5),
            position: Some("Software Engineer".to_string()),
            company: Some("Test Company".to_string()),
            featured: 1,
            approved: 1,
            created_at: None,
            updated_at: None,
        };

        assert_eq!(testimonial.content, "Great experience!");
        assert_eq!(testimonial.rating, Some(5));
        assert_eq!(testimonial.featured, 1);
        assert_eq!(testimonial.approved, 1);
    }

    #[test]
    fn test_create_testimonial_request() {
        let request = models::CreateTestimonialRequest {
            content: "Excellent service!".to_string(),
            rating: Some(5),
        };

        assert_eq!(request.content, "Excellent service!");
        assert_eq!(request.rating, Some(5));
    }
}
