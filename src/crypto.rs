// AES-256-GCM encryption for sensitive fields (api_key etc.)

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::sync::OnceLock;
use rand::RngCore;
use std::path::PathBuf;

const ENC_PREFIX: &str = "enc:v1:";
static MASTER_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Initialize the encryption key from (in priority order):
/// 1. MROUTER_ENCRYPTION_KEY env var (hex-encoded 32 bytes)
/// 2. config encryption_key field (hex-encoded)
/// 3. Auto-generated key file at ~/.mrouter/encryption.key
pub fn init(config_key: Option<&str>) -> anyhow::Result<()> {
    let key = resolve_key(config_key)?;
    MASTER_KEY.set(key).map_err(|_| anyhow::anyhow!("Encryption key already initialized"))?;
    Ok(())
}

fn resolve_key(config_key: Option<&str>) -> anyhow::Result<[u8; 32]> {
    // 1. Environment variable
    if let Ok(hex_key) = std::env::var("MROUTER_ENCRYPTION_KEY") {
        return parse_hex_key(&hex_key);
    }

    // 2. Config value
    if let Some(hex_key) = config_key {
        if !hex_key.is_empty() {
            return parse_hex_key(hex_key);
        }
    }

    // 3. Auto-generated key file
    let key_path = key_file_path();
    if key_path.exists() {
        let content = std::fs::read_to_string(&key_path)?;
        return parse_hex_key(content.trim());
    }

    // Generate new key
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    let hex_key = hex::encode(key);

    // Ensure parent directory exists
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&key_path, &hex_key)?;

    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    tracing::info!("Generated new encryption key at {}", key_path.display());
    Ok(key)
}

fn parse_hex_key(hex_str: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(hex_str.trim())?;
    if bytes.len() != 32 {
        anyhow::bail!("Encryption key must be exactly 32 bytes (64 hex chars), got {} bytes", bytes.len());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn key_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mrouter")
        .join("encryption.key")
}

fn get_cipher() -> anyhow::Result<Aes256Gcm> {
    let key = MASTER_KEY.get().ok_or_else(|| anyhow::anyhow!("Encryption not initialized"))?;
    Ok(Aes256Gcm::new(key.into()))
}

/// Encrypt a plaintext string. Returns `enc:v1:<base64(nonce || ciphertext)>`.
pub fn encrypt(plaintext: &str) -> anyhow::Result<String> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }
    let cipher = get_cipher()?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // nonce (12 bytes) + ciphertext (includes tag)
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENC_PREFIX, B64.encode(&combined)))
}

/// Decrypt if the value has `enc:v1:` prefix, otherwise return as-is (plaintext passthrough).
pub fn decrypt_if_encrypted(stored: &str) -> anyhow::Result<String> {
    if stored.is_empty() {
        return Ok(String::new());
    }
    if !stored.starts_with(ENC_PREFIX) {
        // Legacy plaintext — return as-is
        return Ok(stored.to_string());
    }

    let encoded = &stored[ENC_PREFIX.len()..];
    let combined = B64.decode(encoded)?;
    if combined.len() < 12 {
        anyhow::bail!("Invalid encrypted data: too short");
    }

    let cipher = get_cipher()?;
    let nonce = Nonce::from_slice(&combined[..12]);
    let ciphertext = &combined[12..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(String::from_utf8(plaintext)?)
}

/// Check if a value is already encrypted
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENC_PREFIX)
}
