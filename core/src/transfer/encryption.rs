use anyhow::{Result, anyhow};
use aes::Aes256;
use cbc::cipher::{block_padding::Pkcs7, KeyIvInit, BlockEncryptMut, BlockDecryptMut};
use hmac::{Hmac, Mac};
use hkdf::Hkdf;
use p256::ecdh::EphemeralSecret;
use rand::RngCore;
use sha2::{Sha256, Digest};

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

const AES_IV_SIZE: usize = 16;

/// HKDF-SHA256 extract + expand helper.
fn hkdf_extract_expand(salt: &[u8], ikm: &[u8], info: &[u8], output_len: usize) -> Vec<u8> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = vec![0u8; output_len];
    hkdf.expand(info, &mut okm).expect("HKDF expand failed");
    okm
}

/// Generate a P-256 ECDH ephemeral keypair.
pub fn generate_ecdh_keypair() -> (EphemeralSecret, Vec<u8>) {
    let mut rng = rand::thread_rng();
    let secret = EphemeralSecret::random(&mut rng);
    let public = secret.public_key();
    let public_bytes = public.to_sec1_bytes().to_vec();
    (secret, public_bytes)
}

/// Compute the raw ECDH shared secret from our secret and peer's public key bytes (SEC1 format).
pub fn ecdh_shared_secret(secret: &EphemeralSecret, peer_public_bytes: &[u8]) -> Result<Vec<u8>> {
    let peer_pub = p256::PublicKey::from_sec1_bytes(peer_public_bytes)
        .map_err(|e| anyhow!("Invalid public key: {}", e))?;
    let shared = secret.diffie_hellman(&peer_pub);
    Ok(shared.raw_secret_bytes().to_vec())
}

/// Compute SHA-256 hash.
fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

// === HKDF constants from rquickshare ===

const UKEY2_V1_AUTH_SALT: &[u8] = b"UKEY2 v1 auth";
const UKEY2_V1_NEXT_SALT: &[u8] = b"UKEY2 v1 next";

const D2D_SALT: &[u8] = &[
    0x82, 0xAA, 0x55, 0xA0, 0xD3, 0x97, 0xF8, 0x83,
    0x46, 0xCA, 0x1C, 0xEE, 0x8D, 0x39, 0x09, 0xB9,
    0x5F, 0x13, 0xFA, 0x7D, 0xEB, 0x1D, 0x4A, 0xB3,
    0x83, 0x76, 0xB8, 0x25, 0x6D, 0xA8, 0x55, 0x10,
];

const KEY_SALT: &[u8] = &[
    0xBF, 0x9D, 0x2A, 0x53, 0xC6, 0x36, 0x16, 0xD7,
    0x5D, 0xB0, 0xA7, 0x16, 0x5B, 0x91, 0xC1, 0xEF,
    0x73, 0xE5, 0x37, 0xF2, 0x42, 0x74, 0x05, 0xFA,
    0x23, 0x61, 0x0A, 0x4B, 0xE6, 0x57, 0x64, 0x2E,
];

/// Result of the UKEY2 key exchange.
pub struct KeyExchangeResult {
    pub client_key: [u8; 32],
    pub client_hmac_key: [u8; 32],
    pub server_key: [u8; 32],
    pub server_hmac_key: [u8; 32],
    pub auth_string: [u8; 32],
}

