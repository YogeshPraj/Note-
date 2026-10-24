//! User preferences (Settings > Preferences, Style Configurator, Shortcut Mapper),
//! stored as JSON next to the session file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::sci::{Rgb, Theme};
use super::session::config_dir;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub font: String,
    pub font_size: i32,
    pub tab_width: i32,
    pub use_spaces: bool,
    /// Column of the vertical guide line; 0 = off.
    pub edge_column: i32,
    pub line_numbers: bool,
    pub fold_margin: bool,
    pub bookmark_margin: bool,
    pub caret_line: bool,
    pub smart_highlight: bool,
    pub show_toolbar: bool,
    pub show_tabbar: bool,
    pub show_statusbar: bool,
    /// Encoding key for new documents ("utf8", "utf8bom", "ansi", ...).
    pub new_doc_encoding: String,
    /// "crlf", "lf", "cr", or "" for the platform default.
    pub new_doc_eol: String,
    /// Colour overrides by role key ("#RRGGBB"), per theme.
    pub light_colors: BTreeMap<String, String>,
    pub dark_colors: BTreeMap<String, String>,
    /// Command id -> shortcut text ("" = shortcut removed). Absent = default.
    pub shortcuts: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: "Cascadia Code".into(),
            font_size: 11,
            tab_width: 4,
            use_spaces: true,
            edge_column: 0,
            line_numbers: true,
            fold_margin: true,
            bookmark_margin: true,
            caret_line: true,
            smart_highlight: true,
            show_toolbar: true,
            show_tabbar: true,
            show_statusbar: true,
            new_doc_encoding: "utf8".into(),
            new_doc_eol: String::new(),
            light_colors: BTreeMap::new(),
            dark_colors: BTreeMap::new(),
            shortcuts: BTreeMap::new(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = settings_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)
    }

    /// Keep values in a sane range after the user typed them.
    pub fn clamp(&mut self) {
        self.font_size = self.font_size.clamp(6, 72);
        self.tab_width = self.tab_width.clamp(1, 16);
        self.edge_column = self.edge_column.clamp(0, 500);
        if self.font.trim().is_empty() {
            self.font = Settings::default().font;
        }
    }

    pub fn colors(&self, dark: bool) -> &BTreeMap<String, String> {
        if dark { &self.dark_colors } else { &self.light_colors }
    }
}

/// Colour roles in the Style Configurator: (label, key).
pub const ROLES: &[(&str, &str)] = &[
    ("Default text", "fg"),
    ("Background", "bg"),
    ("Line numbers", "line_no_fg"),
    ("Line number background", "line_no_bg"),
    ("Current line", "caret_line"),
    ("Selection", "sel_bg"),
    ("Comments", "comment"),
    ("Numbers", "number"),
    ("Keywords", "keyword"),
    ("Strings", "string"),
    ("Types", "ty"),
    ("Macros / preprocessor", "mac"),
    ("Functions", "func"),
    ("Properties / attributes", "prop"),
];

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn to_hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

fn slot<'a>(t: &'a mut Theme, key: &str) -> Option<&'a mut Rgb> {
    Some(match key {
        "fg" => &mut t.fg,
        "bg" => &mut t.bg,
        "line_no_fg" => &mut t.line_no_fg,
        "line_no_bg" => &mut t.line_no_bg,
        "caret_line" => &mut t.caret_line,
        "sel_bg" => &mut t.sel_bg,
        "comment" => &mut t.comment,
        "number" => &mut t.number,
        "keyword" => &mut t.keyword,
        "string" => &mut t.string,
        "ty" => &mut t.ty,
        "mac" => &mut t.mac,
        "func" => &mut t.func,
        "prop" => &mut t.prop,
        _ => return None,
    })
}

/// The colour a theme has for a role key.
pub fn color_of(theme: &Theme, key: &str) -> Option<Rgb> {
    let mut t = theme.clone();
    slot(&mut t, key).map(|c| *c)
}

/// A theme with the user's overrides applied on top.
pub fn themed(mut base: Theme, overrides: &BTreeMap<String, String>) -> Theme {
    for (key, hex) in overrides {
        if let (Some(c), Some(s)) = (parse_hex(hex), slot(&mut base, key)) {
            *s = c;
        }
    }
    base
}

/// Overrides that differ from `base`, from `(key, hex)` pairs the user entered.
pub fn diff_from(base: &Theme, entered: &[(&str, String)]) -> BTreeMap<String, String> {
    entered
        .iter()
        .filter_map(|(k, hex)| {
            let c = parse_hex(hex)?;
            (color_of(base, k)? != c).then(|| (k.to_string(), to_hex(c)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        assert_eq!(parse_hex("#FF8000"), Some((255, 128, 0)));
        assert_eq!(parse_hex("00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(to_hex((1, 2, 255)), "#0102FF");
    }

    #[test]
    fn overrides_apply_and_diff() {
        let base = Theme::light();
        let mut map = BTreeMap::new();
        map.insert("keyword".to_string(), "#112233".to_string());
        map.insert("nonsense".to_string(), "#000000".to_string());
        let t = themed(base.clone(), &map);
        assert_eq!(t.keyword, (0x11, 0x22, 0x33));
        assert_eq!(t.string, base.string);
        let entered: Vec<(&str, String)> = ROLES.iter().map(|(_, k)| (*k, to_hex(color_of(&t, k).unwrap()))).collect();
        assert_eq!(diff_from(&base, &entered), BTreeMap::from([("keyword".to_string(), "#112233".to_string())]));
    }

    #[test]
    fn every_role_exists_in_the_theme() {
        let t = Theme::dark();
        assert!(ROLES.iter().all(|(_, k)| color_of(&t, k).is_some()));
    }

    #[test]
    fn clamping_and_defaults() {
        let mut s = Settings { font_size: 500, tab_width: 0, edge_column: -4, font: "  ".into(), ..Default::default() };
        s.clamp();
        assert_eq!((s.font_size, s.tab_width, s.edge_column, s.font.as_str()), (72, 1, 0, "Cascadia Code"));
        let back: Settings = serde_json::from_str("{\"tab_width\": 2}").unwrap();
        assert_eq!(back.tab_width, 2);
        assert!(back.line_numbers);
    }
}
