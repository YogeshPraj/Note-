//! A JSON Web Token in a text pad, decoded for the preview. Pure and portable: the shell only displays the HTML.
//!
//! The pad has to be a token and nothing else (a bare token, or `Authorization: Bearer <token>`, which is how one is copied
//! from a request or a log). The signature is shown, never checked: there is no key to check it with.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use super::escape;

/// The longest text looked at: a token is a few hundred bytes, and a pad this big is not one.
pub const MAX_LEN: usize = 16 * 1024;
/// Times further from now than this (in seconds, about 30 000 years) are shown as they are.
const TIME_LIMIT: i64 = 1 << 40;

/// A token's parts, decoded.
#[derive(Debug, PartialEq)]
pub struct Jwt {
    pub header: Value,
    pub payload: Value,
    /// The signature as written (base64url); empty for an unsecured token.
    pub signature: String,
}

/// The token a pad holds, if it holds one.
pub fn parse(text: &str) -> Option<Jwt> {
    if text.len() > MAX_LEN {
        return None;
    }
    let token = bare(text)?;
    let mut parts = token.split('.');
    let (header, payload, signature) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || header.is_empty() || payload.is_empty() || !signature.bytes().all(is_base64url) {
        return None;
    }
    let header = object(header)?;
    let payload = object(payload)?;
    header.get("alg")?.as_str()?;
    Some(Jwt { header, payload, signature: signature.to_string() })
}

fn is_base64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The token itself: without a `Bearer` or `Authorization: Bearer` in front, quotes round it or a comma after it.
fn bare(text: &str) -> Option<&str> {
    let mut s = text.trim();
    if s.get(..14).is_some_and(|p| p.eq_ignore_ascii_case("authorization:")) {
        s = s[14..].trim_start();
    }
    if s.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("bearer ")) {
        s = s[7..].trim_start();
    }
    let s = s.trim_matches(|c| matches!(c, '"' | '\'' | '`' | ',' | ';')).trim();
    (!s.is_empty() && !s.contains(char::is_whitespace)).then_some(s)
}

/// One part of the token: base64url text that is a JSON object.
fn object(part: &str) -> Option<Value> {
    if !part.trim_end_matches('=').bytes().all(is_base64url) {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok()?;
    serde_json::from_slice::<Value>(&bytes).ok().filter(Value::is_object)
}

/// `secs` since 1970 as `2026-11-14 10:30:00 UTC`, or `None` for a time too far away to be one.
pub fn utc(secs: i64) -> Option<String> {
    if secs.unsigned_abs() > TIME_LIMIT.unsigned_abs() {
        return None;
    }
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970 to a calendar date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let m = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * m + 2) / 5 + 1;
    let month = if m < 10 { m + 3 } else { m - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest % 3600 / 60, rest % 60))
}