/// Finalize the UKEY2 key exchange.
pub fn finalize_key_exchange(
    shared_secret: &[u8],
    initiator: bool,
    our_msg_data: &[u8],
    peer_msg_data: &[u8],
) -> Result<KeyExchangeResult> {
    let derived_secret = sha256(shared_secret);

    let (first_msg, second_msg) = if initiator {
        (our_msg_data, peer_msg_data)
    } else {
        (peer_msg_data, our_msg_data)
    };
    let mut ukey_info = Vec::with_capacity(first_msg.len() + second_msg.len());
    ukey_info.extend_from_slice(first_msg);
    ukey_info.extend_from_slice(second_msg);

    let auth_string = hkdf_extract_expand(UKEY2_V1_AUTH_SALT, &derived_secret, &ukey_info, 32);
    let next_secret = hkdf_extract_expand(UKEY2_V1_NEXT_SALT, &derived_secret, &ukey_info, 32);

    let d2d_client = hkdf_extract_expand(D2D_SALT, &next_secret, b"client", 32);
    let d2d_server = hkdf_extract_expand(D2D_SALT, &next_secret, b"server", 32);

    let client_key = hkdf_extract_expand(KEY_SALT, &d2d_client, b"ENC:2", 32);
    let client_hmac = hkdf_extract_expand(KEY_SALT, &d2d_client, b"SIG:1", 32);
    let server_key = hkdf_extract_expand(KEY_SALT, &d2d_server, b"ENC:2", 32);
    let server_hmac = hkdf_extract_expand(KEY_SALT, &d2d_server, b"SIG:1", 32);

    let mut result = KeyExchangeResult {
        client_key: [0u8; 32],
        client_hmac_key: [0u8; 32],
        server_key: [0u8; 32],
        server_hmac_key: [0u8; 32],
        auth_string: [0u8; 32],
    };
    result.client_key.copy_from_slice(&client_key);
    result.client_hmac_key.copy_from_slice(&client_hmac);
    result.server_key.copy_from_slice(&server_key);
    result.server_hmac_key.copy_from_slice(&server_hmac);
    result.auth_string.copy_from_slice(&auth_string);
    Ok(result)
}

/// Generate a 4-digit PIN from the auth_string.
pub fn auth_string_to_pin(auth_string: &[u8]) -> String {
    let val = u32::from_be_bytes([auth_string[0], auth_string[1], auth_string[2], auth_string[3]]);
    let pin = (val % 9973) * 31;
    format!("{:04}", pin % 10000)
}

/// AES-256-CBC encrypt with PKCS7 padding.
pub fn aes_cbc_encrypt(key: &[u8; 32], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    let encryptor = Aes256CbcEnc::new(key.into(), iv.into());
    encryptor.encrypt_padded_vec_mut::<Pkcs7>(plaintext)
}

/// AES-256-CBC decrypt with PKCS7 padding removal.
pub fn aes_cbc_decrypt(key: &[u8; 32], iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let decryptor = Aes256CbcDec::new(key.into(), iv.into());
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|e| anyhow!("AES-CBC decryption failed: {}", e))
}

/// HMAC-SHA256 sign.
pub fn hmac_sign(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .expect("HMAC key length is always valid for SHA256");
    mac.update(data);
    let result = mac.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result.into_bytes());
    out
}

