use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use argon2::Argon2;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct EncryptedPayload {
    salt: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("key derivation failed");
    key
}

pub fn encrypt_config(plaintext: &str, password: &str) -> Result<Vec<u8>, String> {
    let mut rng = rand::thread_rng();
    let salt: [u8; 16] = rng.gen();
    let nonce_bytes: [u8; 12] = rng.gen();

    let key_bytes = derive_key(password, &salt);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let payload = EncryptedPayload {
        salt: salt.to_vec(),
        nonce: nonce_bytes.to_vec(),
        ciphertext,
    };
    serde_json::to_vec(&payload).map_err(|e| format!("Serialize failed: {}", e))
}

pub fn decrypt_config(data: &[u8], password: &str) -> Result<String, String> {
    let payload: EncryptedPayload =
        serde_json::from_slice(data).map_err(|_| "Not an encrypted config file".to_string())?;

    let key_bytes = derive_key(password, &payload.salt);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&payload.nonce);

    let plaintext = cipher
        .decrypt(nonce, payload.ciphertext.as_ref())
        .map_err(|_| "Wrong password or corrupted config".to_string())?;

    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 error: {}", e))
}

/// Check if data looks like an encrypted config (has salt/nonce/ciphertext fields)
pub fn is_encrypted(data: &[u8]) -> bool {
    serde_json::from_slice::<EncryptedPayload>(data).is_ok()
        && serde_json::from_slice::<crate::config::AppConfig>(data).is_err()
}
