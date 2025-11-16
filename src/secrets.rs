use std::env;
use std::num::NonZeroU32;

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MASTER_ENV: &str = "RUSTCHAT_PASSPHRASE";
const PBKDF2_ITERATIONS: u32 = 150_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSecret {
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

pub fn maybe_encrypt_secret(
    value: Option<String>,
    encrypt: bool,
    passphrase: Option<&str>,
    env_label: &str,
) -> Result<(Option<String>, Option<EncryptedSecret>)> {
    if !encrypt {
        return Ok((value, None));
    }
    let plaintext = match value {
        Some(v) => v,
        None => return Ok((None, None)),
    };
    let passphrase = passphrase.map(|s| s.to_string()).ok_or_else(|| {
        anyhow!("passphrase required via {env_label} when --encrypt-secrets is used")
    })?;
    let encrypted = encrypt_secret(&passphrase, &plaintext)?;
    Ok((None, Some(encrypted)))
}

pub fn resolve_secret(
    plain: Option<&str>,
    encrypted: Option<&EncryptedSecret>,
    provided_passphrase: Option<&str>,
    env_label: &str,
) -> Result<Option<String>> {
    if let Some(value) = plain {
        return Ok(Some(value.to_string()));
    }
    if let Some(data) = encrypted {
        let passphrase = match provided_passphrase {
            Some(value) => value.to_string(),
            None => require_passphrase_from_env(env_label)?,
        };
        let decrypted = decrypt_secret(&passphrase, data)?;
        return Ok(Some(decrypted));
    }
    Ok(None)
}

pub fn require_secret(
    plain: Option<&str>,
    encrypted: Option<&EncryptedSecret>,
    provided_passphrase: Option<&str>,
    env_label: &str,
    missing_context: &str,
) -> Result<String> {
    resolve_secret(plain, encrypted, provided_passphrase, env_label)?
        .ok_or_else(|| anyhow!(missing_context.to_string()))
}

pub fn optional_passphrase_from_env(env_label: &str, strict: bool) -> Result<Option<String>> {
    match env::var(env_label) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) if !strict => Ok(None),
        Err(_) => Err(anyhow!("environment variable {env_label} is not set")),
    }
}

pub fn require_passphrase_from_env(env_label: &str) -> Result<String> {
    optional_passphrase_from_env(env_label, true)?.ok_or_else(|| {
        anyhow!("environment variable {env_label} must be set to use encrypted secrets")
    })
}

fn encrypt_secret(passphrase: &str, plaintext: &str) -> Result<EncryptedSecret> {
    let rng = SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut salt)
        .map_err(|_| anyhow!("failed to read random bytes for salt"))?;
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow!("failed to read random bytes for nonce"))?;

    let key_bytes = derive_key(passphrase, &salt);
    let unbound = UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
        .map_err(|_| anyhow!("failed to initialize AES-256-GCM"))?;
    let sealing_key = LessSafeKey::new(unbound);
    let nonce_encoded = general_purpose::STANDARD.encode(nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut buffer = plaintext.as_bytes().to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut buffer)
        .map_err(|_| anyhow!("failed to encrypt secret"))?;

    Ok(EncryptedSecret {
        salt: general_purpose::STANDARD.encode(salt),
        nonce: nonce_encoded,
        ciphertext: general_purpose::STANDARD.encode(buffer),
    })
}

fn decrypt_secret(passphrase: &str, data: &EncryptedSecret) -> Result<String> {
    let salt = decode_field(&data.salt, "salt")?;
    let nonce_bytes = decode_field(&data.nonce, "nonce")?;
    let ciphertext = decode_field(&data.ciphertext, "ciphertext")?;

    let key_bytes = derive_key(passphrase, &salt);
    let unbound = UnboundKey::new(&aead::AES_256_GCM, &key_bytes)
        .map_err(|_| anyhow!("failed to initialize AES-256-GCM"))?;
    let opening_key = LessSafeKey::new(unbound);
    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
        .map_err(|_| anyhow!("invalid nonce length"))?;
    let mut buffer = ciphertext;
    let decrypted = opening_key
        .open_in_place(nonce, Aad::empty(), &mut buffer)
        .map_err(|_| anyhow!("failed to decrypt secret"))?;
    let plaintext = String::from_utf8(decrypted.to_vec()).context("secret is not utf-8")?;
    Ok(plaintext)
}

fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
        salt,
        passphrase.as_bytes(),
        &mut key,
    );
    key
}

fn decode_field(value: &str, field: &str) -> Result<Vec<u8>> {
    general_purpose::STANDARD
        .decode(value)
        .with_context(|| format!("invalid base64 for {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_secret() {
        let secret = "shh";
        let passphrase = "topsecret";
        let encrypted = encrypt_secret(passphrase, secret).expect("encrypt");
        let decrypted = decrypt_secret(passphrase, &encrypted).expect("decrypt");
        assert_eq!(secret, decrypted);
    }
}
