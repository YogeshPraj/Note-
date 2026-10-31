//! Where a JSON tree node lives in the text: used by the JSON preview to put the editor on the
//! line of the node the user clicked. The scanner is forgiving (comments count as blanks, and
//! it never fails on text that is not quite JSON); it only follows the path it was given.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}

/// Parse a path as the preview sends it: a JSON array of strings and numbers.
pub fn parse_path(json: &str) -> Option<Vec<Seg>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.as_array()?
        .iter()
        .map(|s| match s {
            serde_json::Value::String(k) => Some(Seg::Key(k.clone())),
            serde_json::Value::Number(n) => n.as_u64().map(|i| Seg::Index(i as usize)),
            _ => None,
        })
        .collect()
}

struct Scan<'a> {
    b: &'a [u8],
    i: usize,
}

impl Scan<'_> {
    fn blanks(&mut self) {
        loop {
            match self.b.get(self.i) {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'/') if self.b.get(self.i + 1) == Some(&b'/') => {
                    while self.b.get(self.i).is_some_and(|c| *c != b'\n') {
                        self.i += 1;
                    }
                }
                Some(b'/') if self.b.get(self.i + 1) == Some(&b'*') => {
                    self.i += 2;
                    while self.i < self.b.len() && !(self.b[self.i] == b'*' && self.b.get(self.i + 1) == Some(&b'/')) {
                        self.i += 1;
                    }
                    self.i = (self.i + 2).min(self.b.len());
                }
                _ => break,
            }
        }
    }

    /// Read a string starting at the opening quote; returns its decoded text.
    fn string(&mut self) -> Option<String> {
        let start = self.i;
        if self.b.get(start) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    let raw = std::str::from_utf8(&self.b[start..self.i]).ok()?;
                    return serde_json::from_str::<String>(raw).ok();
                }
                _ => self.i += 1,
            }
        }
        None
    }

    fn skip_value(&mut self) -> Option<()> {
        self.blanks();
        match *self.b.get(self.i)? {
            b'"' => self.string().map(|_| ()),
            open @ (b'{' | b'[') => {
                let close = if open == b'{' { b'}' } else { b']' };
                self.i += 1;
                loop {
                    self.blanks();
                    match *self.b.get(self.i)? {
                        c if c == close => {
                            self.i += 1;
                            return Some(());
                        }
                        b',' | b':' => self.i += 1,
                        _ => self.skip_value()?,
                    }
                }
            }
            _ => {
                // number, true, false, null
                let start = self.i;
                while self.b.get(self.i).is_some_and(|c| !matches!(c, b',' | b'}' | b']' | b':') && !c.is_ascii_whitespace()) {
                    self.i += 1;
                }
                (self.i > start).then_some(())
            }
        }
    }

    fn find(&mut self, path: &[Seg]) -> Option<usize> {
        self.blanks();
        let Some((first, rest)) = path.split_first() else { return Some(self.i) };
        match (*self.b.get(self.i)?, first) {
            (b'{', Seg::Key(want)) => {
                self.i += 1;
                loop {
                    self.blanks();
                    let key_at = self.i;
                    let key = self.string()?;
                    self.blanks();
                    if self.b.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    if &key == want {
                        return if rest.is_empty() { Some(key_at) } else { self.find(rest) };
                    }
                    self.skip_value()?;
                    self.blanks();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        _ => return None,
                    }
                }
            }
            (b'[', Seg::Index(want)) => {
                self.i += 1;
                for n in 0.. {
                    self.blanks();
                    if self.b.get(self.i) == Some(&b']') {
                        return None;
                    }
                    if n == *want {
                        return self.find(rest);
                    }
                    self.skip_value()?;
                    self.blanks();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        _ => return None,
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// Byte offset of the node at `path`: the key for an object member, the value for an array item.
pub fn offset_of(text: &str, path: &[Seg]) -> Option<usize> {
    Scan { b: text.as_bytes(), i: 0 }.find(path)
}

/// 0-based line of the node at `path`.
pub fn line_of(text: &str, path: &[Seg]) -> Option<usize> {
    let off = offset_of(text, path)?;
    Some(text.as_bytes()[..off].iter().filter(|b| **b == b'\n').count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(s: &str) -> Seg {
        Seg::Key(s.into())
    }

    #[test]
    fn finds_object_members_and_array_items() {
        let t = "{\n  \"a\": 1,\n  \"Rules\": [\n    {\"x\": true},\n    {\n      \"y\": \"q\\\"uote\"\n    }\n  ]\n}";
        assert_eq!(line_of(t, &[]), Some(0));
        assert_eq!(line_of(t, &[k("a")]), Some(1));
        assert_eq!(line_of(t, &[k("Rules")]), Some(2));
        assert_eq!(line_of(t, &[k("Rules"), Seg::Index(0)]), Some(3));
        assert_eq!(line_of(t, &[k("Rules"), Seg::Index(1), k("y")]), Some(5));
        assert_eq!(line_of(t, &[k("missing")]), None);
        assert_eq!(line_of(t, &[k("Rules"), Seg::Index(9)]), None);
    }

    #[test]
    fn tolerates_comments_and_garbage() {
        let t = "{ // c\n \"a\": [1, 2, /* x */ 3],\n \"b\": 2 }";
        assert_eq!(line_of(t, &[k("b")]), Some(2));
        assert_eq!(line_of(t, &[k("a"), Seg::Index(2)]), Some(1));
        assert_eq!(line_of("{ not json", &[k("a")]), None);
        assert_eq!(line_of("", &[k("a")]), None);
    }

    #[test]
    fn parses_paths() {
        assert_eq!(parse_path("[\"a\",0,\"b\"]"), Some(vec![k("a"), Seg::Index(0), k("b")]));
        assert_eq!(parse_path("[true]"), None);
        assert_eq!(parse_path("nope"), None);
    }
}
