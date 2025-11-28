// src/services/encryption.rs
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption key not configured")]
    KeyNotConfigured,

    #[error("Invalid encryption key format")]
    InvalidKeyFormat,

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid encrypted data format")]
    InvalidDataFormat,
}

pub struct EncryptionService {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for EncryptionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionService")
            .field("cipher", &"<encrypted>")
            .finish()
    }
}

impl EncryptionService {
    /// Initialize encryption service from environment variable
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self, EncryptionError> {
        let key_str =
            env::var("ENCRYPTION_MASTER_KEY").map_err(|_| EncryptionError::KeyNotConfigured)?;

        Self::from_key(&key_str)
    }

    /// Initialize encryption service from a base64-encoded key string
    #[allow(deprecated)]
    pub fn from_key(key_str: &str) -> Result<Self, EncryptionError> {
        // Decode base64 key
        let key_bytes = BASE64
            .decode(key_str.as_bytes())
            .map_err(|_| EncryptionError::InvalidKeyFormat)?;

        // Ensure key is 32 bytes for AES-256
        if key_bytes.len() != 32 {
            return Err(EncryptionError::InvalidKeyFormat);
        }

        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        Ok(Self { cipher })
    }

    /// Generate a new random encryption key (base64-encoded)
    pub fn generate_key() -> String {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        BASE64.encode(key)
    }

    /// Encrypt a plaintext string and return base64-encoded ciphertext with nonce
    #[allow(deprecated)]
    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        // Generate random nonce (12 bytes for GCM)
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        // Combine nonce + ciphertext and encode as base64
        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(BASE64.encode(combined))
    }

    /// Decrypt a base64-encoded ciphertext (with nonce) and return plaintext
    #[allow(deprecated)]
    pub fn decrypt(&self, encrypted: &str) -> Result<String, EncryptionError> {
        // Decode base64
        let combined = BASE64
            .decode(encrypted.as_bytes())
            .map_err(|_| EncryptionError::InvalidDataFormat)?;

        // Split nonce and ciphertext
        if combined.len() < 12 {
            return Err(EncryptionError::InvalidDataFormat);
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt
        let plaintext_bytes = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

        // Convert to string
        String::from_utf8(plaintext_bytes)
            .map_err(|_| EncryptionError::DecryptionFailed("invalid UTF-8".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let key = EncryptionService::generate_key();
        assert!(!key.is_empty());

        // Should be able to create service from generated key
        let service = EncryptionService::from_key(&key);
        assert!(service.is_ok());
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = EncryptionService::generate_key();
        let service = EncryptionService::from_key(&key).unwrap();

        let plaintext = "sensitive_api_key_12345";
        let encrypted = service.encrypt(plaintext).unwrap();

        // Encrypted should be different from plaintext
        assert_ne!(encrypted, plaintext);

        // Should decrypt back to original
        let decrypted = service.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let key = EncryptionService::generate_key();
        let service = EncryptionService::from_key(&key).unwrap();

        let plaintext = "test_data";
        let encrypted1 = service.encrypt(plaintext).unwrap();
        let encrypted2 = service.encrypt(plaintext).unwrap();

        // Same plaintext should produce different ciphertext due to random nonce
        assert_ne!(encrypted1, encrypted2);

        // Both should decrypt to same plaintext
        assert_eq!(service.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(service.decrypt(&encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn test_invalid_key_format() {
        let result = EncryptionService::from_key("invalid_key");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_invalid_data() {
        let key = EncryptionService::generate_key();
        let service = EncryptionService::from_key(&key).unwrap();

        let result = service.decrypt("invalid_encrypted_data");
        assert!(result.is_err());
    }
}
