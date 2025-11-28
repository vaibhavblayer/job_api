// src/jobs/tests/validators_tests.rs

#[cfg(test)]
mod tests {
    use crate::common::Validator;
    use crate::jobs::models::*;
    use crate::jobs::validators::*;

    #[test]
    fn test_job_validator_valid_data() {
        let validator = JobValidator;
        let request = CreateJob {
            title: "Software Engineer".to_string(),
            description: Some("Test description".to_string()),
            location: Some("Remote".to_string()),
            company: Some("Test Company".to_string()),
            company_id: None,
            company_logo_url: None,
            job_image_url: None,
            salary_min: Some(50000),
            salary_max: Some(100000),
            job_type: Some("full-time".to_string()),
            experience_level: Some("mid".to_string()),
            requirements: Some(vec!["Rust".to_string(), "SQL".to_string()]),
            benefits: Some(vec!["Health Insurance".to_string()]),
            educational_qualifications: None,
            is_featured: Some(false),
            template_id: None,
            status: Some("draft".to_string()),
        };

        let result = validator.validate(&request);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_job_validator_invalid_title() {
        let validator = JobValidator;
        let request = CreateJob {
            title: "".to_string(), // Empty title
            description: None,
            location: None,
            company: None,
            company_id: None,
            company_logo_url: None,
            job_image_url: None,
            salary_min: None,
            salary_max: None,
            job_type: None,
            experience_level: None,
            requirements: None,
            benefits: None,
            educational_qualifications: None,
            is_featured: None,
            template_id: None,
            status: None,
        };

        let result = validator.validate(&request);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "title"));
    }

    #[test]
    fn test_bulk_operation_validator_too_many_jobs() {
        let validator = BulkOperationValidator;
        let request = BulkJobStatusUpdate {
            job_ids: (0..101)
                .map(|_| "550e8400-e29b-41d4-a716-446655440000".to_string())
                .collect(),
            status: "active".to_string(),
        };

        let result = validator.validate(&request);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "job_ids"));
    }
}
