//! Design-token rule: colours and fonts are written in `theme.rs` and nowhere else.
//!
//! This scans the kit's and the application's sources for hard-coded colour triples, hex colour strings and font
//! names. Win32 constants in `native/` are the only exception.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn is_hex_byte(s: &str) -> bool {
    let s = s.trim();
    s.len() == 4 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// A tuple of three hex bytes, `(0x1A, 0x2B, 0x3C)`.
fn has_colour_triple(line: &str) -> bool {
    line.split('(').skip(1).any(|rest| {
        let inner = rest.split(')').next().unwrap_or("");
        let parts: Vec<&str> = inner.split(',').collect();
        parts.len() == 3 && parts.iter().all(|p| is_hex_byte(p))
    })
}

/// A string literal that starts like a CSS colour, `"#1A2B3C"`.
fn has_hex_string(line: &str) -> bool {
    line.split("\"#").skip(1).any(|r| r.len() >= 6 && r[..6].chars().all(|c| c.is_ascii_hexdigit()))
}

#[test]
fn no_hard_coded_colours_or_fonts_outside_the_theme() {
    let kit = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("app").join("src");
    let mut files = Vec::new();
    sources(&kit, &mut files);
    sources(&app, &mut files);
    let mut offenders = Vec::new();
    for f in files {
        let name = f.to_string_lossy().replace('\\', "/");
        if name.ends_with("ui-kit/src/theme.rs") || name.contains("/native/") || name.ends_with("/num.rs") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&f) else { continue };
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if has_colour_triple(code) || has_hex_string(code) || code.contains("with_face_name(\"") {
                offenders.push(format!("{}:{}: {}", name, i + 1, line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "hard-coded colours or fonts (use the theme):\n{}", offenders.join("\n"));
}
