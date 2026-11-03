//! The Find panel's status messages.

use super::*;

/// Does this Find panel action search the current document (rather than files)?
pub(super) fn searches_document(cmd: &Cmd, mode: Mode) -> bool {
    matches!(cmd, Cmd::Prev | Cmd::Replace | Cmd::ReplaceAll | Cmd::Count | Cmd::Mark | Cmd::FindAll) || (matches!(cmd, Cmd::Next) && mode != Mode::InFiles)
}

/// The Find panel's note for a Find Next / Previous / Replace.
pub(super) fn found_note(f: notepp_core::search::Found) -> String {
    use notepp_core::search::Found;
    match f {
        Found::At { wrapped: true } => "Found (wrapped around the document)".to_string(),
        Found::At { wrapped: false } => String::new(),
        Found::NotFound => "Not found".to_string(),
        Found::BadPattern(e) => format!("Invalid regular expression: {e}"),
    }
}

/// Why a search could not run, as the Find panel says it: only a bad pattern is a regular-expression problem.
pub(crate) fn error_note(e: &notepp_core::error::Error) -> String {
    use notepp_core::error::Error;
    match e {
        Error::EmptyPattern => "Type something to search for".into(),
        Error::Pattern(m) => format!("Invalid regular expression: {m}"),
        other => other.to_string(),
    }
}

/// The Find panel's note for a Count / Replace All / Mark: how many, or why it could not run.
pub(super) fn count_note(r: Result<usize, notepp_core::error::Error>, what: &str) -> String {
    match r {
        Ok(n) => format!("{n} {what}"),
        Err(e) => error_note(&e),
    }
}
