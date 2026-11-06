//! Just enough HTTP/1.1 for one request and one reply on a local connection, and the door check every request passes.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use super::access::same_secret;

const MAX_HEAD: usize = 16 * 1024;
/// The largest request: enough to write a large document, small enough not to be a way to fill the memory.
pub const MAX_BODY: usize = 32 * 1024 * 1024;

/// One HTTP request. Header names are in lower case.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first value of header `name` (lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// Why a request is not served: the status and what to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reject {
    pub status: u16,
    pub reason: &'static str,
}

const BAD: Reject = Reject { status: 400, reason: "bad request" };
const TIMEOUT: Reject = Reject { status: 408, reason: "too slow" };
const TOO_LARGE: Reject = Reject { status: 413, reason: "too large" };

/// The start of a request: its method, path and headers, with no body yet.
fn parse_head(head: &[u8]) -> Result<Request, Reject> {
    let text = std::str::from_utf8(head).map_err(|_| BAD)?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(path), Some(version)) = (first.next(), first.next(), first.next()) else { return Err(BAD) };
    if !version.starts_with("HTTP/1.") {
        return Err(BAD);
    }
    let headers = lines
        .filter(|l| !l.is_empty())
        .map(|l| l.split_once(':').map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string())).ok_or(BAD))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Request { method: method.to_string(), path: path.to_string(), headers, body: Vec::new() })
}

/// Fill `buf` from `r` until `done` says how much of it is wanted, or the time is up.
fn read_until(r: &mut impl Read, buf: &mut Vec<u8>, until: Instant, done: impl Fn(&[u8]) -> Result<Option<usize>, Reject>) -> Result<Option<usize>, Reject> {
    let mut chunk = [0u8; 8192];
    loop {
        if let Some(n) = done(buf)? {
            return Ok(Some(n));
        }
        if Instant::now() > until {
            return Err(TIMEOUT);
        }
        match r.read(&mut chunk) {
            Ok(0) if buf.is_empty() => return Ok(None),
            Ok(0) => return Err(BAD),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(TIMEOUT),
        }
    }
}

/// Read one request, taking at most `limit`. None when the other side closed without sending anything.
///
/// # Errors
/// The status to answer with: malformed, too large, too slow, or chunked (the clients send a length).
pub fn read_request(r: &mut impl Read, limit: Duration) -> Result<Option<Request>, Reject> {
    let until = Instant::now() + limit;
    let mut buf = Vec::new();
    let head_end = read_until(r, &mut buf, until, |b| match b.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(p) if p <= MAX_HEAD => Ok(Some(p)),
        None if b.len() <= MAX_HEAD + 4 => Ok(None),
        _ => Err(TOO_LARGE),
    })?;
    let Some(head_end) = head_end else { return Ok(None) };
    let mut request = parse_head(&buf[..head_end])?;
    if request.header("transfer-encoding").is_some() {
        return Err(Reject { status: 411, reason: "send a length" });
    }
    let length = request.header("content-length").map_or(Ok(0), |v| v.parse::<usize>().map_err(|_| BAD))?;
    if length > MAX_BODY {
        return Err(TOO_LARGE);
    }
    let start = head_end + 4;
    read_until(r, &mut buf, until, |b| Ok((b.len() >= start + length).then_some(0)))?;
    request.body = buf[start..start + length].to_vec();
    Ok(Some(request))
}

/// The door: a web page must not reach the server, whatever it sends, and everyone else needs the token.
///
/// # Errors
/// What to answer instead of serving. A browser always names the page's `Origin` on a cross-site request, and `Host`
/// stops a site that renames itself to this address.
pub fn gate(req: &Request, port: u16, token: &str) -> Result<(), Reject> {
    if req.header("origin").is_some() {
        return Err(Reject { status: 403, reason: "web pages are refused" });
    }
    let local = req.header("host").is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    if !local {
        return Err(Reject { status: 403, reason: "wrong host" });
    }
    let shown = req.header("authorization").and_then(|a| a.strip_prefix("Bearer ")).unwrap_or_default();
    if !same_secret(shown, token) {
        return Err(Reject { status: 401, reason: "wrong token" });
    }
    if req.path != "/mcp" {
        return Err(Reject { status: 404, reason: "not found" });
    }
    if req.method != "POST" && req.method != "DELETE" {
        return Err(Reject { status: 405, reason: "use POST" });
    }
    Ok(())
}

/// Write a reply and close.
pub fn respond(w: &mut impl Write, status: u16, body: &str) {
    respond_with(w, status, body, "");
}

