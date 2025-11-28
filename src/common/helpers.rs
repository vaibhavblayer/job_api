// Helper functions for safe logging and serialization

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Masks email addresses for safe logging
/// Prevents sensitive data exposure while preserving debugging utility
///
/// # Example
/// ```
/// let masked = safe_email_log("user@example.com");
/// // Returns: "u***@example.com"
/// ```
pub fn safe_email_log(email: &str) -> String {
    if email.len() > 3 {
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() == 2 {
            format!("{}***@{}", &parts[0][..1.min(parts[0].len())], parts[1])
        } else {
            "***@***.***".to_string()
        }
    } else {
        "***@***.***".to_string()
    }
}

/// Masks tokens for safe logging
/// Shows only first and last 4 characters
///
/// # Example
/// ```
/// let masked = safe_token_log("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
/// // Returns: "eyJh...kpXVCJ9"
/// ```
#[allow(dead_code)]
pub fn safe_token_log(token: &str) -> String {
    if token.len() > 8 {
        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    } else {
        "***".to_string()
    }
}

/// Serializes skills from JSON string to array for API responses
pub fn serialize_skills<S>(skills: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match skills {
        Some(skills_json) => {
            let skills_vec: Vec<String> =
                serde_json::from_str(skills_json).unwrap_or_else(|_| Vec::new());
            skills_vec.serialize(serializer)
        }
        None => Vec::<String>::new().serialize(serializer),
    }
}

/// Deserializes skills from array to JSON string for database storage
pub fn deserialize_skills<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let skills_vec: Vec<String> = Vec::deserialize(deserializer)?;
    let skills_json = serde_json::to_string(&skills_vec).map_err(serde::de::Error::custom)?;
    Ok(Some(skills_json))
}
