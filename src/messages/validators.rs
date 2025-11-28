use crate::common::error::ApiError;

/// Validate message content
pub fn validate_message_content(content: &str) -> Result<(), ApiError> {
    let trimmed = content.trim();

    if trimmed.is_empty() {
        return Err(ApiError::ValidationError(
            "Message cannot be empty".to_string(),
        ));
    }

    if trimmed.len() > 10000 {
        return Err(ApiError::ValidationError(
            "Message exceeds maximum length of 10000 characters".to_string(),
        ));
    }

    Ok(())
}

/// Validate file attachment
pub fn validate_attachment(
    filename: &str,
    mime_type: &str,
    file_size: usize,
) -> Result<(), ApiError> {
    // Check filename
    if filename.is_empty() {
        return Err(ApiError::ValidationError(
            "Filename cannot be empty".to_string(),
        ));
    }

    if filename.len() > 255 {
        return Err(ApiError::ValidationError(
            "Filename exceeds maximum length of 255 characters".to_string(),
        ));
    }

    // Check file size (10MB limit)
    const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;
    if file_size > MAX_FILE_SIZE {
        return Err(ApiError::ValidationError(
            "File size exceeds 10MB limit".to_string(),
        ));
    }

    if file_size == 0 {
        return Err(ApiError::ValidationError(
            "File cannot be empty".to_string(),
        ));
    }

    // Check file type
    let allowed_types = [
        "application/pdf",
        "application/msword",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "text/plain",
        "image/jpeg",
        "image/jpg",
        "image/png",
        "image/gif",
        "image/webp",
    ];

    if !allowed_types.contains(&mime_type) {
        return Err(ApiError::ValidationError(format!(
            "File type '{}' is not supported. Allowed types: PDF, DOCX, TXT, PNG, JPG, JPEG, GIF, WEBP",
            mime_type
        )));
    }

    Ok(())
}

/// Validate file type by checking magic bytes
pub fn validate_file_content(data: &[u8], declared_mime_type: &str) -> Result<(), ApiError> {
    let infer = infer::Infer::new();

    if let Some(info) = infer.get(data) {
        let actual_mime = info.mime_type();

        // Allow some flexibility for JPEG (jpg vs jpeg)
        let is_jpeg_variant = (declared_mime_type == "image/jpeg"
            || declared_mime_type == "image/jpg")
            && (actual_mime == "image/jpeg" || actual_mime == "image/jpg");

        if actual_mime != declared_mime_type && !is_jpeg_variant {
            return Err(ApiError::ValidationError(format!(
                "File content does not match declared type. Expected: {}, Actual: {}",
                declared_mime_type, actual_mime
            )));
        }
    }

    Ok(())
}

/// Sanitize filename to prevent path traversal and other security issues
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect::<String>()
        .trim_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_message_content() {
        // Valid message
        assert!(validate_message_content("Hello, world!").is_ok());

        // Empty message
        assert!(validate_message_content("").is_err());
        assert!(validate_message_content("   ").is_err());

        // Too long message
        let long_message = "a".repeat(10001);
        assert!(validate_message_content(&long_message).is_err());
    }

    #[test]
    fn test_validate_attachment() {
        // Valid attachment
        assert!(validate_attachment("test.pdf", "application/pdf", 1024).is_ok());

        // Empty filename
        assert!(validate_attachment("", "application/pdf", 1024).is_err());

        // File too large
        assert!(validate_attachment("test.pdf", "application/pdf", 11 * 1024 * 1024).is_err());

        // Empty file
        assert!(validate_attachment("test.pdf", "application/pdf", 0).is_err());

        // Invalid file type
        assert!(validate_attachment("test.exe", "application/x-msdownload", 1024).is_err());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test.pdf"), "test.pdf");
        assert_eq!(sanitize_filename("../../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_filename("test file.pdf"), "testfile.pdf");
        assert_eq!(sanitize_filename("test@#$%file.pdf"), "testfile.pdf");
    }
}
