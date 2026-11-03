//! Small dialogs: Summary, Date Time (customized), the Column Editor and Find characters in range.

use super::*;

// ---- dialogs ---------------------------------------------------------------------------------------------

pub(super) fn info(app: &App, title: &str, text: &str) {
    crate::widgets::ask::tell(&app.frame, title, text, crate::widgets::ask::Icon::Info);
}

/// The Summary box: file facts and text statistics.
pub(super) fn summary(app: &App) {
    let ws = &app.ws;
    let counts = view::count_text(&sci::get_text(&ws.sci));
    let t = &ws.tabs[ws.active];
    let mut s = String::new();
    match &t.path {
        Some(p) => {
            s += &format!("Full file path: {}\n", p.display());
            if let Ok(md) = std::fs::metadata(p) {
                s += &format!("File size on disk: {} bytes\n", md.len());
            }
        }
        None => s += &format!("{} (not saved yet)\n", t.title),
    }
    s += &format!(
        "\nCharacters (without line endings): {}\nWords: {}\nLines: {}\nDocument length: {} bytes\nEncoding: {}\n",
        counts.chars,
        counts.words,
        counts.lines,
        counts.bytes,
        t.enc.label()
    );
    let sel = search::selected_text(&ws.sci);
    if !sel.is_empty() {
        let v = view::count_text(&sel);
        s += &format!("\nSelection: {} characters, {} words, {} lines", v.chars, v.words, v.lines);
    }
    info(app, "Summary", &s);
}

/// Ask for the picture used by Insert > Date Time (customized).
pub(super) fn ask_date_format(app: &App, current: &str) -> Option<String> {
    const FMT: usize = 101;
    let spec = Spec {
        title: "Date Time (customized)",
        size: (420, 140),
        ctls: vec![
            ctl(100, Kind::Label, "Format: yyyy MM dd HH mm ss tt dddd MMMM  ('quoted text' is kept as is)", 14, 12, 392, 34),
            ctl(FMT, Kind::Edit, current, 14, 52, 392, 24),
        ],
        ok: "Insert",
        buttons: None,
        on_change: None,
        on_apply: None,
    };
    let v = run(&app.frame, spec, |_| {})?;
    let f = v.text(FMT).trim().to_string();
    (!f.is_empty()).then_some(f)
}

/// The Column Editor: insert the same text, or a counting sequence, on every selected line.
#[allow(clippy::too_many_lines, reason = "a declarative dialog spec: one line per control")]
pub(super) fn column_editor(app: &mut App) -> Option<String> {
    const TEXT_R: usize = 100;
    const TEXT: usize = 101;
    const NUM_R: usize = 102;
    const INIT: usize = 104;
    const STEP: usize = 106;
    const REPEAT: usize = 108;
    const ZEROS: usize = 109;
    const DEC: usize = 111;
    const HEX: usize = 112;
    const OCT: usize = 113;
    const BIN: usize = 114;
    fn react(f: &crate::widgets::form::Form, _id: usize) {
        let text = f.checked(TEXT_R);
        f.enable(TEXT, text);
        for id in [103, INIT, 105, STEP, 107, REPEAT, ZEROS, DEC, HEX, OCT, BIN] {
            f.enable(id, !text);
        }
    }
    let spec = Spec {
        title: "Column / Multi-Selection Editor",
        size: (380, 262),
        ctls: vec![
            ctl(TEXT_R, Kind::Radio(true), "&Text to Insert", 14, 12, 200, 20),
            ctl(TEXT, Kind::Edit, "", 30, 36, 330, 24),
            ctl(NUM_R, Kind::Radio(false), "&Number to Insert", 14, 72, 200, 20),
            ctl(103, Kind::Label, "Initial number:", 30, 98, 110, 20),
            ctl(INIT, Kind::Edit, "1", 150, 95, 100, 24),
            ctl(105, Kind::Label, "Increase by:", 30, 128, 110, 20),
            ctl(STEP, Kind::Edit, "1", 150, 125, 100, 24),
            ctl(107, Kind::Label, "Repeat:", 30, 158, 110, 20),
            ctl(REPEAT, Kind::Edit, "1", 150, 155, 100, 24),
            ctl(ZEROS, Kind::Check, "Leading &zeros", 30, 188, 200, 20),
            ctl(110, Kind::Group, "Format", 268, 86, 92, 122),
            ctl(DEC, Kind::Radio(true), "&Dec", 280, 106, 70, 20),
            ctl(HEX, Kind::Radio(false), "&Hex", 280, 130, 70, 20),
            ctl(OCT, Kind::Radio(false), "&Oct", 280, 154, 70, 20),
            ctl(BIN, Kind::Radio(false), "&Bin", 280, 178, 70, 20),
        ],
        ok: "OK",
        buttons: None,
        on_change: Some(react),
        on_apply: None,
    };
    let v = run(&app.frame, spec, |f| {
        f.set_checked(TEXT_R, true);
        f.set_checked(DEC, true);
        react(f, 0);
    })?;
    if v.checked(TEXT_R) {
        return app.ws.column_edit(Some(v.text(TEXT)), None);
    }
    let int = |id| v.text(id).trim().parse::<i64>().ok();
    let (Some(initial), Some(step), Some(repeat)) = (int(INIT), int(STEP), int(REPEAT)) else {
        return Some("The initial number, step and repeat must be whole numbers".into());
    };
    let format = if v.checked(HEX) {
        NumFormat::Hex
    } else if v.checked(OCT) {
        NumFormat::Oct
    } else if v.checked(BIN) {
        NumFormat::Bin
    } else {
        NumFormat::Dec
    };
    let numbers = ColumnNumbers { initial, step, repeat: crate::platform::wx::to_usize(repeat.max(1)), format, leading_zeros: v.checked(ZEROS) };
    app.ws.column_edit(None, Some(&numbers))
}

