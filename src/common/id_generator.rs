// src/common/id_generator.rs
//! Crockford Base32 ID Generator
//!
//! Generates human-readable, prefixed IDs using Crockford Base32 encoding.
//! Format: PREFIX_XXXXXX (e.g., J_K7NP3X for jobs)
//!
//! Benefits:
//! - No ambiguous characters (excludes I, L, O, U)
//! - Case-insensitive
//! - ~1 billion combinations per entity type (32^6)
//! - Easy to read, type, and communicate verbally

use rand::Rng;

/// Crockford Base32 alphabet (excludes I, L, O, U to avoid confusion)
const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Entity type prefixes for ID generation
#[derive(Debug, Clone, Copy)]
pub enum EntityPrefix {
    /// Job posting (J_)
    Job,
    /// Resume (R_)
    Resume,
    /// Company (C_)
    Company,
    /// Application (A_)
    Application,
    /// User (U_)
    User,
    /// Interview (I_)
    Interview,
    /// Message (M_)
    Message,
    /// Video (V_)
    Video,
    /// Template (T_)
    Template,
    /// Education (E_)
    Education,
    /// Experience (X_)
    Experience,
    /// Testimonial (S_) - S for Story/Statement
    Testimonial,
    /// Panelist (P_)
    Panelist,
    /// History/Audit (H_)
    History,
    /// Attachment (F_) - F for File
    Attachment,
    /// Asset (G_) - G for Graphic/General asset
    Asset,
    /// Token (K_) - K for Key
    Token,
    /// View (W_) - W for Watch/View
    View,
    /// Connection (N_) - N for Network connection
    Connection,
    /// ContentVersion (CV_) - Content version for inline AI editor
    ContentVersion,
}

impl EntityPrefix {
    /// Get the string prefix for this entity type
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityPrefix::Job => "J",
            EntityPrefix::Resume => "R",
            EntityPrefix::Company => "C",
            EntityPrefix::Application => "A",
            EntityPrefix::User => "U",
            EntityPrefix::Interview => "I",
            EntityPrefix::Message => "M",
            EntityPrefix::Video => "V",
            EntityPrefix::Template => "T",
            EntityPrefix::Education => "E",
            EntityPrefix::Experience => "X",
            EntityPrefix::Testimonial => "S",
            EntityPrefix::Panelist => "P",
            EntityPrefix::History => "H",
            EntityPrefix::Attachment => "F",
            EntityPrefix::Asset => "G",
            EntityPrefix::Token => "K",
            EntityPrefix::View => "W",
            EntityPrefix::Connection => "N",
            EntityPrefix::ContentVersion => "CV",
        }
    }
}

/// Generate a random Crockford Base32 string of specified length
fn generate_crockford_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..32);
            CROCKFORD_ALPHABET[idx] as char
        })
        .collect()
}

/// Generate a prefixed ID using Crockford Base32 encoding
///
/// # Arguments
/// * `prefix` - The entity type prefix
///
/// # Returns
/// A string in format "PREFIX_XXXXXX" (e.g., "J_K7NP3X")
///
/// # Example
/// ```
/// use crate::common::id_generator::{generate_id, EntityPrefix};
///
/// let job_id = generate_id(EntityPrefix::Job);
/// // Returns something like "J_K7NP3X"
///
/// let resume_id = generate_id(EntityPrefix::Resume);
/// // Returns something like "R_8MWQT2"
/// ```
pub fn generate_id(prefix: EntityPrefix) -> String {
    format!("{}_{}", prefix.as_str(), generate_crockford_string(6))
}

/// Generate a prefixed ID with custom length
///
/// # Arguments
/// * `prefix` - The entity type prefix
/// * `length` - Number of random characters (default is 6)
///
/// # Example
/// ```
/// let long_id = generate_id_with_length(EntityPrefix::Job, 8);
/// // Returns something like "J_K7NP3XY2"
/// ```
#[allow(dead_code)]
pub fn generate_id_with_length(prefix: EntityPrefix, length: usize) -> String {
    format!("{}_{}", prefix.as_str(), generate_crockford_string(length))
}

/// Generate a raw Crockford Base32 string without prefix
/// Useful for filenames or other non-entity identifiers
///
/// # Arguments
/// * `length` - Number of random characters
///
/// # Example
/// ```
/// let random_str = generate_raw_id(8);
/// // Returns something like "K7NP3XY2"
/// ```
pub fn generate_raw_id(length: usize) -> String {
    generate_crockford_string(length)
}

// ============================================================================
// Convenience functions for each entity type
// ============================================================================

/// Generate a Job ID (J_XXXXXX)
pub fn generate_job_id() -> String {
    generate_id(EntityPrefix::Job)
}

/// Generate a Resume ID (R_XXXXXX)
pub fn generate_resume_id() -> String {
    generate_id(EntityPrefix::Resume)
}

/// Generate a Company ID (C_XXXXXX)
pub fn generate_company_id() -> String {
    generate_id(EntityPrefix::Company)
}

/// Generate an Application ID (A_XXXXXX)
pub fn generate_application_id() -> String {
    generate_id(EntityPrefix::Application)
}

