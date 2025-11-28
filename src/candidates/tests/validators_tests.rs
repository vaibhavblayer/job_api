// src/candidates/tests/validators_tests.rs

#[cfg(test)]
mod tests {
    use crate::candidates::models::*;
    use crate::candidates::validators::*;
    use crate::common::{ValidationResult, Validator};

    #[test]
    fn test_application_validator_valid_request() {
        let validator = ApplicationValidator;
        let request = CreateApplicationRequest {
            job_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            resume_id: None,
            cover_letter: Some("Test cover letter".to_string()),
        };

        let result = validator.validate(&request);
        assert!(result.is_valid);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_application_validator_invalid_job_id() {
        let validator = ApplicationValidator;
        let request = CreateApplicationRequest {
            job_id: "invalid-uuid".to_string(),
            resume_id: None,
            cover_letter: None,
        };

        let result = validator.validate(&request);
        assert!(!result.is_valid);
        assert!(result.errors.len() > 0);
    }

    #[test]
    fn test_application_validator_cover_letter_too_long() {
        let validator = ApplicationValidator;
        let request = CreateApplicationRequest {
            job_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            resume_id: None,
            cover_letter: Some("a".repeat(5001)),
        };

        let result = validator.validate(&request);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_stage_transition_validation() {
        // Test valid transition
        let result = validate_stage_transition("Applied", "Resume Review");
        assert!(result.is_ok());

        // Test rejection from any stage
        let result = validate_stage_transition("Applied", "Rejected");
        assert!(result.is_ok());

        // Test invalid transition to Hired
        let result = validate_stage_transition("Applied", "Hired");
        assert!(result.is_err());

        // Test valid transition to Hired
        let result = validate_stage_transition("Offer Extended", "Hired");
        assert!(result.is_ok());
    }
}
