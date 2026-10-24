//! Plugins > NppExport: turn the styled document into HTML or RTF.

use super::sci::{self, Send};

/// How one Scintilla style looks. Colours are (r, g, b).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look {
    pub fore: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

/// Text split into runs of one style each. Runs only break between characters.
pub fn runs(text: &[u8], styles: &[u8]) -> Vec<(u8, String)> {
    let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
    for (i, b) in text.iter().enumerate() {
        let style = styles.get(i).copied().unwrap_or(0);
        let continuation = (0x80..0xC0).contains(b);
        match out.last_mut() {
            Some((s, bytes)) if *s == style || continuation => bytes.push(*b),
            _ => out.push((style, vec![*b])),
        }
    }
    out.into_iter().map(|(s, b)| (s, String::from_utf8_lossy(&b).into_owned())).collect()
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn html(title: &str, font: &str, size: i32, bg: (u8, u8, u8), runs: &[(u8, String)], look: &dyn Fn(u8) -> Look) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut out = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n<div style=\"float:left;white-space:pre;font-family:'{}',monospace;font-size:{}pt;background:{};\">",
        esc(title),
        font,
        size,
        hex(bg)
    );
    for (style, text) in runs {
        let l = look(*style);
        let mut css = format!("color:{};", hex(l.fore));
        if l.bold {
            css.push_str("font-weight:bold;");
        }
        if l.italic {
            css.push_str("font-style:italic;");
        }
        out.push_str(&format!("<span style=\"{css}\">{}</span>", esc(&text.replace("\r\n", "\n").replace('\r', "\n"))));
    }
    out.push_str("</div>\n</body>\n</html>\n");
    out
}

pub fn rtf(font: &str, size: i32, runs: &[(u8, String)], look: &dyn Fn(u8) -> Look) -> String {
    let mut table: Vec<(u8, u8, u8)> = Vec::new();
    let mut body = String::new();
    for (style, text) in runs {
        let l = look(*style);
        let idx = match table.iter().position(|c| *c == l.fore) {
            Some(i) => i,
            None => {
                table.push(l.fore);
                table.len() - 1
            }
        };
        body.push_str(&format!("{{\\cf{}{}{} ", idx + 1, if l.bold { "\\b" } else { "" }, if l.italic { "\\i" } else { "" }));
        for ch in text.replace("\r\n", "\n").replace('\r', "\n").chars() {
            match ch {
                '\\' | '{' | '}' => {
                    body.push('\\');
                    body.push(ch);
                }
                '\n' => body.push_str("\\par\n"),
                '\t' => body.push_str("\\tab "),
                c if (c as u32) < 128 => body.push(c),
                c => {
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        body.push_str(&format!("\\u{}?", *u as i16));
                    }
                }
            }
        }
        body.push('}');
    }
    let colors: String = table.iter().map(|(r, g, b)| format!("\\red{r}\\green{g}\\blue{b};")).collect();
    format!("{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0\\fmodern {font};}}}}{{\\colortbl;{colors}}}\\f0\\fs{}\n{body}}}", size * 2)
}

/// The document's text with the style of each byte, and the looks the styles resolve to.
pub struct Styled {
    pub text: Vec<u8>,
    pub styles: Vec<u8>,
}

pub fn read_styled<S: Send>(sci: &S) -> Styled {
    let len = sci::length(sci).max(0) as usize;
    let mut text = vec![0u8; len + 1];
    sci.send(sci::SCI_GETTEXT, len + 1, text.as_mut_ptr() as isize);
    text.truncate(len);
    let styles = (0..len).map(|i| sci.send(SCI_GETSTYLEAT, i, 0) as u8).collect();
    Styled { text, styles }
}

pub fn look_of<S: Send>(sci: &S, style: u8) -> Look {
    let c = sci.send(SCI_STYLEGETFORE, style as usize, 0) as u32;
    Look {
        fore: ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8),
        bold: sci.send(SCI_STYLEGETBOLD, style as usize, 0) != 0,
        italic: sci.send(SCI_STYLEGETITALIC, style as usize, 0) != 0,
    }
}

pub const SCI_GETSTYLEAT: u32 = 2010;
pub const SCI_STYLEGETFORE: u32 = 2481;
pub const SCI_STYLEGETBOLD: u32 = 2483;
pub const SCI_STYLEGETITALIC: u32 = 2484;

#[cfg(test)]
mod tests {
    use super::*;

    fn look(s: u8) -> Look {
        Look { fore: if s == 1 { (255, 0, 0) } else { (0, 0, 0) }, bold: s == 1, italic: false }
    }

    #[test]
    fn runs_group_by_style_and_keep_characters_whole() {
        let r = runs(b"ab\x01cd", &[0, 0, 1, 1, 1]);
        assert_eq!(r, vec![(0, "ab".to_string()), (1, "\u{1}cd".to_string())]);
        // A multi-byte character whose bytes carry different styles stays together.
        let r = runs("aé".as_bytes(), &[0, 0, 1]);
        assert_eq!(r, vec![(0, "aé".to_string())]);
    }

    #[test]
    fn html_escapes_and_colours() {
        let out = html("t<", "Consolas", 10, (255, 255, 255), &[(1, "if <a>&".into()), (0, "\r\nx".into())], &look);
        assert!(out.contains("<title>t&lt;</title>"));
        assert!(out.contains("<span style=\"color:#FF0000;font-weight:bold;\">if &lt;a&gt;&amp;</span>"));
        assert!(out.contains("<span style=\"color:#000000;\">\nx</span>"));
        assert!(out.contains("background:#FFFFFF"));
    }

    #[test]
    fn rtf_has_colour_table_and_escapes() {
        let out = rtf("Consolas", 10, &[(1, "a{b}\\".into()), (0, "é\n".into())], &look);
        assert!(out.starts_with("{\\rtf1"));
        assert!(out.contains("\\red255\\green0\\blue0;\\red0\\green0\\blue0;"));
        assert!(out.contains("{\\cf1\\b a\\{b\\}\\\\}"));
        assert!(out.contains("\\u233?\\par"));
        assert!(out.ends_with('}'));
    }
}
