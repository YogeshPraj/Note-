//! Settings dialogs: Preferences and the Style Configurator, plus applying new settings.

use notepp_core::sci::Theme;
use notepp_core::settings::{self, Settings};
use notepp_core::workspace::Workspace;
use wxdragon::prelude::*;

use crate::sci::WxSci;

/// Take new settings: the workspace restyles its views, and they are saved.
pub fn apply_settings(ws: &mut Workspace<WxSci>, s: Settings) {
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    ws.set_settings(s);
    let _ = ws.settings.save();
    ws.set_margins_visible(true);
}

fn number(s: &str, fallback: i32) -> i32 {
    s.trim().parse().unwrap_or(fallback)
}

fn pick_font(owner: &Frame, family: &str, size: i32) -> Option<(String, i32)> {
    let fd = FontDialog::builder(owner).build();
    let _ = (family, size);
    if fd.show_modal() != ID_OK {
        return None;
    }
    let f = fd.get_font()?;
    Some((f.get_face_name(), f.get_point_size()))
}

/// Settings > Preferences. Returns the edited settings on OK. Laid out exactly like the Win32 shell's.
pub fn preferences(frame: &Frame, cur: &Settings) -> Option<Settings> {
    use crate::widgets::form::{Kind, Spec, ctl, run};
    const FONT: usize = 101;
    const SIZE: usize = 102;
    const TAB: usize = 103;
    const SPACES: usize = 104;
    const EDGE: usize = 105;
    const LINENO: usize = 106;
    const FOLD: usize = 107;
    const BOOKMARK: usize = 108;
    const CARET: usize = 109;
    const SMART: usize = 110;
    const TOOLBAR: usize = 111;
    const TABBAR: usize = 112;
    const STATUS: usize = 113;
    const E_UTF8: usize = 120;
    const E_BOM: usize = 121;
    const E_ANSI: usize = 122;
    const L_CRLF: usize = 124;
    const L_LF: usize = 125;
    const L_CR: usize = 126;
    const REMOTE: usize = 131;
    const LOCKMIN: usize = 140;
    const FONT_PICK: usize = 130;
    thread_local! {
        static OWNER: std::cell::RefCell<Option<Frame>> = const { std::cell::RefCell::new(None) };
    }
    OWNER.with(|o| *o.borrow_mut() = Some(*frame));
    fn on_click(f: &crate::widgets::form::Form, id: usize) {
        if id == FONT_PICK {
            let size = f.text(SIZE).trim().parse().unwrap_or(11);
            let owner = OWNER.with(|o| *o.borrow());
            if let Some(owner) = owner
                && let Some((name, pt)) = pick_font(&owner, f.text(FONT).trim(), size)
            {
                f.set_text(FONT, &name);
                f.set_text(SIZE, &pt.to_string());
            }
        }
    }
    let spec = Spec {
        title: "Preferences",
        size: (440, 534),
        ctls: vec![
            ctl(1000, Kind::Group, "Editor", 12, 8, 416, 118),
            ctl(1001, Kind::Label, "Font:", 24, 32, 56, 18),
            ctl(FONT, Kind::Edit, &cur.font, 84, 29, 146, 22),
            ctl(FONT_PICK, Kind::Button, "...", 234, 28, 30, 24),
            ctl(1002, Kind::Label, "Size:", 282, 32, 36, 18),
            ctl(SIZE, Kind::Edit, &cur.font_size.to_string(), 322, 29, 50, 22),
            ctl(1003, Kind::Label, "Tab size:", 24, 62, 60, 18),
            ctl(TAB, Kind::Edit, &cur.tab_width.to_string(), 84, 59, 50, 22),
            ctl(SPACES, Kind::Check, "Insert spaces instead of tabs", 160, 61, 250, 20),
            ctl(1004, Kind::Label, "Vertical guide at column (0 = none):", 24, 94, 220, 18),
            ctl(EDGE, Kind::Edit, &cur.edge_column.to_string(), 250, 91, 50, 22),
            ctl(1010, Kind::Group, "Display", 12, 132, 416, 112),
            ctl(LINENO, Kind::Check, "Line numbers", 24, 152, 180, 20),
            ctl(FOLD, Kind::Check, "Fold margin", 224, 152, 180, 20),
            ctl(BOOKMARK, Kind::Check, "Bookmark margin", 24, 176, 180, 20),
            ctl(CARET, Kind::Check, "Highlight current line", 224, 176, 190, 20),
            ctl(SMART, Kind::Check, "Highlight matching words", 24, 200, 190, 20),
            ctl(TOOLBAR, Kind::Check, "Toolbar", 224, 200, 90, 20),
            ctl(TABBAR, Kind::Check, "Tab bar", 24, 220, 90, 20),
            ctl(STATUS, Kind::Check, "Status bar", 224, 220, 120, 20),
            ctl(1020, Kind::Group, "New documents", 12, 250, 416, 100),
            ctl(1021, Kind::Label, "Encoding:", 24, 274, 64, 18),
            ctl(E_UTF8, Kind::Radio(true), "UTF-8", 92, 272, 70, 20),
            ctl(E_BOM, Kind::Radio(false), "UTF-8-BOM", 170, 272, 100, 20),
            ctl(E_ANSI, Kind::Radio(false), "ANSI", 278, 272, 70, 20),
            ctl(1022, Kind::Label, "Line ending:", 24, 312, 70, 18),
            ctl(L_CRLF, Kind::Radio(true), "Windows (CR LF)", 98, 310, 130, 20),
            ctl(L_LF, Kind::Radio(false), "Unix (LF)", 236, 310, 90, 20),
            ctl(L_CR, Kind::Radio(false), "Mac (CR)", 334, 310, 90, 20),
            ctl(1030, Kind::Group, "Live preview", 12, 356, 416, 52),
            ctl(REMOTE, Kind::Check, "Load images from the web (applies after restart)", 24, 378, 380, 20),
            ctl(1040, Kind::Group, "Passwords", 12, 414, 416, 52),
            ctl(1041, Kind::Label, "Lock the Bitwarden vault after idle minutes (0 = never):", 24, 438, 316, 18),
            ctl(LOCKMIN, Kind::Edit, &cur.passwords_auto_lock_min.to_string(), 346, 435, 50, 22),
        ],
        ok: "OK",
        buttons: None,
        on_change: Some(on_click),
        on_apply: None,
    };
    let v = run(frame, spec, |f| {
        for (id, on) in [
            (SPACES, cur.use_spaces),
            (LINENO, cur.line_numbers),
            (FOLD, cur.fold_margin),
            (BOOKMARK, cur.bookmark_margin),
            (CARET, cur.caret_line),
            (SMART, cur.smart_highlight),
            (REMOTE, cur.preview_remote_images),
            (TOOLBAR, cur.show_toolbar),
            (TABBAR, cur.show_tabbar),
            (STATUS, cur.show_statusbar),
        ] {
            f.set_checked(id, on);
        }
        f.set_checked(
            match cur.new_doc_encoding.as_str() {
                "utf8bom" => E_BOM,
                "ansi" => E_ANSI,
                _ => E_UTF8,
            },
            true,
        );
        f.set_checked(
            match cur.new_doc_eol.as_str() {
                "lf" => L_LF,
                "cr" => L_CR,
                _ => L_CRLF,
            },
            true,
        );
    })?;
    let mut s = Settings {
        font: v.text(FONT).trim().to_string(),
        font_size: number(v.text(SIZE), cur.font_size),
        tab_width: number(v.text(TAB), cur.tab_width),
        use_spaces: v.checked(SPACES),
        edge_column: number(v.text(EDGE), cur.edge_column),
        line_numbers: v.checked(LINENO),
        fold_margin: v.checked(FOLD),
        bookmark_margin: v.checked(BOOKMARK),
        caret_line: v.checked(CARET),
        smart_highlight: v.checked(SMART),
        preview_remote_images: v.checked(REMOTE),
        passwords_auto_lock_min: crate::platform::wx::to_u32(number(v.text(LOCKMIN), crate::platform::wx::to_i32(cur.passwords_auto_lock_min)).max(0)),
        show_toolbar: v.checked(TOOLBAR),
        show_tabbar: v.checked(TABBAR),
        show_statusbar: v.checked(STATUS),
        new_doc_encoding: if v.checked(E_BOM) {
            "utf8bom"
        } else if v.checked(E_ANSI) {
            "ansi"
        } else {
            "utf8"
        }
        .into(),
        new_doc_eol: if v.checked(L_LF) {
            "lf"
        } else if v.checked(L_CR) {
            "cr"
        } else {
            "crlf"
        }
        .into(),
        ..cur.clone()
    };
    s.clamp();
    Some(s)
}
pub struct StyleEdit {
    pub reset: bool,
    pub entered: Vec<(&'static str, String)>,
}

/// Settings > Style Configurator: the colours of the theme in use, as #RRGGBB. Laid out like the Win32 shell's.
pub fn style_configurator(frame: &Frame, theme: &Theme) -> Option<StyleEdit> {
    use crate::widgets::form::{Kind, Spec, ctl, run};
    const FIRST: usize = 100; // edit i is FIRST + i
    const PICK: usize = 200; // button i is PICK + i
    const RESET: usize = 300;
    thread_local! {
        static OWNER: std::cell::RefCell<Option<Frame>> = const { std::cell::RefCell::new(None) };
    }
    OWNER.with(|o| *o.borrow_mut() = Some(*frame));
    fn on_click(f: &crate::widgets::form::Form, id: usize) {
        if (PICK..PICK + settings::ROLES.len()).contains(&id) {
            let edit = FIRST + (id - PICK);
            let start = settings::parse_hex(&f.text(edit)).unwrap_or((0, 0, 0));
            let Some(owner) = OWNER.with(|o| *o.borrow()) else { return };
            let cd = ColourDialog::builder(&owner).with_initial_colour(crate::widgets::paint::colour(start)).build();
            if cd.show_modal() == ID_OK
                && let Some(c) = cd.get_colour()
            {
                f.set_text(edit, &settings::to_hex((c.r, c.g, c.b)));
            }
        }
    }
    let mut ctls =
        vec![ctl(10, Kind::Label, if theme.dark { "Colours of the dark theme (#RRGGBB):" } else { "Colours of the light theme (#RRGGBB):" }, 16, 12, 300, 18)];
    for (i, (label, key)) in settings::ROLES.iter().enumerate() {
        let y = 40 + crate::platform::wx::to_i32(i) * 26;
        let hex = settings::color_of(theme, key).map(settings::to_hex).unwrap_or_default();
        ctls.push(ctl(400 + i, Kind::Label, label, 16, y + 3, 170, 18));
        ctls.push(ctl(FIRST + i, Kind::Edit, &hex, 190, y, 84, 22));
        ctls.push(ctl(500 + i, Kind::Swatch, &(FIRST + i).to_string(), 282, y, 28, 22));
        ctls.push(ctl(PICK + i, Kind::Button, "...", 316, y - 1, 30, 24));
    }
    let n = crate::platform::wx::to_i32(settings::ROLES.len());
    ctls.push(ctl(RESET, Kind::Check, "Reset every colour to the default", 16, 40 + n * 26 + 6, 290, 20));
    let spec = Spec { title: "Style Configurator", size: (366, 40 + n * 26 + 78), ctls, ok: "OK", buttons: None, on_change: Some(on_click), on_apply: None };
    let v = run(frame, spec, |_| {})?;
    Some(StyleEdit {
        reset: v.checked(RESET),
        entered: settings::ROLES.iter().enumerate().map(|(i, (_, key))| (*key, v.text(FIRST + i).to_string())).collect(),
    })
}