/// Write a reply with `more` among its headers (each ending in a line break), and close. A failure to write means the other
/// side has gone, which is nobody's concern here.
pub fn respond_with(w: &mut impl Write, status: u16, body: &str, more: &str) {
    let name = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        _ => "Service Unavailable",
    };
    let extra = match status {
        401 => "WWW-Authenticate: Bearer\r\n",
        405 => "Allow: POST\r\n",
        _ => "",
    };
    let head =
        format!("HTTP/1.1 {status} {name}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}{more}Connection: close\r\n\r\n", body.len());
    let _ = w.write_all(head.as_bytes()).and_then(|()| w.write_all(body.as_bytes())).and_then(|()| w.flush());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOW: Duration = Duration::from_secs(5);

    fn read(raw: &str) -> Result<Option<Request>, Reject> {
        read_request(&mut raw.as_bytes(), SLOW)
    }

    fn request(extra: &str) -> Request {
        read(&format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:9\r\nAuthorization: Bearer tok\r\n{extra}\r\n")).unwrap().unwrap()
    }

    #[test]
    fn a_request_is_read_with_its_body() {
        let r = read("POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\nX-Up: A b\r\n\r\n{}").unwrap().unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str(), r.body.as_slice()), ("POST", "/mcp", b"{}".as_slice()));
        assert_eq!(r.header("x-up"), Some("A b"));
        assert_eq!(r.header("nope"), None);
    }

    #[test]
    fn a_closed_connection_is_nothing_and_broken_requests_are_refused() {
        assert_eq!(read(""), Ok(None));
        assert_eq!(read("POST /mcp"), Err(BAD), "cut off");
        assert_eq!(read("nonsense\r\n\r\n"), Err(BAD));
        assert_eq!(read("POST /mcp HTTP/1.1\r\nno colon\r\n\r\n"), Err(BAD));
        assert_eq!(read("POST /mcp HTTP/1.1\r\nContent-Length: x\r\n\r\n"), Err(BAD));
        assert_eq!(read("POST /mcp HTTP/1.1\r\nContent-Length: 9\r\n\r\n{}"), Err(BAD), "shorter than it said");
        assert_eq!(read("POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap_err().status, 411);
    }

    #[test]
    fn size_limits_hold_before_anything_is_read() {
        let big = format!("POST /mcp HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert_eq!(read(&big), Err(TOO_LARGE));
        let long = format!("POST /mcp HTTP/1.1\r\nX: {}", "a".repeat(MAX_HEAD + 10));
        assert_eq!(read(&long), Err(TOO_LARGE));
    }

    #[test]
    fn only_a_local_caller_with_the_token_gets_in() {
        assert_eq!(gate(&request(""), 9, "tok"), Ok(()));
        assert_eq!(gate(&request("Origin: https://evil.example\r\n"), 9, "tok").unwrap_err().status, 403);
        assert_eq!(gate(&request(""), 9, "other").unwrap_err().status, 401);
        assert_eq!(gate(&request(""), 10, "tok").unwrap_err().status, 403, "a host for another port");
        let mut no_token = request("");
        no_token.headers.retain(|(n, _)| n != "authorization");
        assert_eq!(gate(&no_token, 9, "tok").unwrap_err().status, 401);
        let mut rebound = request("");
        rebound.headers[0].1 = "evil.example:9".into();
        assert_eq!(gate(&rebound, 9, "tok").unwrap_err().status, 403);
    }

    #[test]
    fn paths_and_methods_are_checked_after_the_token() {
        let mut r = request("");
        r.path = "/other".into();
        assert_eq!(gate(&r, 9, "tok").unwrap_err().status, 404);
        assert_eq!(gate(&r, 9, "bad").unwrap_err().status, 401, "nothing is told without the token");
        let mut g = request("");
        g.method = "GET".into();
        assert_eq!(gate(&g, 9, "tok").unwrap_err().status, 405);
    }

    #[test]
    fn a_reply_says_its_length_and_closes() {
        let mut out = Vec::new();
        respond(&mut out, 200, "{}");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 2\r\n") && text.contains("Connection: close\r\n") && text.ends_with("\r\n\r\n{}"));
        let mut with = Vec::new();
        respond_with(&mut with, 200, "", "Mcp-Session-Id: abc\r\n");
        assert!(String::from_utf8(with).unwrap().contains("Mcp-Session-Id: abc\r\n"));
    }
}
