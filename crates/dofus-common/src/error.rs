use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Account banned")]
    AccountBanned,
    #[error("Already connected")]
    AlreadyConnected,
    #[error("Server not found: {0}")]
    ServerNotFound(u16),
    #[error("RSA decryption failed: {0}")]
    RsaError(String),
}

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("Invalid ticket")]
    InvalidTicket,
    #[error("Character not found: {0}")]
    CharacterNotFound(i64),
    #[error("Character name already taken")]
    CharacterNameTaken,
}

/// Dofus IdentificationFailedReason codes
pub mod identification_failure {
    pub const BAD_VERSION: u8 = 1;
    pub const WRONG_CREDENTIALS: u8 = 2;
    pub const BANNED: u8 = 3;
    pub const KICKED: u8 = 4;
    pub const IN_MAINTENANCE: u8 = 5;
    pub const TOO_MANY_ON_IP: u8 = 6;
    pub const TIME_TOO_EARLY: u8 = 7;
    pub const BAD_IPRANGE: u8 = 8;
    pub const CREDENTIALS_RESET: u8 = 9;
    pub const EMAIL_UNVALIDATED: u8 = 10;
    pub const OTP_TIMEOUT: u8 = 11;
    pub const UNKNOWN_AUTH_ERROR: u8 = 99;
}
