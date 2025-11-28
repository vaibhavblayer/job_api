use super::models::CreateCompanyRequest;
use crate::common::{ValidationResult, Validator};

impl Validator<CreateCompanyRequest> for CreateCompanyRequest {
    fn validate(&self, data: &CreateCompanyRequest) -> ValidationResult {
        let mut result = ValidationResult::new();

        if data.name.trim().is_empty() {
            result.add_error("name", "Company name is required");
        }

        if data.name.len() > 255 {
            result.add_error("name", "Company name must not exceed 255 characters");
        }

        if let Some(website) = &data.website {
            if !website.is_empty()
                && !website.starts_with("http://")
                && !website.starts_with("https://")
            {
                result.add_error(
                    "website",
                    "Website must be a valid URL starting with http:// or https://",
                );
            }
        }

        result
    }
}

/// Validates asset type
pub fn validate_asset_type(asset_type: &str) -> Result<(), String> {
    if asset_type != "logo" && asset_type != "image" {
        return Err("Asset type must be 'logo' or 'image'".to_string());
    }
    Ok(())
}

/// Validates file is an image
pub fn validate_image_mime_type(mime_type: &str) -> Result<(), String> {
    if !mime_type.starts_with("image/") {
        return Err("Only image files are allowed".to_string());
    }
    Ok(())
}