/// HMAC-SHA256 verify.
pub fn hmac_verify(key: &[u8; 32], data: &[u8], tag: &[u8]) -> bool {
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .expect("HMAC key length is always valid for SHA256");
    mac.update(data);
    let expected = mac.finalize().into_bytes();
    if expected.len() != tag.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(tag.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Crypto keys for SecureMessage encryption/decryption.
#[derive(Clone)]
pub struct SecureMessageKeys {
    pub encrypt_key: [u8; 32],
    pub decrypt_key: [u8; 32],
    pub send_hmac_key: [u8; 32],
    pub recv_hmac_key: [u8; 32],
}

impl SecureMessageKeys {
    pub fn from_key_exchange(kx: &KeyExchangeResult, is_initiator: bool) -> Self {
        if is_initiator {
            SecureMessageKeys {
                encrypt_key: kx.server_key,
                decrypt_key: kx.client_key,
                send_hmac_key: kx.server_hmac_key,
                recv_hmac_key: kx.client_hmac_key,
            }
        } else {
            SecureMessageKeys {
                encrypt_key: kx.client_key,
                decrypt_key: kx.server_key,
                send_hmac_key: kx.client_hmac_key,
                recv_hmac_key: kx.server_hmac_key,
            }
        }
    }
}

/// Encrypt a protobuf payload into a SecureMessage envelope.
pub fn encrypt_secure_message(keys: &SecureMessageKeys, payload: &[u8]) -> Result<Vec<u8>> {
    let mut iv = [0u8; AES_IV_SIZE];
    rand::thread_rng().fill_bytes(&mut iv);

    let ciphertext = aes_cbc_encrypt(&keys.encrypt_key, &iv, payload);

    let header = crate::protocol::securemessage::Header {
        signature_scheme: 1,  // HMAC_SHA256
        encryption_scheme: 2, // AES_256_CBC
        verification_key_id: None,
        decryption_key_id: None,
        iv: Some(iv.to_vec()),
        public_metadata: None,
        associated_data_length: Some(0),
    };

    let header_and_body = crate::protocol::securemessage::HeaderAndBody {
        header,
        body: ciphertext,
    };
    let header_and_body_bytes = prost::Message::encode_to_vec(&header_and_body);

    let signature = hmac_sign(&keys.send_hmac_key, &header_and_body_bytes);

    let secure_msg = crate::protocol::securemessage::SecureMessage {
        header_and_body: header_and_body_bytes,
        signature: signature.to_vec(),
    };

    Ok(prost::Message::encode_to_vec(&secure_msg))
}

/// Decrypt a SecureMessage to get the inner protobuf payload.
pub fn decrypt_secure_message(keys: &SecureMessageKeys, secure_msg_bytes: &[u8]) -> Result<Vec<u8>> {
    let secure_msg: crate::protocol::securemessage::SecureMessage =
        prost::Message::decode(secure_msg_bytes)
            .map_err(|e| anyhow!("Failed to decode SecureMessage: {}", e))?;

    if !hmac_verify(&keys.recv_hmac_key, &secure_msg.header_and_body, &secure_msg.signature) {
        return Err(anyhow!("HMAC verification failed"));
    }

    let header_and_body: crate::protocol::securemessage::HeaderAndBody =
        prost::Message::decode(&secure_msg.header_and_body[..])
            .map_err(|e| anyhow!("Failed to decode HeaderAndBody: {}", e))?;

    let header = header_and_body.header;
    let iv = header.iv
        .ok_or_else(|| anyhow!("Missing IV in header"))?;

    if iv.len() != AES_IV_SIZE {
        return Err(anyhow!("Invalid IV length: {}", iv.len()));
    }

    let mut iv_arr = [0u8; AES_IV_SIZE];
    iv_arr.copy_from_slice(&iv);

    aes_cbc_decrypt(&keys.decrypt_key, &iv_arr, &header_and_body.body)
}

/// Wrap bytes into a SecureMessage with DeviceToDeviceMessage.
pub fn wrap_secure(keys: &SecureMessageKeys, payload: &[u8], sequence_number: i32) -> Result<Vec<u8>> {
    let d2d = crate::protocol::securegcm_proto::DeviceToDeviceMessage {
        message: Some(payload.to_vec()),
        sequence_number: Some(sequence_number),
    };
    let d2d_bytes = prost::Message::encode_to_vec(&d2d);
    encrypt_secure_message(keys, &d2d_bytes)
}

/// Unwrap a SecureMessage to get the inner payload bytes and sequence number.
pub fn unwrap_secure(keys: &SecureMessageKeys, secure_msg_bytes: &[u8]) -> Result<(Vec<u8>, i32)> {
    let plaintext = decrypt_secure_message(keys, secure_msg_bytes)?;

    let d2d: crate::protocol::securegcm_proto::DeviceToDeviceMessage =
        prost::Message::decode(&plaintext[..])
            .map_err(|e| anyhow!("Failed to decode DeviceToDeviceMessage: {}", e))?;

    let message = d2d.message
        .ok_or_else(|| anyhow!("Missing message in DeviceToDeviceMessage"))?;
    let seq = d2d.sequence_number.unwrap_or(0);

    Ok((message, seq))
}

/// Generate a random 4-byte salt for advertisement.
pub fn generate_salt() -> [u8; 4] {
    let mut salt = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

/// Generate a random payload ID (16 bytes).
pub fn generate_payload_id() -> Vec<u8> {
    let mut id = vec![0u8; 16];
    rand::thread_rng().fill_bytes(&mut id);
    id
}

/// Wrap the current transfer state for async use.
pub struct CryptoContext {
    keys: std::sync::Arc<tokio::sync::Mutex<Option<SecureMessageKeys>>>,
    send_seq: std::sync::Arc<tokio::sync::Mutex<i32>>,
    recv_seq: std::sync::Arc<tokio::sync::Mutex<i32>>,
}

impl CryptoContext {
    pub fn new() -> Self {
        CryptoContext {
            keys: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            send_seq: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
            recv_seq: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    pub async fn init(&self, keys: SecureMessageKeys) {
        *self.keys.lock().await = Some(keys);
    }

    pub async fn encrypt(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let keys_guard = self.keys.lock().await;
        let keys = keys_guard.as_ref().ok_or_else(|| anyhow!("Keys not initialized"))?;
        let mut seq = self.send_seq.lock().await;
        let result = wrap_secure(keys, payload, *seq)?;
        *seq += 1;
        Ok(result)
    }

    pub async fn decrypt(&self, data: &[u8]) -> Result<(Vec<u8>, i32)> {
        let keys_guard = self.keys.lock().await;
        let keys = keys_guard.as_ref().ok_or_else(|| anyhow!("Keys not initialized"))?;
        let (payload, seq) = unwrap_secure(keys, data)?;
        let mut recv_seq = self.recv_seq.lock().await;
        if seq != *recv_seq {
            return Err(anyhow!("Sequence number mismatch: expected {}, got {}", *recv_seq, seq));
        }
        *recv_seq += 1;
        Ok((payload, seq))
    }

    pub async fn is_initialized(&self) -> bool {
        self.keys.lock().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecdh_key_exchange() {
        let (secret_a, pub_a) = generate_ecdh_keypair();
        let (secret_b, pub_b) = generate_ecdh_keypair();

        let shared_a = ecdh_shared_secret(&secret_a, &pub_b).unwrap();
        let shared_b = ecdh_shared_secret(&secret_b, &pub_a).unwrap();
        assert_eq!(shared_a, shared_b);
    }

    #[test]
    fn test_hkdf_key_derivation() {
        let shared_secret = sha256(b"test shared secret");
        let client_init = b"client_init_data";
        let server_init = b"server_init_data";

        let mut info = Vec::new();
        info.extend_from_slice(client_init);
        info.extend_from_slice(server_init);

        let auth1 = hkdf_extract_expand(UKEY2_V1_AUTH_SALT, &shared_secret, &info, 32);
        let auth2 = hkdf_extract_expand(UKEY2_V1_AUTH_SALT, &shared_secret, &info, 32);
        assert_eq!(auth1, auth2, "HKDF must be deterministic");
    }

    #[test]
    fn test_finalize_key_exchange_symmetry() {
        let (secret_a, pub_a_bytes) = generate_ecdh_keypair();
        let (secret_b, pub_b_bytes) = generate_ecdh_keypair();

        let shared_a = ecdh_shared_secret(&secret_a, &pub_b_bytes).unwrap();
        let shared_b = ecdh_shared_secret(&secret_b, &pub_a_bytes).unwrap();

        let client_init = b"fake_client_init";
        let server_init = b"fake_server_init";

        let kx_a = finalize_key_exchange(&shared_a, true, client_init, server_init).unwrap();
        let kx_b = finalize_key_exchange(&shared_b, false, server_init, client_init).unwrap();

        assert_eq!(kx_a.client_key, kx_b.client_key);
        assert_eq!(kx_a.server_key, kx_b.server_key);
        assert_eq!(kx_a.auth_string, kx_b.auth_string);
    }

    #[test]
    fn test_aes_cbc_roundtrip() {
        let key = [0x42u8; 32];
        let iv = [0x24u8; 16];
        let plaintext = b"Hello, QuickShare UKEY2!";

        let ciphertext = aes_cbc_encrypt(&key, &iv, plaintext);
        let decrypted = aes_cbc_decrypt(&key, &iv, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_hmac_sign_verify() {
        let key = [0xAAu8; 32];
        let data = b"test data to sign";

        let tag = hmac_sign(&key, data);
        assert!(hmac_verify(&key, data, &tag));
        assert!(!hmac_verify(&key, b"wrong data", &tag));
        assert!(!hmac_verify(&[0xBBu8; 32], data, &tag));
    }

    #[test]
    fn test_secure_message_roundtrip() {
        let kx = KeyExchangeResult {
            client_key: [1u8; 32],
            client_hmac_key: [2u8; 32],
            server_key: [3u8; 32],
            server_hmac_key: [4u8; 32],
            auth_string: [5u8; 32],
        };

        let sender_keys = SecureMessageKeys::from_key_exchange(&kx, true);
        let receiver_keys = SecureMessageKeys::from_key_exchange(&kx, false);

        let plaintext = b"test offline frame payload";
        let encrypted = encrypt_secure_message(&sender_keys, plaintext).unwrap();
        let decrypted = decrypt_secure_message(&receiver_keys, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_auth_string_to_pin() {
        let auth = [0x00, 0x00, 0x27, 0x1A, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let pin = auth_string_to_pin(&auth);
        assert_eq!(pin.len(), 4);
    }
}
