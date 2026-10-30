//! Keyboard shortcuts as text ("Ctrl+Shift+K") and as Windows virtual-key
//! chords, for the Shortcut Mapper.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows virtual-key code.
    pub vk: u16,
}

const NAMED: &[(&str, u16)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Esc", 0x1B),
    ("Space", 0x20),
    ("PgUp", 0x21),
    ("PgDn", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Ins", 0x2D),
    ("Del", 0x2E),
    ("Num 0", 0x60),
    ("Num 1", 0x61),
    ("Num 2", 0x62),
    ("Num 3", 0x63),
    ("Num 4", 0x64),
    ("Num 5", 0x65),
    ("Num 6", 0x66),
    ("Num 7", 0x67),
    ("Num 8", 0x68),
    ("Num 9", 0x69),
    ("Num *", 0x6A),
    ("Num +", 0x6B),
    ("Num -", 0x6D),
    ("Num /", 0x6F),
    (";", 0xBA),
    ("=", 0xBB),
    (",", 0xBC),
    ("-", 0xBD),
    (".", 0xBE),
    ("/", 0xBF),
    ("`", 0xC0),
    ("[", 0xDB),
    ("\\", 0xDC),
    ("]", 0xDD),
    ("'", 0xDE),
];

fn key_vk(name: &str) -> Option<u16> {
    let n = name.trim();
    if n.len() == 1 {
        let c = n.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as u16);
        }
    }
    if let Some(num) = n.strip_prefix(['F', 'f']).and_then(|d| d.parse::<u16>().ok()) {
        if (1..=24).contains(&num) {
            return Some(0x6F + num);
        }
    }
    NAMED.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| *v)
}

fn vk_key(vk: u16) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some((vk as u8 as char).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6F)),
        _ => NAMED.iter().find(|(_, v)| *v == vk).map(|(k, _)| k.to_string()),
    }
}

/// Parse "Ctrl+Alt+Shift+Key". A letter or digit needs a modifier, or it
/// would stop that key from typing.
pub fn parse(text: &str) -> Option<Chord> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    // "Num +" and "Num -" contain the separator; protect them.
    let protected = t.replace("Num +", "Num\u{1}P").replace("num +", "Num\u{1}P").replace("Num -", "Num\u{1}M").replace("num -", "Num\u{1}M");
    let mut chord = Chord { ctrl: false, alt: false, shift: false, vk: 0 };
    let parts: Vec<&str> = protected.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last()?;
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" => chord.alt = true,
            "shift" => chord.shift = true,
            _ => return None,
        }
    }
    let key = key.replace("Num\u{1}P", "Num +").replace("Num\u{1}M", "Num -");
    chord.vk = key_vk(&key)?;
    let plain_typing = matches!(chord.vk, 0x30..=0x39 | 0x41..=0x5A | 0x20 | 0xBA..=0xDE) && !chord.ctrl && !chord.alt;
    (!plain_typing).then_some(chord)
}

pub fn format(c: &Chord) -> String {
    let mut s = String::new();
    for (on, name) in [(c.ctrl, "Ctrl+"), (c.alt, "Alt+"), (c.shift, "Shift+")] {
        if on {
            s.push_str(name);
        }
    }
    s + &vk_key(c.vk).unwrap_or_default()
}

/// Windows ACCEL flags: FVIRTKEY | FSHIFT | FCONTROL | FALT.
pub fn accel_flags(c: &Chord) -> u8 {
    1 | if c.shift { 4 } else { 0 } | if c.ctrl { 8 } else { 0 } | if c.alt { 0x10 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        for s in ["Ctrl+S", "Ctrl+Num 3", "Ctrl+Alt+Shift+F3", "F11", "Alt+Shift+0", "Ctrl+Num +", "Ctrl+Shift+PgDn", "Ctrl+/", "Shift+Tab", "Del"] {
            let c = parse(s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(format(&c), s);
        }
        let c = parse("ctrl + shift + k").unwrap();
        assert_eq!((c.ctrl, c.shift, c.alt, c.vk), (true, true, false, b'K' as u16));
        assert_eq!(accel_flags(&c), 1 | 4 | 8);
    }

    #[test]
    fn rejects_bad_shortcuts() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("Ctrl+"), None);
        assert_eq!(parse("Ctrl+Banana"), None);
        assert_eq!(parse("Hyper+K"), None);
        // A bare letter, digit or space would break typing.
        assert_eq!(parse("K"), None);
        assert_eq!(parse("Shift+K"), None);
        assert_eq!(parse("5"), None);
    }
}
