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
pub fn expand(text: &str, vars: &HashMap<String, String>) -> String {
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
        let url = parts.next().unwrap_or("").trim().split_whitespace().next().unwrap_or("");
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

/// Substitute `{{vault:...}}` using `lookup(reference)`. Returns the text, the references used and
/// the ones that could not be found.
pub fn resolve_vault_refs(text: &str, cache: &mut HashMap<String, Option<String>>, lookup: &mut dyn FnMut(&str) -> Option<String>, used: &mut Vec<String>, missing: &mut Vec<String>) -> String {
    rewrite(text, |name| {
        if !starts_with_vault(name) {
            return None;
        }
        let reference = name[6..].trim().to_string();
        let value = cache.entry(reference.clone()).or_insert_with(|| lookup(&reference)).clone();
        match value {
            Some(v) => {
                if !used.contains(&reference) {
                    used.push(reference);
                }
                Some(v)
            }
            None => {
                if !missing.contains(&reference) {
                    missing.push(reference);
                }
                None
            }
        }
    })
}

// ---- sending --------------------------------------------------------------------------------

pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub binary: bool,
    pub content_type: String,
    pub bytes: usize,
    pub truncated: bool,
    pub time_ms: u128,
    pub redirects: u32,
    /// Names of the vault items used (never their values).
    pub vault_used: Vec<String>,
}

fn is_text_type(ct: &str) -> bool {
    let c = ct.to_ascii_lowercase();
    c.is_empty()
        || c.starts_with("text/")
        || ["application/json", "application/xml", "application/javascript", "application/x-www-form-urlencoded", "application/graphql"].iter().any(|p| c.starts_with(p))
        || c.contains("+json")
        || c.contains("+xml")
}

/// `loc` (a Location header) resolved against the URL it came from.
pub fn resolve_location(base: &str, loc: &str) -> String {
    let lower = loc.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return loc.to_string();
    }
    let scheme_end = base.find("://").map(|i| i + 3).unwrap_or(0);
    let host_end = base[scheme_end..].find(['/', '?', '#']).map(|i| scheme_end + i).unwrap_or(base.len());
    let origin = &base[..host_end];
    if let Some(rest) = loc.strip_prefix("//") {
        return format!("{}//{rest}", &base[..base.find("//").unwrap_or(0)]);
    }
    if loc.starts_with('/') {
        return format!("{origin}{loc}");
    }
    let path = &base[host_end..];
    let dir = path.split(['?', '#']).next().unwrap_or("");
    let dir = &dir[..dir.rfind('/').map(|i| i + 1).unwrap_or(0)];
    format!("{origin}{}{loc}", if dir.is_empty() { "/" } else { dir })
}

fn host_of(url: &str) -> &str {
    let s = url.find("://").map(|i| i + 3).unwrap_or(0);
    let rest = &url[s..];
    &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())]
}

struct Raw {
    status: u16,
    reason: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    total: usize,
    truncated: bool,
}

fn perform(method: &str, url: &str, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Raw, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).max_redirects(0).timeout_global(Some(TIMEOUT)).build().into();
    let mut b = ureq::http::Request::builder().method(method).uri(url);
    for (k, v) in headers {
        b = b.header(k.as_str(), v.as_str());
    }
    let req = b.body(body.unwrap_or_default().to_vec()).map_err(|e| format!("Invalid request: {e}"))?;
    let mut res = agent.run(req).map_err(|e| match e {
        ureq::Error::Timeout(_) => format!("Request timed out after {}s", TIMEOUT.as_secs()),
        other => other.to_string(),
    })?;
    let status = res.status();
    let hdrs = res.headers().iter().map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
    let mut data = Vec::new();
    let mut reader = res.body_mut().as_reader().take(MAX_BODY as u64 + 1);
    let _ = reader.read_to_end(&mut data);
    let total = data.len();
    let truncated = total > MAX_BODY;
    data.truncate(MAX_BODY);
    Ok(Raw { status: status.as_u16(), reason: status.canonical_reason().unwrap_or("").to_string(), headers: hdrs, body: data, total, truncated })
}

