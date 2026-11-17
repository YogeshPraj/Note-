//! Word (`.docx`) files and Markdown, each way. Pure and portable: bytes in, bytes or text out.
//!
//! Writing keeps what Markdown has: headings, paragraphs, bold, italic, strikethrough, code, links, lists (also nested),
//! quotes, rules and tables. Reading takes the same from a Word file (by its styles and numbering) and drops what Markdown
//! has no word for: pictures, colours, fonts, page layout. Nothing here runs a macro or fetches a link.

use crate::error::Error;

mod parts;
mod read;
mod render;
mod write;
mod xml;

pub use read::to_markdown;
pub use write::from_markdown;

/// The largest part of a Word file read (it is a zip: a small file can hold a huge part), in bytes.
pub const MAX_PART: u64 = 64 * 1024 * 1024;

/// Is `path` a Word file by its name?
pub fn is_docx_path(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("docx"))
}

fn bad(e: impl std::fmt::Display) -> Error {
    Error::BadArchive(e.to_string())
}

#[cfg(test)]
mod tests;
