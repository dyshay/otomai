use sha2::{Digest, Sha256};

const SALT_MIN_LENGTH: usize = 32;

/// Decrypt RSA-encrypted credentials from the Dofus client.
///
/// AS3 format (AuthentificationManager.cipherRsa):
///   salt_bytes (padded to 32 with spaces)
///   + AES_key (32 bytes)
///   + [if certificate: cert_id (u32 BE) + cert_hash (UTF-8 bytes)]
///   + username_len (1 byte)
///   + username (UTF-8 bytes)
///   + password (UTF-8 bytes, remaining data)
pub fn decrypt_credentials(
    private_key: &rsa::RsaPrivateKey,
    salt: &str,
    encrypted: &[u8],
    has_certificate: bool,
) -> anyhow::Result<(String, String)> {
    use rsa::Pkcs1v15Encrypt;
    let decrypted = private_key.decrypt(Pkcs1v15Encrypt, encrypted)?;
    parse_credential_bytes(&decrypted, salt, has_certificate)
}

/// Parse decrypted credential bytes (after RSA decryption).
pub fn parse_credential_bytes(
    decrypted: &[u8],
    salt: &str,
    has_certificate: bool,
) -> anyhow::Result<(String, String)> {
    let salt_len = salt.len().max(SALT_MIN_LENGTH);
    let mut offset = salt_len;

    const AES_KEY_LENGTH: usize = 32;
    if decrypted.len() < offset + AES_KEY_LENGTH {
        anyhow::bail!(
            "Decrypted data too short for AES key (len={}, need={})",
            decrypted.len(),
            offset + AES_KEY_LENGTH
        );
    }
    offset += AES_KEY_LENGTH;

    if has_certificate {
        if decrypted.len() < offset + 4 {
            anyhow::bail!("Decrypted data too short for certificate");
        }
        let _cert_id = u32::from_be_bytes([
            decrypted[offset],
            decrypted[offset + 1],
            decrypted[offset + 2],
            decrypted[offset + 3],
        ]);
        offset += 4;
        tracing::debug!("Certificate present, cert_id={}", _cert_id);
    }

    if decrypted.len() < offset + 1 {
        anyhow::bail!("Decrypted data too short for username length");
    }
    let username_len = decrypted[offset] as usize;
    offset += 1;

    if decrypted.len() < offset + username_len {
        anyhow::bail!(
            "Decrypted data too short for username (need={}, have={})",
            offset + username_len,
            decrypted.len()
        );
    }
    let username = String::from_utf8_lossy(&decrypted[offset..offset + username_len]).to_string();
    offset += username_len;

    let password = String::from_utf8_lossy(&decrypted[offset..]).to_string();

    if username.is_empty() {
        anyhow::bail!("Empty username");
    }

    Ok((username, password))
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT_MIN: usize = 32;

    fn build_credentials(salt: &str, username: &str, password: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut padded_salt = salt.to_string();
        while padded_salt.len() < SALT_MIN {
            padded_salt.push(' ');
        }
        buf.extend_from_slice(padded_salt.as_bytes());
        buf.extend_from_slice(&[0xAA; 32]);
        buf.push(username.len() as u8);
        buf.extend_from_slice(username.as_bytes());
        buf.extend_from_slice(password.as_bytes());
        buf
    }

    fn build_credentials_with_cert(salt: &str, username: &str, password: &str, cert_id: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut padded_salt = salt.to_string();
        while padded_salt.len() < SALT_MIN {
            padded_salt.push(' ');
        }
        buf.extend_from_slice(padded_salt.as_bytes());
        buf.extend_from_slice(&[0xBB; 32]);
        buf.extend_from_slice(&cert_id.to_be_bytes());
        buf.push(username.len() as u8);
        buf.extend_from_slice(username.as_bytes());
        buf.extend_from_slice(password.as_bytes());
        buf
    }

    #[test]
    fn parse_credentials_basic() {
        let salt = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let data = build_credentials(salt, "testuser", "mypassword");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "testuser");
        assert_eq!(pass, "mypassword");
    }

    #[test]
    fn parse_credentials_short_salt() {
        let salt = "short";
        let data = build_credentials(salt, "admin", "secret123");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "admin");
        assert_eq!(pass, "secret123");
    }

    #[test]
    fn parse_credentials_long_salt() {
        let salt = "this-is-a-very-long-salt-string-that-exceeds-32-characters-easily";
        let mut buf = Vec::new();
        buf.extend_from_slice(salt.as_bytes());
        buf.extend_from_slice(&[0xCC; 32]);
        buf.push(4);
        buf.extend_from_slice(b"user");
        buf.extend_from_slice(b"pass");
        let (user, pass) = parse_credential_bytes(&buf, salt, false).unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "pass");
    }

    #[test]
    fn parse_credentials_unicode() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials(salt, "joueur", "motdepasse123");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "joueur");
        assert_eq!(pass, "motdepasse123");
    }

    #[test]
    fn parse_credentials_empty_password() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials(salt, "user", "");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "");
    }

    #[test]
    fn parse_credentials_empty_username_fails() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials(salt, "", "pass");
        assert!(parse_credential_bytes(&data, salt, false).is_err());
    }

    #[test]
    fn parse_credentials_too_short_fails() {
        let salt = "12345678901234567890123456789012";
        let data = vec![0u8; 10];
        assert!(parse_credential_bytes(&data, salt, false).is_err());
    }

    #[test]
    fn parse_credentials_with_certificate() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials_with_cert(salt, "certuser", "certpass", 12345);
        let (user, pass) = parse_credential_bytes(&data, salt, true).unwrap();
        assert_eq!(user, "certuser");
        assert_eq!(pass, "certpass");
    }

    #[test]
    fn hash_password_deterministic() {
        let h1 = hash_password("test123");
        let h2 = hash_password("test123");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_password_different_inputs() {
        let h1 = hash_password("password1");
        let h2 = hash_password("password2");
        assert_ne!(h1, h2);
    }
}