/// Run `req`. `vault(reference)` supplies `{{vault:...}}` values.
pub fn send(req: &Request, vault: &mut dyn FnMut(&str) -> Option<String>) -> Result<Response, String> {
    let started = Instant::now();
    let (mut cache, mut used, mut missing) = (HashMap::new(), Vec::new(), Vec::new());
    let mut res_ = |t: &str| resolve_vault_refs(t, &mut cache, &mut *vault, &mut used, &mut missing);
    let mut url = res_(&req.url);
    let mut headers: Vec<(String, String)> = req.headers.iter().map(|(k, v)| (k.clone(), res_(v))).collect();
    let body = req.body.as_ref().map(|b| res_(b));
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|r| format!("{{{{vault:{r}}}}}")).collect();
        return Err(format!("Could not resolve {} - unlock the Passwords vault, or check the item name.", list.join(", ")));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(format!("Unsupported or invalid URL: {}", req.url));
    }
    let has = |h: &[(String, String)], n: &str| h.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    if !has(&headers, "user-agent") {
        headers.push(("User-Agent".into(), "NotePP/http".into()));
    }
    if !has(&headers, "accept") {
        headers.push(("Accept".into(), "*/*".into()));
    }

    let (mut method, mut payload, mut redirects) = (req.method.to_ascii_uppercase(), body.map(String::into_bytes).filter(|b| !b.is_empty()), 0u32);
    loop {
        let raw = perform(&method, &url, &headers, payload.as_deref())?;
        let location = raw.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location")).map(|(_, v)| v.clone());
        if let (Some(loc), 300..=399) = (location, raw.status) {
            if redirects < MAX_REDIRECTS {
                redirects += 1;
                let next = resolve_location(&url, &loc);
                // 303, and 301/302 in practice, turn into a GET without a body.
                if raw.status == 303 || ((raw.status == 301 || raw.status == 302) && method != "HEAD") {
                    method = "GET".into();
                    payload = None;
                    headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("content-length"));
                }
                // Credentials never follow a redirect to another host.
                if host_of(&next) != host_of(&url) {
                    headers.retain(|(k, _)| !k.eq_ignore_ascii_case("authorization") && !k.eq_ignore_ascii_case("cookie"));
                }
                url = next;
                continue;
            }
        }
        let content_type = raw.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).map(|(_, v)| v.clone()).unwrap_or_default();
        let text = is_text_type(&content_type);
        return Ok(Response {
            status: raw.status,
            status_text: raw.reason,
            headers: raw.headers,
            body: text.then(|| String::from_utf8_lossy(&raw.body).into_owned()),
            binary: !text,
            content_type,
            bytes: raw.total.min(MAX_BODY),
            truncated: raw.truncated,
            time_ms: started.elapsed().as_millis(),
            redirects,
            vault_used: used,
        });
    }
}

// ---- what the preview page is told ----------------------------------------------------------

pub fn pending_json(req: &Request) -> String {
    json!({ "state": "pending", "method": req.method, "url": req.url }).to_string()
}

pub fn error_json(req: &Request, error: &str) -> String {
    json!({ "state": "error", "method": req.method, "url": req.url, "error": error }).to_string()
}

