//! HTTP scratchpad: requests written as plain text (`.http` / `.rest` files), run with Ctrl+Enter.
//!
//! ```http
//! @base = https://api.example.com
//!
//! ### create a charge
//! POST {{base}}/v1/charges
//! Authorization: Bearer {{vault:Stripe}}
//! Content-Type: application/json
//!
//! { "amount": 2000 }
//! ```
//!
//! Requests are separated by `###`; the one under the cursor runs. `{{name}}` comes from `@name`
//! lines. `{{vault:Item}}` / `{{vault:Item.username}}` are looked up in the Bitwarden vault only at
//! the moment the request is sent, so the editor text (and the displayed request) never holds the
//! secret.

use crate::error::Error;
use serde_json::json;
use std::collections::HashMap;
use std::io::Read;
use std::time::{Duration, Instant};

const METHODS: [&str; 8] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE"];
const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;
const TIMEOUT: Duration = Duration::from_secs(30);

/// A request as written: `{{vault:...}}` placeholders are still in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub name: Option<String>,
}

/// Is this file one the scratchpad runs?
pub fn is_http_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("http") || e.eq_ignore_ascii_case("rest"))
}

// ---- parsing --------------------------------------------------------------------------------

fn is_comment(l: &str) -> bool {
    let t = l.trim_start();
    t.starts_with('#') || t.starts_with("//")
}

fn var_line(l: &str) -> Option<(&str, &str)> {
    let t = l.trim_start().strip_prefix('@')?;
    let end = t.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(t.len());
    let name = &t[..end];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return None;
    }
    let rest = t[end..].trim_start().strip_prefix('=')?;
    Some((name, rest.trim()))
}

/// File-level variables: every `@name = value` line.
pub fn parse_vars(lines: &[&str]) -> HashMap<String, String> {
    lines.iter().filter_map(|l| var_line(l)).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Replace `{{name}}` from `vars`. `{{vault:...}}` is left alone on purpose.
pub fn expand<S: std::hash::BuildHasher>(text: &str, vars: &HashMap<String, String, S>) -> String {
    rewrite(text, |name| {
        if starts_with_vault(name) {
            return None;
        }
        vars.get(name).cloned()
    })
}

fn starts_with_vault(name: &str) -> bool {
    name.len() >= 6 && name.is_char_boundary(6) && name[..6].eq_ignore_ascii_case("vault:")
}

/// Replace each `{{ ... }}` for which `f(trimmed inside)` gives a value; others stay as written.
fn rewrite(text: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        match after.find("}}") {
            Some(close) if !after[..close].contains('{') => {
                match f(after[..close].trim()) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + 2 + close + 2]),
                }
                rest = &after[close + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

struct Block {
    start: usize,
    end: usize, // inclusive
    name: Option<String>,
}

fn split_blocks(lines: &[&str]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let (mut start, mut name) = (0usize, None::<String>);
    let mut push = |end_exclusive: usize, start: usize, name: &Option<String>| {
        if end_exclusive > start {
            blocks.push(Block { start, end: end_exclusive - 1, name: name.clone() });
        }
    };
    for (i, l) in lines.iter().enumerate() {
        if let Some(rest) = l.trim_start().strip_prefix("###") {
            push(i, start, &name);
            start = i + 1;
            let n = rest.trim();
            name = (!n.is_empty()).then(|| n.to_string());
        }
    }
    push(lines.len(), start, &name);
    blocks
}

fn parse_block(lines: &[&str], b: &Block, vars: &HashMap<String, String>) -> Option<Request> {
    let mut i = b.start;
    while i <= b.end && (lines[i].trim().is_empty() || is_comment(lines[i]) || var_line(lines[i]).is_some()) {
        i += 1;
    }
    if i > b.end {
        return None;
    }
    let first = lines[i].trim();
    let mut parts = first.splitn(2, char::is_whitespace);
    let word = parts.next().unwrap_or("");
    let (method, url) = if METHODS.iter().any(|m| m.eq_ignore_ascii_case(word)) {
        let url = parts.next().unwrap_or("").split_whitespace().next().unwrap_or("");
        if url.is_empty() {
            return None;
        }
        (word.to_ascii_uppercase(), url.to_string())
    } else {
        // a bare URL is a GET
        let lower = first.to_ascii_lowercase();
        if first.contains(char::is_whitespace) || !(lower.starts_with("http://") || lower.starts_with("https://") || first.starts_with("{{")) {
            return None;
        }
        ("GET".to_string(), first.to_string())
    };
    i += 1;

    let mut headers = Vec::new();
    while i <= b.end {
        let l = lines[i];
        if l.trim().is_empty() {
            i += 1;
            break;
        }
        i += 1;
        if is_comment(l) {
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            let k = k.trim();
            if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                headers.push((k.to_string(), expand(v.trim(), vars)));
            }
        }
    }

    let mut body: Vec<&str> = Vec::new();
    while i <= b.end {
        if is_comment(lines[i]) && body.is_empty() {
            i += 1;
            continue;
        }
        body.push(lines[i]);
        i += 1;
    }
    let body = body.join("\n").trim().to_string();
    Some(Request { method, url: expand(&url, vars), headers, body: (!body.is_empty()).then(|| expand(&body, vars)), name: b.name.clone() })
}

/// The request the cursor is in (0-based `cursor_line`), else the first request in the file.
pub fn request_at(text: &str, cursor_line: usize) -> Option<Request> {
    let lines: Vec<&str> = text.lines().collect();
    let vars = parse_vars(&lines);
    let blocks = split_blocks(&lines);
    blocks
        .iter()
        .find(|b| cursor_line >= b.start && cursor_line <= b.end)
        .and_then(|b| parse_block(&lines, b, &vars))
        .or_else(|| blocks.iter().find_map(|b| parse_block(&lines, b, &vars)))
}

// ---- vault references -----------------------------------------------------------------------

/// Does `text` have a `{{vault:...}}` reference in it?
fn has_vault_ref(text: &str) -> bool {
    let mut missing = Vec::new();
    resolve_vault_refs(text, &mut HashMap::new(), &mut |_| None, &mut Vec::new(), &mut missing);
    !missing.is_empty()
}

/// Substitute `{{vault:...}}` using `lookup(reference)`. Returns the text, the references used and
/// the ones that could not be found.
pub fn resolve_vault_refs(
    text: &str,
    cache: &mut HashMap<String, Option<String>, impl std::hash::BuildHasher>,
    lookup: &mut dyn FnMut(&str) -> Option<String>,
    used: &mut Vec<String>,
    missing: &mut Vec<String>,
) -> String {
    rewrite(text, |name| {
        if !starts_with_vault(name) {
            return None;
        }
        let reference = name[6..].trim().to_string();
        let value = cache.entry(reference.clone()).or_insert_with(|| lookup(&reference)).clone();
        if let Some(v) = value {
            if !used.contains(&reference) {
                used.push(reference);
            }
            Some(v)
        } else {
            if !missing.contains(&reference) {
                missing.push(reference);
            }
            None
        }
    })
}

mod send;
pub use send::*;

#[cfg(test)]
mod tests;