/// A length of time in its two biggest units: `3 days 4 hours`.
pub fn span(secs: i64) -> String {
    let secs = secs.saturating_abs();
    let units = [("day", 86_400), ("hour", 3600), ("minute", 60), ("second", 1)];
    let mut out: Vec<String> = Vec::new();
    let mut left = secs;
    for (name, size) in units {
        let n = left / size;
        left %= size;
        if n > 0 && out.len() < 2 {
            out.push(format!("{n} {name}{}", if n == 1 { "" } else { "s" }));
        } else if !out.is_empty() {
            break;
        }
    }
    if out.is_empty() { "0 seconds".into() } else { out.join(" ") }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn time_of(payload: &Value, claim: &str) -> Option<i64> {
    payload.get(claim)?.as_i64()
}

/// How the token stands at `now`: what to tell the reader first.
fn standing(jwt: &Jwt, now: i64) -> Vec<String> {
    let mut lines = Vec::new();
    if jwt.header["alg"].as_str().is_some_and(|a| a.eq_ignore_ascii_case("none")) {
        lines.push("**Unsecured token**: its algorithm is `none`, so it has no signature and anyone could have written it.".to_string());
    }
    if let Some(exp) = time_of(&jwt.payload, "exp") {
        lines.push(if exp <= now {
            format!("**Expired** {} ago.", span(now.saturating_sub(exp)))
        } else {
            format!("**Not expired**: {} left.", span(exp.saturating_sub(now)))
        });
    } else {
        lines.push("It has no expiry (`exp`).".to_string());
    }
    if let Some(nbf) = time_of(&jwt.payload, "nbf").filter(|n| *n > now) {
        lines.push(format!("**Not valid yet**: it starts in {}.", span(nbf.saturating_sub(now))));
    }
    lines
}

/// The words with `**bold**` and `` `code` `` made into the tags.
fn inline(s: &str) -> String {
    let mut out = String::new();
    for (i, bold) in s.split("**").enumerate() {
        let mut part = String::new();
        for (j, code) in bold.split('`').enumerate() {
            part.push_str(&if j % 2 == 1 { format!("<code>{}</code>", escape(code)) } else { escape(code) });
        }
        out.push_str(&if i % 2 == 1 { format!("<strong>{part}</strong>") } else { part });
    }
    out
}

const CLAIMS: [(&str, &str); 7] =
    [("iss", "Issuer"), ("sub", "Subject"), ("aud", "Audience"), ("exp", "Expires"), ("nbf", "Not before"), ("iat", "Issued at"), ("jti", "Token id")];

fn claim_value(name: &str, v: &Value, now: i64) -> String {
    if let (true, Some(secs)) = (matches!(name, "exp" | "nbf" | "iat"), v.as_i64())
        && let Some(when) = utc(secs)
    {
        let rel = if secs <= now { format!("{} ago", span(now.saturating_sub(secs))) } else { format!("in {}", span(secs.saturating_sub(now))) };
        return format!("{} ({})", escape(&when), escape(&rel));
    }
    match v {
        Value::String(s) => escape(s),
        other => escape(&other.to_string()),
    }
}

/// The page for the preview pane (HTML, in the markdown page's styles) for a token seen at `now` (seconds since 1970).
pub fn html(jwt: &Jwt, now: i64) -> String {
    let mut out = String::from("<h1>JSON Web Token</h1>\n");
    out.push_str("<blockquote>");
    for line in standing(jwt, now) {
        out.push_str(&format!("<p>{}</p>", inline(&line)));
    }
    out.push_str("</blockquote>\n");

    let rows: Vec<String> = CLAIMS
        .iter()
        .filter_map(|(name, label)| {
            jwt.payload.get(*name).map(|v| format!("<tr><td>{label}</td><td><code>{name}</code></td><td>{}</td></tr>", claim_value(name, v, now)))
        })
        .collect();
    if !rows.is_empty() {
        out.push_str("<h2>Claims</h2>\n<table><thead><tr><th>Claim</th><th>Name</th><th>Value</th></tr></thead><tbody>");
        out.push_str(&rows.concat());
        out.push_str("</tbody></table>\n");
    }
    out.push_str(&format!("<h2>Header</h2>\n<pre><code>{}</code></pre>\n", escape(&pretty(&jwt.header))));
    out.push_str(&format!("<h2>Payload</h2>\n<pre><code>{}</code></pre>\n", escape(&pretty(&jwt.payload))));
    out.push_str("<h2>Signature</h2>\n");
    if jwt.signature.is_empty() {
        out.push_str("<p>There is none.</p>\n");
    } else {
        out.push_str(&format!("<pre><code>{}</code></pre>\n", escape(&jwt.signature)));
    }
    out.push_str("<p><em>The signature is not checked: Note++ has no key for it. Decoding a token shows what it says, not that it is genuine.</em></p>\n");
    out
}

/// Seconds since 1970 now.
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().and_then(|d| i64::try_from(d.as_secs()).ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests;
