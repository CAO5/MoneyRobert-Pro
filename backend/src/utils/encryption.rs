use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use std::env;

use crate::error::{AppError, Result};

const NONCE_SIZE: usize = 12;

fn get_encryption_key() -> Result<[u8; 32]> {
    let key_str = env::var("ENCRYPTION_KEY").map_err(|_| {
        AppError::Internal("ENCRYPTION_KEY environment variable not set".to_string())
    })?;

    let key_bytes = STANDARD.decode(&key_str).map_err(|e| {
        AppError::Internal(format!("Invalid ENCRYPTION_KEY encoding: {}", e))
    })?;

    if key_bytes.len() < 32 {
        return Err(AppError::Internal(
            "ENCRYPTION_KEY must be at least 32 bytes when base64 decoded".to_string(),
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes[..32]);
    Ok(key)
}

pub fn encrypt(plaintext: &str) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| AppError::Internal(format!("Cipher init error: {}", e)))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Internal(format!("Encryption error: {}", e)))?;

    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(STANDARD.encode(&combined))
}

pub fn decrypt(ciphertext: &str) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| AppError::Internal(format!("Cipher init error: {}", e)))?;

    let combined = STANDARD.decode(ciphertext).map_err(|e| {
        AppError::Internal(format!("Invalid ciphertext encoding: {}", e))
    })?;

    if combined.len() < NONCE_SIZE {
        return Err(AppError::Internal("Ciphertext too short".to_string()));
    }

    let (nonce_bytes, encrypted_data) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, encrypted_data)
        .map_err(|e| AppError::Internal(format!("Decryption error: {}", e)))?;

    String::from_utf8(plaintext)
        .map_err(|e| AppError::Internal(format!("Invalid UTF-8 in decrypted data: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        env::set_var("ENCRYPTION_KEY", STANDARD.encode([42u8; 32]));
        let original = "my-secret-api-key-12345";
        let encrypted = encrypt(original).unwrap();
        assert_ne!(encrypted, original);
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        env::set_var("ENCRYPTION_KEY", STANDARD.encode([42u8; 32]));
        let plaintext = "same-input";
        let enc1 = encrypt(plaintext).unwrap();
        let enc2 = encrypt(plaintext).unwrap();
        assert_ne!(enc1, enc2);
        assert_eq!(decrypt(&enc1).unwrap(), decrypt(&enc2).unwrap());
    }
}
