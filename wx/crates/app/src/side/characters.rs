//! The character table: every byte value with its hex code and what it stands for.

use super::{Activate, Content};

const CONTROL_NAMES: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS",
    "US",
];

/// The character a table row stands for (Windows-1252 above 127).
pub fn char_for(n: u8) -> char {
    if n < 128 {
        n as char
    } else {
        encoding_rs::WINDOWS_1252.decode(&[n]).0.chars().next().unwrap_or('?')
    }
}

/// What the "Char" column shows: a control code's name, DEL, nothing for unmapped bytes, else the character.
pub fn label_for(n: u8) -> String {
    match n {
        0..=31 => CONTROL_NAMES[n as usize].to_string(),
        127 => "DEL".into(),
        128..=159 if char_for(n) == '\u{FFFD}' || char_for(n).is_control() => String::new(),
        _ => char_for(n).to_string(),
    }
}

pub struct Characters;

impl Content for Characters {
    fn columns(&self) -> Vec<(&'static str, i32)> {
        vec![("Value", 60), ("Hex", 60), ("Char", 100)]
    }

    fn rows(&self) -> Vec<Vec<String>> {
        (0..=255u8).map(|n| vec![n.to_string(), format!("{n:02X}"), label_for(n)]).collect()
    }

    fn activate(&self, row: usize) -> Option<Activate> {
        u8::try_from(row).ok().map(|n| Activate::Insert(char_for(n).to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_a_row_for_every_byte() {
        let rows = Characters.rows();
        assert_eq!(rows.len(), 256);
        assert_eq!(rows[0], vec!["0", "00", "NUL"]);
        assert_eq!(rows[255][1], "FF");
    }

    #[test]
    fn control_codes_are_named() {
        assert_eq!(label_for(10), "LF");
        assert_eq!(label_for(9), "TAB");
        assert_eq!(label_for(27), "ESC");
        assert_eq!(label_for(127), "DEL");
    }

    #[test]
    fn printable_ascii_stands_for_itself() {
        assert_eq!(label_for(b'A'), "A");
        assert_eq!(label_for(b' '), " ");
    }

    #[test]
    fn high_bytes_use_windows_1252() {
        assert_eq!(char_for(0xE9), '\u{e9}');
        assert_eq!(char_for(0x80), '\u{20ac}');
    }

    #[test]
    fn unmapped_bytes_show_nothing() {
        assert_eq!(label_for(0x81), "");
    }

    #[test]
    fn activating_a_row_inserts_its_character() {
        assert_eq!(Characters.activate(65), Some(Activate::Insert("A".into())));
        assert_eq!(Characters.activate(300), None);
    }
}
