//! Extended search mode: \n, \t, \x.. and friends in the Find box.

// Up to max digits of `radix`, as a character.
pub(super) fn number(it: &mut std::iter::Peekable<std::str::Chars<'_>>, radix: u32, max: usize) -> Option<char> {
    let mut n = 0u32;
    let mut got = 0;
    while got < max {
        match it.peek().and_then(|c| c.to_digit(radix)) {
            Some(d) => {
                n = n * radix + d;
                it.next();
                got += 1;
            }
            None => break,
        }
    }
    if got == 0 { None } else { char::from_u32(n) }
}

/// Expand the escapes of the Extended search mode.
pub fn unescape_ext(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some('n') => {
                it.next();
                out.push('\n');
            }
            Some('r') => {
                it.next();
                out.push('\r');
            }
            Some('t') => {
                it.next();
                out.push('\t');
            }
            Some('0') => {
                it.next();
                out.push('\0');
            }
            Some('\\') => {
                it.next();
                out.push('\\');
            }
            Some(k @ ('x' | 'd' | 'o' | 'b' | 'u')) => {
                it.next();
                let (radix, max) = match k {
                    'x' => (16, 2),
                    'd' => (10, 3),
                    'o' => (8, 3),
                    'b' => (2, 8),
                    _ => (16, 4),
                };
                if let Some(ch) = number(&mut it, radix, max) {
                    out.push(ch);
                } else {
                    out.push('\\');
                    out.push(k);
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}
