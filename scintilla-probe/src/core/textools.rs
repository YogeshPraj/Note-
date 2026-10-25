//! Text tools: Base64, URL encoding, and JSON / XML formatting.

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        if c == '=' {
            break;
        }
        // Accept the URL-safe alphabet too.
        let c = match c {
            '-' => '+',
            '_' => '/',
            c => c,
        };
        let v = B64.iter().position(|b| *b as char == c).ok_or_else(|| format!("'{c}' is not a Base64 character"))? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if b.get(i + 1..i + 3).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn json_pretty(text: &str) -> Result<String, String> {
    use serde::Serialize;
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"    "));
    v.serialize(&mut ser).map_err(|e| e.to_string())?;
    String::from_utf8(out).map_err(|e| e.to_string())
}

pub fn json_minify(text: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

#[derive(Debug, PartialEq)]
enum Tok<'a> {
    Open(&'a str),
    Close(&'a str),
    /// Self-closing tags, comments, processing instructions, doctype, CDATA.
    Standalone(&'a str),
    Text(&'a str),
}

fn tokens(xml: &str) -> Result<Vec<Tok<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('<') {
            let end_marker = if r.starts_with("!--") {
                "-->"
            } else if r.starts_with("![CDATA[") {
                "]]>"
            } else if r.starts_with('?') {
                "?>"
            } else {
                ">"
            };
            let end = r.find(end_marker).ok_or("an XML tag is not closed")? + end_marker.len();
            let tag = &rest[..end + 1];
            rest = &rest[end + 1..];
            out.push(if tag.starts_with("</") {
                Tok::Close(tag)
            } else if tag.ends_with("/>") || tag.starts_with("<!") || tag.starts_with("<?") {
                Tok::Standalone(tag)
            } else {
                Tok::Open(tag)
            });
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            out.push(Tok::Text(&rest[..end]));
            rest = &rest[end..];
        }
    }
    Ok(out)
}

pub fn xml_pretty(text: &str) -> Result<String, String> {
    let toks: Vec<Tok> = tokens(text)?.into_iter().filter(|t| !matches!(t, Tok::Text(s) if s.trim().is_empty())).collect();
    let (mut out, mut depth, mut i) = (Vec::<String>::new(), 0usize, 0usize);
    let pad = |d: usize| "    ".repeat(d);
    while i < toks.len() {
        match &toks[i] {
            Tok::Open(o) => {
                // <a>text</a> stays on one line.
                if let (Some(Tok::Text(t)), Some(Tok::Close(c))) = (toks.get(i + 1), toks.get(i + 2)) {
                    out.push(format!("{}{o}{}{c}", pad(depth), t.trim()));
                    i += 3;
                    continue;
                }
                out.push(format!("{}{o}", pad(depth)));
                depth += 1;
            }
            Tok::Close(c) => {
                depth = depth.saturating_sub(1);
                out.push(format!("{}{c}", pad(depth)));
            }
            Tok::Standalone(s) => out.push(format!("{}{s}", pad(depth))),
            Tok::Text(t) => out.push(format!("{}{}", pad(depth), t.trim())),
        }
        i += 1;
    }
    Ok(out.join("\n"))
}

pub fn xml_minify(text: &str) -> Result<String, String> {
    Ok(tokens(text)?
        .into_iter()
        .filter(|t| !matches!(t, Tok::Text(s) if s.trim().is_empty()))
        .map(|t| match t {
            Tok::Open(s) | Tok::Close(s) | Tok::Standalone(s) => s,
            Tok::Text(s) => s.trim(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode("Zm8=\n").unwrap(), b"fo");
        assert!(base64_decode("Zm!8").is_err());
        let data: Vec<u8> = (0..=255).collect();
        assert_eq!(base64_decode(&base64_encode(&data)).unwrap(), data);
    }

    #[test]
    fn url_round_trips() {
        assert_eq!(url_encode("a b&c/é"), "a%20b%26c%2F%C3%A9");
        assert_eq!(url_decode("a%20b%26c%2F%C3%A9+x"), "a b&c/é x");
        assert_eq!(url_decode("100%"), "100%");
    }

    #[test]
    fn json_formatting_keeps_key_order() {
        let src = r#"{"b":1,"a":[1,2],"c":{"x":null}}"#;
        let pretty = json_pretty(src).unwrap();
        assert_eq!(pretty, "{\n    \"b\": 1,\n    \"a\": [\n        1,\n        2\n    ],\n    \"c\": {\n        \"x\": null\n    }\n}");
        assert_eq!(json_minify(&pretty).unwrap(), src);
        assert!(json_pretty("{oops").is_err());
    }

    #[test]
    fn xml_formatting() {
        let src = "<?xml version=\"1.0\"?><root><a>1</a><b><c/><!-- note --></b></root>";
        let pretty = xml_pretty(src).unwrap();
        assert_eq!(pretty, "<?xml version=\"1.0\"?>\n<root>\n    <a>1</a>\n    <b>\n        <c/>\n        <!-- note -->\n    </b>\n</root>");
        assert_eq!(xml_minify(&pretty).unwrap(), src);
        assert!(xml_pretty("<a").is_err());
    }
}
