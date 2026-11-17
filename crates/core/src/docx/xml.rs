//! Just enough XML to read a Word file's parts: tags (with their attributes, prefixes dropped) and text.

#[derive(Debug, PartialEq)]
pub enum Tok {
    /// An opening tag: the local name and the attributes by local name. A tag written `<x/>` gives a `Start` and an `End`.
    Start(String, Vec<(String, String)>),
    End(String),
    Text(String),
}

/// The attribute `name` of a tag.
pub fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix('#')
                .and_then(|n| n.strip_prefix(['x', 'X']).map_or_else(|| n.parse().ok(), |h| u32::from_str_radix(h, 16).ok()))
                .and_then(char::from_u32),
        };
        if let Some(c) = ch {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn attributes(mut s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        let Some(eq) = s.find('=') else { break };
        let name = s[..eq].trim();
        let after = s[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let body = &after[1..];
        let Some(end) = body.find(quote) else { break };
        out.push((local(name).to_string(), unescape(&body[..end])));
        s = &body[end + 1..];
    }
    out
}

/// The end of the tag that starts at `from` (the `>`), not counting a `>` inside a quoted value.
fn tag_end(s: &str, from: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, b) in s.bytes().enumerate().skip(from) {
        match (quote, b) {
            (None, b'"' | b'\'') => quote = Some(b),
            (Some(q), _) if q == b => quote = None,
            (None, b'>') => return Some(i),
            _ => {}
        }
    }
    None
}

pub fn tokens(xml: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < xml.len() {
        let Some(lt) = xml[at..].find('<').map(|i| i + at) else {
            out.push(Tok::Text(unescape(&xml[at..])));
            break;
        };
        if lt > at {
            out.push(Tok::Text(unescape(&xml[at..lt])));
        }
        let rest = &xml[lt..];
        let skip = |end: &str| rest.find(end).map_or(xml.len(), |i| lt + i + end.len());
        if rest.starts_with("<?") {
            at = skip("?>");
        } else if rest.starts_with("<!--") {
            at = skip("-->");
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").map_or(xml.len(), |i| lt + i);
            out.push(Tok::Text(xml[lt + 9..end].to_string()));
            at = (end + 3).min(xml.len());
        } else if rest.starts_with("<!") {
            at = tag_end(xml, lt).map_or(xml.len(), |e| e + 1);
        } else if let Some(name) = rest.strip_prefix("</") {
            let end = tag_end(xml, lt).unwrap_or(xml.len());
            let name = name.split(['>', ' ', '\t', '\r', '\n']).next().unwrap_or("");
            out.push(Tok::End(local(name).to_string()));
            at = (end + 1).min(xml.len());
        } else {
            let Some(end) = tag_end(xml, lt) else { break };
            let inner = &xml[lt + 1..end];
            let selfclosing = inner.ends_with('/');
            let name_end = inner.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(inner.len());
            let name = local(&inner[..name_end]).to_string();
            out.push(Tok::Start(name.clone(), attributes(&inner[name_end..])));
            if selfclosing {
                out.push(Tok::End(name));
            }
            at = end + 1;
        }
    }
    out
}
