//! The crate's error type. One enum for the failures the core reports to its callers; the messages are what the
//! user reads, so they are written for people.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("no secure random numbers: {0}")]
    Random(String),
    #[error("corrupt data (bad base64)")]
    BadBase64,
    #[error("The recovery key is empty")]
    EmptyRecoveryKey,
    #[error("That recovery key is too short")]
    RecoveryKeyTooShort,
    #[error("That recovery key does not match this profile")]
    RecoveryKeyMismatch,
    #[error("Incorrect password")]
    WrongPassword,
    #[error("key derivation failed")]
    KeyDerivation,
    #[error("encryption failed")]
    Encryption,
    #[error("This file could not be decrypted: wrong key, or it was changed after it was saved")]
    Decryption,
    #[error("corrupt data: {0}")]
    Corrupt(String),
    #[error("The decrypted text is not valid UTF-8")]
    NotUtf8,
    #[error("Not a valid recovery key character: {0}")]
    BadKeyChar(char),
    #[error("This profile has no password")]
    NoPassword,
    #[error("This profile has no recovery key")]
    NoRecoveryKey,
    #[error("{0}")]
    Cipher(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Serialize(String),
}

pub type Result<T> = std::result::Result<T, Error>;
