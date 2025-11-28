//! Tests for companies module
//!
//! These tests verify core company functionality including:
//! - Company model structure
//! - Company validation
//! - Asset type validation

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::common::{ValidationResult, Validator};

    #[test]
    fn test_company_model_structure() {
        // Test that Company model can be created
        let company = models::Company {
            id: "test-id".to_string(),
            name: "Test Company".to_string(),
            description: Some("A test company".to_string()),
            website: Some("https://example.com".to_string()),
            industry: Some("Technology".to_string()),
            company_size: Some("11-50".to_string()),
            founded_year: Some(2020),
            headquarters: Some(r#"{"city": "San Francisco", "state": "CA"}"#.to_string()),
            operating_locations: Some(r#"[{"city": "NYC", "state": "NY"}]"#.to_string()),
            culture: Some(r#"{"values": ["Innovation"]}"#.to_string()),
            benefits: Some(r#"["Health Insurance", "401k"]"#.to_string()),
            default_logo_url: Some("/logo.png".to_string()),
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
        };

        assert_eq!(company.name, "Test Company");
        assert_eq!(company.industry, Some("Technology".to_string()));
    }

    #[test]
    fn test_company_asset_model_structure() {
        // Test that CompanyAsset model can be created
        let asset = models::CompanyAsset {
            id: "asset-id".to_string(),
            company_id: "company-id".to_string(),
            asset_type: "logo".to_string(),
            url: "/logo.png".to_string(),
            filename: "logo.png".to_string(),
            file_size: 1024,
            mime_type: "image/png".to_string(),
            is_default: 1,
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
        };

        assert_eq!(asset.asset_type, "logo");
        assert_eq!(asset.is_default, 1);
    }

    #[test]
    fn test_create_company_validation_success() {
        // Test valid company creation request
        let request = models::CreateCompanyRequest {
            name: "Valid Company".to_string(),
            description: Some("Description".to_string()),
            website: Some("https://example.com".to_string()),
            industry: Some("Tech".to_string()),
            company_size: Some("11-50".to_string()),
            founded_year: Some(2020),
            headquarters: None,
            operating_locations: None,
            culture: None,
            benefits: None,
            default_logo_url: None,
        };

        let result = request.validate(&request);
        assert!(result.is_valid, "Valid company should pass validation");
    }

    #[test]
    fn test_create_company_validation_empty_name() {
        // Test that empty name fails validation
        let request = models::CreateCompanyRequest {
            name: "".to_string(),
            description: None,
            website: None,
            industry: None,
            company_size: None,
            founded_year: None,
            headquarters: None,
            operating_locations: None,
            culture: None,
            benefits: None,
            default_logo_url: None,
        };

        let result = request.validate(&request);
        assert!(!result.is_valid, "Empty name should fail validation");
        assert!(result.errors.iter().any(|e| e.field == "name"));
    }

    #[test]
    fn test_create_company_validation_name_too_long() {
        // Test that name exceeding 255 characters fails validation
        let long_name = "a".repeat(256);
        let request = models::CreateCompanyRequest {
            name: long_name,
            description: None,
            website: None,
            industry: None,
            company_size: None,
            founded_year: None,
            headquarters: None,
            operating_locations: None,
            culture: None,
            benefits: None,
            default_logo_url: None,
        };

        let result = request.validate(&request);
        assert!(
            !result.is_valid,
            "Name over 255 chars should fail validation"
        );
        assert!(result.errors.iter().any(|e| e.field == "name"));
    }

    #[test]
    fn test_create_company_validation_invalid_website() {
        // Test that invalid website URL fails validation
        let request = models::CreateCompanyRequest {
            name: "Test Company".to_string(),
            description: None,
            website: Some("not-a-url".to_string()),
            industry: None,
            company_size: None,
            founded_year: None,
            headquarters: None,
            operating_locations: None,
            culture: None,
            benefits: None,
            default_logo_url: None,
        };

        let result = request.validate(&request);
        assert!(
            !result.is_valid,
            "Invalid website URL should fail validation"
        );
        assert!(result.errors.iter().any(|e| e.field == "website"));
    }

    #[test]
    fn test_validate_asset_type_logo() {
        // Test that 'logo' is a valid asset type
        let result = validators::validate_asset_type("logo");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_asset_type_image() {
        // Test that 'image' is a valid asset type
        let result = validators::validate_asset_type("image");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_asset_type_invalid() {
        // Test that invalid asset type fails validation
        let result = validators::validate_asset_type("document");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Asset type must be 'logo' or 'image'");
    }

    #[test]
    fn test_validate_image_mime_type_png() {
        // Test that PNG mime type is valid
        let result = validators::validate_image_mime_type("image/png");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_image_mime_type_jpeg() {
        // Test that JPEG mime type is valid
        let result = validators::validate_image_mime_type("image/jpeg");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_image_mime_type_invalid() {
        // Test that non-image mime type fails validation
        let result = validators::validate_image_mime_type("application/pdf");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Only image files are allowed");
    }

    #[test]
    fn test_save_url_as_asset_request_structure() {
        // Test SaveUrlAsAssetRequest model structure
        let request = models::SaveUrlAsAssetRequest {
            url: "/logo.png".to_string(),
            asset_type: "logo".to_string(),
            is_default: Some(true),
        };

        assert_eq!(request.url, "/logo.png");
        assert_eq!(request.asset_type, "logo");
        assert_eq!(request.is_default, Some(true));
    }

    #[test]
    fn test_message_response_structure() {
        // Test MessageResponse model structure
        let response = models::MessageResponse {
            message: "Success".to_string(),
        };

        assert_eq!(response.message, "Success");
    }
}
