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
    /// Let the live preview fetch images from the web. Off by default: a previewed
    /// document could otherwise report that it was opened. Needs a restart to change.
    pub preview_remote_images: bool,
    /// Show the Markdown formatting toolbar above Markdown files.
    pub md_toolbar: bool,
    /// Whether Markdown files open with the live preview showing (remembers the last choice).
    pub md_preview: bool,
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
            preview_remote_images: false,
            md_toolbar: true,
            md_preview: true,
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
        let path = settings_path();
        let Ok(text) = std::fs::read_to_string(&path) else { return Settings::default() };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            // Keep what the user had: the next save would otherwise overwrite it with defaults.
            let _ = std::fs::copy(&path, path.with_extension("json.bad"));
            Settings::default()
        })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = settings_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        super::files::atomic_write(&p, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?.as_bytes())
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

/// A ready-made colour scheme: name, dark?, and the colours in `ROLES` order.
pub struct Preset {
    pub name: &'static str,
    pub dark: bool,
    pub colors: [&'static str; 14],
}

/// Popular Notepad++ style themes, approximated from their published palettes.
pub const PRESETS: &[Preset] = &[
    Preset { name: "Monokai", dark: true, colors: ["#F8F8F2", "#272822", "#8F908A", "#272822", "#3E3D32", "#49483E", "#75715E", "#AE81FF", "#F92672", "#E6DB74", "#66D9EF", "#FD971F", "#A6E22E", "#FD971F"] },
    Preset { name: "Dracula", dark: true, colors: ["#F8F8F2", "#282A36", "#6272A4", "#282A36", "#44475A", "#44475A", "#6272A4", "#BD93F9", "#FF79C6", "#F1FA8C", "#8BE9FD", "#FF5555", "#50FA7B", "#FFB86C"] },
    Preset { name: "Solarized Dark", dark: true, colors: ["#839496", "#002B36", "#586E75", "#073642", "#0A3A47", "#134B5A", "#586E75", "#D33682", "#859900", "#2AA198", "#B58900", "#CB4B16", "#268BD2", "#6C71C4"] },
    Preset { name: "Solarized Light", dark: false, colors: ["#657B83", "#FDF6E3", "#93A1A1", "#EEE8D5", "#EEE8D5", "#D3CBB0", "#93A1A1", "#D33682", "#859900", "#2AA198", "#B58900", "#CB4B16", "#268BD2", "#6C71C4"] },
    Preset { name: "Obsidian", dark: true, colors: ["#E0E2E4", "#293134", "#81969A", "#293134", "#2F393C", "#3A4A4F", "#66747B", "#FFCD22", "#93C763", "#EC7600", "#678CB1", "#A082BD", "#F1F2F3", "#8CBBAD"] },
    Preset { name: "Zenburn", dark: true, colors: ["#DCDCCC", "#3F3F3F", "#9FAFAF", "#3F3F3F", "#4F4F4F", "#5F5F5F", "#7F9F7F", "#8CD0D3", "#F0DFAF", "#CC9393", "#DFDFBF", "#DFAF8F", "#EFEF8F", "#93E0E3"] },
    Preset { name: "Twilight", dark: true, colors: ["#F8F8F8", "#141414", "#5F5A60", "#141414", "#222222", "#2D3033", "#5F5A60", "#CF6A4C", "#CDA869", "#8F9D6A", "#F9EE98", "#9B703F", "#9B859D", "#7587A6"] },
    Preset { name: "Deep Black", dark: true, colors: ["#E0E0E0", "#000000", "#808080", "#000000", "#141414", "#264F78", "#6A9955", "#B5CEA8", "#569CD6", "#CE9178", "#4EC9B0", "#C586C0", "#DCDCAA", "#9CDCFE"] },
    Preset { name: "Bespin", dark: true, colors: ["#BAAE9E", "#2A211C", "#6D6458", "#2A211C", "#36312E", "#5E5D5C", "#666666", "#CF6A4C", "#5EA6EA", "#54BE0D", "#F9EE98", "#9B859D", "#937121", "#7587A6"] },
    Preset { name: "Vibrant Ink", dark: true, colors: ["#FFFFFF", "#000000", "#888888", "#000000", "#1A1A1A", "#3A3A5C", "#9933CC", "#99CC99", "#FF6600", "#66FF00", "#FFCC00", "#FFEE98", "#FFEE98", "#66CCFF"] },
    Preset { name: "Nord", dark: true, colors: ["#D8DEE9", "#2E3440", "#4C566A", "#2E3440", "#3B4252", "#434C5E", "#616E88", "#B48EAD", "#81A1C1", "#A3BE8C", "#8FBCBB", "#5E81AC", "#88C0D0", "#EBCB8B"] },
    Preset { name: "One Dark", dark: true, colors: ["#ABB2BF", "#282C34", "#636D83", "#282C34", "#2C313C", "#3E4451", "#5C6370", "#D19A66", "#C678DD", "#98C379", "#E5C07B", "#E06C75", "#61AFEF", "#56B6C2"] },
];

impl Settings {
    /// Make `p` the colour scheme of its light/dark mode (None = the built-in default).
    pub fn apply_preset(&mut self, dark: bool, p: Option<&Preset>) {
        let base = if dark { Theme::dark() } else { Theme::light() };
        let diff = match p {
            Some(p) => {
                let entered: Vec<(&str, String)> = ROLES.iter().zip(p.colors.iter()).map(|((_, k), h)| (*k, h.to_string())).collect();
                diff_from(&base, &entered)
            }
            None => BTreeMap::new(),
        };
        if dark { self.dark_colors = diff } else { self.light_colors = diff }
    }
}
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
    fn presets_are_complete_and_valid() {
        for p in PRESETS {
            assert_eq!(p.colors.len(), ROLES.len());
            assert!(p.colors.iter().all(|h| parse_hex(h).is_some()), "{}", p.name);
        }
        let mut s = Settings::default();
        s.apply_preset(true, Some(&PRESETS[0]));
        assert_eq!(themed(Theme::dark(), &s.dark_colors).bg, (0x27, 0x28, 0x22));
        s.apply_preset(true, None);
        assert!(s.dark_colors.is_empty());
    }

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