/// Generate a User ID (U_XXXXXX)
pub fn generate_user_id() -> String {
    generate_id(EntityPrefix::User)
}

/// Generate an Interview ID (I_XXXXXX)
pub fn generate_interview_id() -> String {
    generate_id(EntityPrefix::Interview)
}

/// Generate a Message ID (M_XXXXXX)
pub fn generate_message_id() -> String {
    generate_id(EntityPrefix::Message)
}

/// Generate a Video ID (V_XXXXXX)
pub fn generate_video_id() -> String {
    generate_id(EntityPrefix::Video)
}

/// Generate a Template ID (T_XXXXXX)
pub fn generate_template_id() -> String {
    generate_id(EntityPrefix::Template)
}

/// Generate an Education ID (E_XXXXXX)
pub fn generate_education_id() -> String {
    generate_id(EntityPrefix::Education)
}

/// Generate an Experience ID (X_XXXXXX)
pub fn generate_experience_id() -> String {
    generate_id(EntityPrefix::Experience)
}

/// Generate a Testimonial ID (S_XXXXXX)
pub fn generate_testimonial_id() -> String {
    generate_id(EntityPrefix::Testimonial)
}

/// Generate a Panelist ID (P_XXXXXX)
pub fn generate_panelist_id() -> String {
    generate_id(EntityPrefix::Panelist)
}

/// Generate a History/Audit ID (H_XXXXXX)
pub fn generate_history_id() -> String {
    generate_id(EntityPrefix::History)
}

/// Generate an Attachment ID (F_XXXXXX)
pub fn generate_attachment_id() -> String {
    generate_id(EntityPrefix::Attachment)
}

/// Generate an Asset ID (G_XXXXXX)
pub fn generate_asset_id() -> String {
    generate_id(EntityPrefix::Asset)
}

/// Generate a Token ID (K_XXXXXX)
pub fn generate_token_id() -> String {
    generate_id(EntityPrefix::Token)
}

/// Generate a View ID (W_XXXXXX)
pub fn generate_view_id() -> String {
    generate_id(EntityPrefix::View)
}

/// Generate a Connection ID (N_XXXXXX)
pub fn generate_connection_id() -> String {
    generate_id(EntityPrefix::Connection)
}

/// Generate a Content Version ID (CV_XXXXXX)
pub fn generate_content_version_id() -> String {
    generate_id(EntityPrefix::ContentVersion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_generate_id_format() {
        let job_id = generate_job_id();
        assert!(job_id.starts_with("J_"));
        assert_eq!(job_id.len(), 8); // "J_" + 6 chars

        let resume_id = generate_resume_id();
        assert!(resume_id.starts_with("R_"));
        assert_eq!(resume_id.len(), 8);
    }

    #[test]
    fn test_crockford_alphabet_only() {
        let id = generate_job_id();
        let random_part = &id[2..]; // Skip "J_"

        for c in random_part.chars() {
            assert!(
                CROCKFORD_ALPHABET.contains(&(c as u8)),
                "Character '{}' not in Crockford alphabet",
                c
            );
        }

        // Verify no ambiguous characters
        assert!(!random_part.contains('I'));
        assert!(!random_part.contains('L'));
        assert!(!random_part.contains('O'));
        assert!(!random_part.contains('U'));
    }

    #[test]
    fn test_uniqueness() {
        let mut ids = HashSet::new();
        for _ in 0..1000 {
            let id = generate_job_id();
            assert!(ids.insert(id), "Duplicate ID generated");
        }
    }

    #[test]
    fn test_all_prefixes() {
        assert!(generate_job_id().starts_with("J_"));
        assert!(generate_resume_id().starts_with("R_"));
        assert!(generate_company_id().starts_with("C_"));
        assert!(generate_application_id().starts_with("A_"));
        assert!(generate_user_id().starts_with("U_"));
        assert!(generate_interview_id().starts_with("I_"));
        assert!(generate_message_id().starts_with("M_"));
        assert!(generate_video_id().starts_with("V_"));
        assert!(generate_template_id().starts_with("T_"));
        assert!(generate_education_id().starts_with("E_"));
        assert!(generate_experience_id().starts_with("X_"));
        assert!(generate_testimonial_id().starts_with("S_"));
        assert!(generate_panelist_id().starts_with("P_"));
        assert!(generate_history_id().starts_with("H_"));
        assert!(generate_attachment_id().starts_with("F_"));
        assert!(generate_asset_id().starts_with("G_"));
        assert!(generate_token_id().starts_with("K_"));
        assert!(generate_view_id().starts_with("W_"));
        assert!(generate_connection_id().starts_with("N_"));
    }

    #[test]
    fn test_custom_length() {
        let id = generate_id_with_length(EntityPrefix::Job, 10);
        assert!(id.starts_with("J_"));
        assert_eq!(id.len(), 12); // "J_" + 10 chars
    }

    #[test]
    fn test_raw_id() {
        let raw = generate_raw_id(8);
        assert_eq!(raw.len(), 8);
        assert!(!raw.contains('_')); // No prefix separator
    }
}
