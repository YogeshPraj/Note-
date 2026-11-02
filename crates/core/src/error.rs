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
    #[error("locked")]
    Locked,
    #[error("field not allowed")]
    FieldNotAllowed,
    #[error("{0}")]
    Cli(String),
    #[error("Bitwarden does not publish a command-line client for this system")]
    NoBitwardenClient,
    #[error("Download failed: {0}")]
    Download(String),
    #[error("Checksum mismatch - refusing to install.\nexpected {expected}\ngot      {got}")]
    ChecksumMismatch { expected: String, got: String },
    #[error("The download is not a valid archive: {0}")]
    BadArchive(String),
    #[error("{0} was not in the download")]
    MissingInArchive(String),
    #[error("No session token supplied")]
    NoSessionToken,
    #[error("That session token was not accepted by bw")]
    SessionRejected,
    #[error("Not signed in - use \"Sign in\" first")]
    NotSignedIn,
    #[error("Incorrect master password")]
    WrongMasterPassword,
    #[error("Unlock failed")]
    UnlockFailed,
    #[error("This account needs two-step login. Run `bw login` in a terminal once, then use \"I have a session token\".")]
    TwoStepLogin,
    #[error("Sign in failed")]
    SignInFailed,
    #[error("could not list items")]
    ItemListUnreadable,
    #[error("This item has no {0}")]
    MissingField(String),
    #[error("No vault item matches \"{0}\"")]
    NoMatchingItem(String),
    #[error("Title is required")]
    TitleRequired,
    #[error("bw generate returned nothing")]
    GenerateEmpty,
    #[error("{0}")]
    Pattern(String),
    #[error("")]
    EmptyPattern,
    #[error("{0}")]
    Json(String),
    #[error("an XML tag is not closed")]
    XmlUnclosedTag,
    #[error("'{0}' is not a Base64 character")]
    BadBase64Char(char),
    #[error("The first line cannot be hidden")]
    FirstLineHidden,
    #[error("Save {0} first: unsaved changes to a protected file can only be kept while encryption is unlocked")]
    UnsavedProtected(String),
    #[error("The file has no \"light\" or \"dark\" colours")]
    ThemeNoColours,
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Request timed out after {0}s")]
    Timeout(u64),
    #[error("{0}")]
    Request(String),
    #[error("Could not resolve {0} - unlock the Passwords vault, or check the item name.")]
    UnresolvedVariables(String),
    #[error("Unsupported or invalid URL: {0}")]
    UnsupportedUrl(String),
    #[error("GitHub sent an answer that could not be read")]
    UpdateAnswerUnreadable,
    #[error("{0}")]
    Github(String),
    #[error("Could not reach GitHub: {0}")]
    GithubUnreachable(String),
    #[error("Could not read GitHub's answer: {0}")]
    GithubRead(String),
    #[error("The archive contains an unsafe path")]
    UnsafeArchivePath,
    #[error("The download does not contain {0}")]
    MissingExe(String),
    #[error("The release has no valid checksum file")]
    NoChecksum,
    #[error("The download does not match its checksum - it was not installed.")]
    UpdateChecksumMismatch,
    #[error("cannot find the app folder")]
    NoAppFolder,
    #[error("Cannot start the updater: {0}")]
    LaunchUpdater(String),
    #[error("That is not a valid shortcut (try Ctrl+Shift+K, F9, Alt+Left)")]
    InvalidShortcut,
    #[error("Already used by \"{0}\"")]
    ShortcutInUse(String),
    #[error("{0}")]
    Cipher(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Serialize(String),
}

pub type Result<T> = std::result::Result<T, Error>;
