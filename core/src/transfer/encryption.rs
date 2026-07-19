use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use anyhow::{Result, anyhow};
use rand::RngCore;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use std::sync::Arc;
use tokio::sync::Mutex;

const AES_KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;

pub struct CryptoContext {
    key: Arc<Mutex<Option<Aes256Gcm>>>,
}

impl CryptoContext {
    pub fn new() -> Self {
        CryptoContext {
            key: Arc::new(Mutex::new(None)),
        }
    }

    /// Generate an X25519 keypair for key exchange.
    pub fn generate_keypair() -> (EphemeralSecret, PublicKey) {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        (secret, public)
    }

    /// Derive an AES-256-GCM key from an X25519 shared secret.
    pub fn derive_key(shared_secret: &SharedSecret) -> AesGcmKey {
        let mut key_material = [0u8; AES_KEY_SIZE];
        let hash = blake3::hash(shared_secret.as_bytes());
        key_material.copy_from_slice(&hash.as_bytes()[..AES_KEY_SIZE]);
        AesGcmKey(key_material)
    }

    /// Initialize the AES-GCM cipher from a derived key.
    pub async fn init_cipher(&self, key: &AesGcmKey) -> Result<()> {
        let cipher = Aes256Gcm::new_from_slice(&key.0)
            .map_err(|e| anyhow!("Failed to create AES-GCM cipher: {}", e))?;
        *self.key.lock().await = Some(cipher);
        Ok(())
    }

    /// Encrypt payload. Returns (nonce, ciphertext).
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        let cipher = self.key.lock().await;
        let cipher = cipher.as_ref()
            .ok_or_else(|| anyhow!("Cipher not initialized"))?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        Ok((nonce_bytes.to_vec(), ciphertext))
    }

    /// Decrypt payload. Takes (nonce, ciphertext) and returns plaintext.
    pub async fn decrypt(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let cipher = self.key.lock().await;
        let cipher = cipher.as_ref()
            .ok_or_else(|| anyhow!("Cipher not initialized"))?;

        let nonce = Nonce::from_slice(nonce);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }
}

/// Wrapper for the 32-byte AES key.
pub struct AesGcmKey(pub [u8; AES_KEY_SIZE]);

/// Generate a random 32-byte salt for advertisement frame.
pub fn generate_salt() -> [u8; 4] {
    let mut salt = [0u8; 4];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Generate a random payload ID (16 bytes).
pub fn generate_payload_id() -> Vec<u8> {
    let mut id = vec![0u8; 16];
    OsRng.fill_bytes(&mut id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let (secret_a, public_a) = CryptoContext::generate_keypair();
        let (secret_b, public_b) = CryptoContext::generate_keypair();

        let shared_a = secret_a.diffie_hellman(&public_b);
        let shared_b = secret_b.diffie_hellman(&public_a);

        let key1 = CryptoContext::derive_key(&shared_a);
        let key2 = CryptoContext::derive_key(&shared_b);

        assert_eq!(key1.0, key2.0, "Both sides should derive the same AES key");
    }

    #[tokio::test]
    async fn test_encrypt_decrypt() {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let other_secret = EphemeralSecret::random_from_rng(OsRng);
        let other_pub = PublicKey::from(&other_secret);

        let shared = secret.diffie_hellman(&other_pub);
        let key = CryptoContext::derive_key(&shared);

        let ctx = CryptoContext::new();
        ctx.init_cipher(&key).await.unwrap();

        let plaintext = b"Hello, QuickShare!";
        let (nonce, ciphertext) = ctx.encrypt(plaintext).await.unwrap();

        let decrypted = ctx.decrypt(&nonce, &ciphertext).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