pub fn response_json(req: &Request, res: &Response) -> String {
    json!({
        "state": "done", "method": req.method, "url": req.url,
        "status": res.status, "statusText": res.status_text, "contentType": res.content_type,
        "headers": res.headers, "body": res.body, "binary": res.binary, "bytes": res.bytes,
        "timeMs": res.time_ms as u64, "redirects": res.redirects, "truncated": res.truncated,
        "vault": res.vault_used,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    const SAMPLE: &str = "@base = https://api.example.com\n@token=abc\n\n### create a charge\nPOST {{base}}/v1/charges\nAuthorization: Bearer {{token}}\nContent-Type: application/json\n# a comment\n\n{ \"amount\": 2000 }\n\n###\n// second\nhttps://example.com/ping\n\n### vault\nGET {{base}}/me\nX-Key: {{vault:My Key.password}}\n";

    #[test]
    fn parses_the_request_under_the_cursor() {
        let r = request_at(SAMPLE, 5).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.example.com/v1/charges");
        assert_eq!(r.headers, vec![("Authorization".to_string(), "Bearer abc".to_string()), ("Content-Type".to_string(), "application/json".to_string())]);
        assert_eq!(r.body.as_deref(), Some("{ \"amount\": 2000 }"));
        assert_eq!(r.name.as_deref(), Some("create a charge"));

        let bare = request_at(SAMPLE, 13).unwrap();
        assert_eq!((bare.method.as_str(), bare.url.as_str()), ("GET", "https://example.com/ping"));
        assert!(bare.body.is_none());
    }

    #[test]
    fn falls_back_to_the_first_request_and_leaves_vault_refs_alone() {
        // cursor on the @var lines, before any request block
        assert_eq!(request_at(SAMPLE, 0).unwrap().method, "POST".to_string());
        let v = request_at(SAMPLE, 17).unwrap();
        assert_eq!(v.headers[0].1, "{{vault:My Key.password}}");
        assert!(request_at("just some notes\nnothing here", 0).is_none());
        assert!(request_at("", 0).is_none());
    }

    #[test]
    fn variables_and_placeholders() {
        let vars: HashMap<String, String> = [("a".to_string(), "1".to_string())].into();
        assert_eq!(expand("x{{a}}y{{ a }}z{{b}}{{vault:V}}", &vars), "x1y1z{{b}}{{vault:V}}");
        assert_eq!(expand("{{ unclosed", &vars), "{{ unclosed");
        assert_eq!(expand("ünï {{a}} ✓", &vars), "ünï 1 ✓");
    }

    #[test]
    fn vault_references_are_resolved_once_and_reported() {
        let (mut cache, mut used, mut missing) = (HashMap::new(), vec![], vec![]);
        let mut calls = 0;
        let mut look = |r: &str| {
            calls += 1;
            (r == "Known").then(|| "s3cret".to_string())
        };
        let out = resolve_vault_refs("{{vault:Known}} {{ VAULT:Known }} {{vault:Nope}}", &mut cache, &mut look, &mut used, &mut missing);
        assert_eq!(out, "s3cret s3cret {{vault:Nope}}");
        assert_eq!((used, missing), (vec!["Known".to_string()], vec!["Nope".to_string()]));
        assert_eq!(calls, 2);
    }

    #[test]
    fn relative_redirects() {
        assert_eq!(resolve_location("https://a.com/x/y?q=1", "/z"), "https://a.com/z");
        assert_eq!(resolve_location("https://a.com/x/y", "z"), "https://a.com/x/z");
        assert_eq!(resolve_location("https://a.com", "z"), "https://a.com/z");
        assert_eq!(resolve_location("https://a.com/x", "https://b.com/"), "https://b.com/");
        assert_eq!(resolve_location("https://a.com/x", "//b.com/p"), "https://b.com/p");
        assert_eq!(host_of("https://a.com:8080/x?y"), "a.com:8080");
    }

    #[test]
    fn text_and_binary_types() {
        assert!(is_text_type("") && is_text_type("text/html; charset=utf-8") && is_text_type("application/json") && is_text_type("application/vnd.api+json"));
        assert!(!is_text_type("image/png") && !is_text_type("application/octet-stream"));
    }

    /// A one-shot local server: answers each connection with the next canned reply.
    fn serve(replies: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for reply in replies {
                let (mut s, _) = l.accept().unwrap();
                // headers, then as many body bytes as Content-Length announces
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).into_owned();
                    let Some(end) = text.find("\r\n\r\n") else {
                        if n == 0 {
                            break;
                        }
                        continue;
                    };
                    let want: usize = text[..end].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())).unwrap_or(0);
                    if data.len() >= end + 4 + want || n == 0 {
                        break;
                    }
                }
                seen.push(String::from_utf8_lossy(&data).into_owned());
                let _ = s.write_all(reply.as_bytes());
            }
            seen
        });
        (addr, h)
    }

    #[test]
    fn sends_a_request_follows_a_302_as_get_and_reads_json() {
        let (base, server) = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: /done\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Thing: yes\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}".to_string(),
        ]);
        let req = Request {
            method: "POST".into(),
            url: format!("{base}/start"),
            headers: vec![("Authorization".into(), "Bearer {{vault:Tok}}".into()), ("Content-Type".into(), "application/json".into())],
            body: Some("{\"a\":1}".into()),
            name: None,
        };
        let res = send(&req, &mut |r| (r == "Tok").then(|| "SECRET".to_string())).unwrap();
        assert_eq!((res.status, res.redirects), (200, 1));
        assert_eq!(res.body.as_deref(), Some("{\"ok\":true}"));
        assert!(res.headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("x-thing") && v == "yes"));
        assert_eq!(res.vault_used, vec!["Tok".to_string()]);

        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("POST /start HTTP/1.1"), "{}", seen[0]);
        assert!(seen[0].contains("Bearer SECRET") && seen[0].contains("{\"a\":1}"));
        assert!(seen[1].starts_with("GET /done HTTP/1.1"), "{}", seen[1]);
        assert!(!seen[1].contains("{\"a\":1}"), "the body is dropped on the redirect");
        let json = response_json(&req, &res);
        assert!(!json.contains("SECRET"), "the secret never reaches the page");
    }

    #[test]
    fn missing_vault_item_and_bad_urls_are_reported() {
        let req = Request { method: "GET".into(), url: "https://x.y/{{vault:Nope}}".into(), headers: vec![], body: None, name: None };
        let e = send(&req, &mut |_| None).err().unwrap();
        assert!(e.contains("{{vault:Nope}}"), "{e}");
        let bad = Request { method: "GET".into(), url: "ftp://x".into(), headers: vec![], body: None, name: None };
        assert!(send(&bad, &mut |_| None).is_err());
    }
}