/// Search > Find characters in range: stays open so Find can be pressed repeatedly.
#[allow(clippy::too_many_lines, reason = "a declarative dialog spec: one line per control")]
pub(super) fn find_chars_dialog(app: &App) {
    const NON_ASCII: usize = 100;
    const ASCII: usize = 101;
    const MY_RANGE: usize = 102;
    const FROM: usize = 103;
    const TO: usize = 104;
    const UP: usize = 121;
    const DOWN: usize = 122;
    const WRAP: usize = 123;
    const STATUS: usize = 130;
    fn react(f: &crate::widgets::form::Form, _id: usize) {
        let mine = f.checked(MY_RANGE);
        f.enable(FROM, mine);
        f.enable(TO, mine);
    }
    // Decimal, or hex with a 0x prefix.
    fn number(s: &str) -> Option<u32> {
        let s = s.trim();
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16).ok(),
            None => s.parse().ok(),
        }
    }
    let sci = app.ws.sci;
    let apply = std::rc::Rc::new(move |v: &crate::widgets::form::Values, f: &crate::widgets::form::Form| {
        let (lo, hi) = if v.checked(NON_ASCII) {
            (0x80, u32::MAX)
        } else if v.checked(ASCII) {
            (0x00, 0x7F)
        } else {
            match (number(v.text(FROM)), number(v.text(TO))) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => {
                    f.set_text(STATUS, "Enter a valid range, e.g. 0 to 255");
                    return true;
                }
            }
        };
        let found = search::find_char(&sci, lo, hi, v.checked(DOWN), v.checked(WRAP));
        f.set_text(STATUS, if found { "Found" } else { "No character in that range" });
        true
    });
    let spec = Spec {
        title: "Find characters in range",
        size: (360, 270),
        ctls: vec![
            ctl(110, Kind::Group, "Search mode", 12, 8, 336, 118),
            ctl(NON_ASCII, Kind::Radio(true), "&Non-ASCII characters (128 - 255 and above)", 26, 28, 310, 20),
            ctl(ASCII, Kind::Radio(false), "&ASCII characters (0 - 127)", 26, 52, 310, 20),
            ctl(MY_RANGE, Kind::Radio(false), "&My range:", 26, 80, 90, 20),
            ctl(FROM, Kind::Edit, "0", 120, 78, 60, 22),
            ctl(111, Kind::Label, "to", 188, 81, 16, 18),
            ctl(TO, Kind::Edit, "255", 208, 78, 60, 22),
            ctl(112, Kind::Label, "(decimal, or 0x for hex)", 26, 102, 250, 18),
            ctl(120, Kind::Group, "Direction", 12, 132, 336, 48),
            ctl(UP, Kind::Radio(true), "&Up", 26, 150, 80, 20),
            ctl(DOWN, Kind::Radio(false), "&Down", 120, 150, 80, 20),
            ctl(WRAP, Kind::Check, "&Wrap around", 26, 188, 200, 20),
            ctl(STATUS, Kind::Label, "", 12, 214, 336, 18),
        ],
        ok: "Find",
        buttons: None,
        on_change: Some(react),
        on_apply: Some(apply),
    };
    let _ = run(&app.frame, spec, |f| {
        f.set_checked(NON_ASCII, true);
        f.set_checked(DOWN, true);
        f.set_checked(WRAP, true);
        react(f, 0);
    });
}
