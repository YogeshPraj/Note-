use super::send::{host_of, is_text_type};
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
                let want: usize =
                    text[..end].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())).unwrap_or(0);
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
    assert!(e.to_string().contains("{{vault:Nope}}"), "{e}");
    let bad = Request { method: "GET".into(), url: "ftp://x".into(), headers: vec![], body: None, name: None };
    assert!(send(&bad, &mut |_| None).is_err());
}
