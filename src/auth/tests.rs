//! Tests for auth module
//!
//! These tests verify core authentication functionality including:
//! - JWT token validation
//! - User model structure
//! - Claims structure

#[cfg(test)]
mod tests {
    use super::super::*;
    use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

    #[test]
    fn test_claims_structure() {
        // Test that Claims can be created and serialized
        let claims = models::Claims {
            sub: "test-user-id".to_string(),
            exp: 1234567890,
        };

        assert_eq!(claims.sub, "test-user-id");
        assert_eq!(claims.exp, 1234567890);
    }

    #[test]
    fn test_jwt_encoding_and_decoding() {
        // Test JWT token creation and validation
        let secret = "test_secret_key";
        let claims = models::Claims {
            sub: "test-user-123".to_string(),
            exp: 9999999999, // Far future
        };

        // Encode
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("Failed to encode token");

        // Decode
        let decoded = decode::<models::Claims>(
            &token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .expect("Failed to decode token");

        assert_eq!(decoded.claims.sub, "test-user-123");
        assert_eq!(decoded.claims.exp, 9999999999);
    }

    #[test]
    fn test_jwt_validation_fails_with_wrong_secret() {
        // Test that JWT validation fails with incorrect secret
        let secret = "test_secret_key";
        let wrong_secret = "wrong_secret_key";

        let claims = models::Claims {
            sub: "test-user-123".to_string(),
            exp: 9999999999,
        };

        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("Failed to encode token");

        // Try to decode with wrong secret
        let result = decode::<models::Claims>(
            &token,
            &DecodingKey::from_secret(wrong_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        );

        assert!(
            result.is_err(),
            "Token validation should fail with wrong secret"
        );
    }

    #[test]
    fn test_user_model_structure() {
        // Test User model can be created
        let user = models::User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            avatar: Some("http://example.com/avatar.jpg".to_string()),
            provider: Some("google".to_string()),
            provider_id: Some("google-123".to_string()),
            created_at: Some("2024-01-01".to_string()),
        };

        assert_eq!(user.id, "user-123");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.name, Some("Test User".to_string()));
        assert_eq!(user.provider, Some("google".to_string()));
    }

    #[test]
    fn test_google_id_token_payload_structure() {
        // Test GoogleIdTokenPayload can be created
        let payload = models::GoogleIdTokenPayload {
            id_token: "test_token_string".to_string(),
        };

        assert_eq!(payload.id_token, "test_token_string");
    }
}
