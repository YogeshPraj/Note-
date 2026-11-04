//! Sending a request and describing the response for the preview page.

use super::*;

// ---- sending --------------------------------------------------------------------------------

#[derive(Debug)]
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

pub(super) fn is_text_type(ct: &str) -> bool {
    let c = ct.to_ascii_lowercase();
    c.is_empty()
        || c.starts_with("text/")
        || ["application/json", "application/xml", "application/javascript", "application/x-www-form-urlencoded", "application/graphql"]
            .iter()
            .any(|p| c.starts_with(p))
        || c.contains("+json")
        || c.contains("+xml")
}

/// `loc` (a Location header) resolved against the URL it came from.
pub fn resolve_location(base: &str, loc: &str) -> String {
    let lower = loc.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return loc.to_string();
    }
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let host_end = base[scheme_end..].find(['/', '?', '#']).map_or(base.len(), |i| scheme_end + i);
    let origin = &base[..host_end];
    if let Some(rest) = loc.strip_prefix("//") {
        return format!("{}//{rest}", &base[..base.find("//").unwrap_or(0)]);
    }
    if loc.starts_with('/') {
        return format!("{origin}{loc}");
    }
    let path = &base[host_end..];
    let dir = path.split(['?', '#']).next().unwrap_or("");
    let dir = &dir[..dir.rfind('/').map_or(0, |i| i + 1)];
    format!("{origin}{}{loc}", if dir.is_empty() { "/" } else { dir })
}

pub fn host_of(url: &str) -> &str {
    let s = url.find("://").map_or(0, |i| i + 3);
    let rest = &url[s..];
    &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())]
}

/// May what was sent to `from` go on to `to`? Not to another host, and not from https down to plain http.
pub(super) fn keeps_credentials(from: &str, to: &str) -> bool {
    let secure = |u: &str| u.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://"));
    host_of(from) == host_of(to) && (secure(to) || !secure(from))
}

/// Does `name` carry a credential? The usual headers, and any the request fills from the vault.
fn secret_header(req: &Request, name: &str) -> bool {
    ["authorization", "cookie", "proxy-authorization"].iter().any(|h| h.eq_ignore_ascii_case(name))
        || req.headers.iter().any(|(k, v)| k.eq_ignore_ascii_case(name) && has_vault_ref(v))
}

/// 307 and 308 send the body again, so one with vault values in it stops at a redirect that credentials may not follow.
fn may_follow(req: &Request, status: u16, from: &str, to: &str) -> bool {
    keeps_credentials(from, to) || !(matches!(status, 307 | 308) && req.body.as_deref().is_some_and(has_vault_ref))
}

/// What Send has to ask before vault values leave for a place the user has not agreed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultConsent {
    /// Scheme and host: agreeing covers this, for the session.
    pub origin: String,
    pub question: String,
}

/// The question for `req`; `None` when it uses no vault value. A `.http` file can come from anyone and name any host.
pub fn vault_consent(req: &Request) -> Option<VaultConsent> {
    let used = has_vault_ref(&req.url) || req.headers.iter().any(|(_, v)| has_vault_ref(v)) || req.body.as_deref().is_some_and(has_vault_ref);
    if !used {
        return None;
    }
    let scheme = req.url.split("://").next().unwrap_or_default().to_ascii_lowercase();
    let host = host_of(&req.url);
    let warning = if scheme == "https" { "" } else { "\n\nThis address is not https: they would travel unencrypted." };
    let question =
        format!("This request sends values from your Passwords vault to {host}.{warning}\n\nSend only if you trust this file and that address. Send them?");
    Some(VaultConsent { origin: format!("{scheme}://{host}"), question })
}

struct Raw {
    status: u16,
    reason: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    total: usize,
    truncated: bool,
}

fn perform(method: &str, url: &str, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Raw, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).max_redirects(0).timeout_global(Some(TIMEOUT)).build().into();
    let mut b = ureq::http::Request::builder().method(method).uri(url);
    for (k, v) in headers {
        b = b.header(k.as_str(), v.as_str());
    }
    let req = b.body(body.unwrap_or_default().to_vec()).map_err(|e| Error::InvalidRequest(e.to_string()))?;
    let mut res = agent.run(req).map_err(|e| match e {
        ureq::Error::Timeout(_) => Error::Timeout(TIMEOUT.as_secs()),
        other => Error::Request(other.to_string()),
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
pub fn send(req: &Request, vault: &mut dyn FnMut(&str) -> Option<String>) -> Result<Response, Error> {
    let started = Instant::now();
    let (mut cache, mut used, mut missing) = (HashMap::new(), Vec::new(), Vec::new());
    let mut res_ = |t: &str| resolve_vault_refs(t, &mut cache, &mut *vault, &mut used, &mut missing);
    let mut url = res_(&req.url);
    let mut headers: Vec<(String, String)> = req.headers.iter().map(|(k, v)| (k.clone(), res_(v))).collect();
    let body = req.body.as_ref().map(|b| res_(b));
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|r| format!("{{{{vault:{r}}}}}")).collect();
        return Err(Error::UnresolvedVariables(list.join(", ")));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(Error::UnsupportedUrl(req.url.clone()));
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
        let target = location.filter(|_| (300..=399).contains(&raw.status) && redirects < MAX_REDIRECTS).map(|loc| resolve_location(&url, &loc));
        if let Some(next) = target.filter(|next| may_follow(req, raw.status, &url, next)) {
            redirects += 1;
            // 303, and 301/302 in practice, turn into a GET without a body.
            if raw.status == 303 || ((raw.status == 301 || raw.status == 302) && method != "HEAD") {
                method = "GET".into();
                payload = None;
                headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("content-length"));
            }
            if !keeps_credentials(&url, &next) {
                headers.retain(|(k, _)| !secret_header(req, k));
            }
            url = next;
            continue;
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
        "timeMs": crate::num::to_u64(res.time_ms), "redirects": res.redirects, "truncated": res.truncated,
        "vault": res.vault_used,
    })
    .to_string()
}
