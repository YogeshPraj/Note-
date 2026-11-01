//! Find Next / Previous and Go To Line (the docked Find panel itself is in `findpanel`).

use notepp_core::search::{self, Found};

use crate::app::Shared;
use crate::findpanel::{self, Mode};

/// Open the docked panel in `mode`, prefilled with the selection.
pub fn open_panel(app: &Shared, mode: Mode) {
    let (sel, last, dir) = match app.try_borrow() {
        Ok(a) => (search::selected_text(&a.ws.sci), a.ws.search.find.clone(), a.results.root.display().to_string()),
        Err(_) => return,
    };
    findpanel::open(mode, &sel, &last, &dir);
}
/// Find Next / Previous with the remembered options; opens the dialog when nothing was searched yet.
pub fn step(app: &Shared, forward: bool) {
    let (empty, initial) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.ws.search.find.is_empty(), search::selected_text(&a.ws.sci))
    };
    if empty {
        let _ = initial;
        return open_panel(app, Mode::Find);
    }
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let o = a.ws.search.clone();
    let sci = a.ws.sci;
    a.flash = Some(match search::find(&sci, &o, forward) {
        Found::At { wrapped } => {
            if wrapped {
                "Found (wrapped around)".to_string()
            } else {
                "Found".to_string()
            }
        }
        Found::NotFound => "Not found".to_string(),
        Found::BadPattern(e) => format!("Bad pattern: {e}"),
    });
    a.refresh();
}

/// Go To Line: ask for a number and jump there.
pub fn goto_line(app: &Shared) {
    let (frame, lines) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, crate::platform::wx::to_usize(notepp_core::sci::line_count(&a.ws.sci).max(1)))
    };
    use crate::widgets::form::{Kind, Spec, ctl, run};
    let spec = Spec {
        title: "Go To Line",
        size: (300, 118),
        ctls: vec![ctl(1, Kind::Label, &format!("Line number (1 - {lines}):"), 14, 14, 272, 20), ctl(2, Kind::Edit, "", 14, 38, 272, 26)],
        ok: "&Go",
        buttons: Some(((118, 78), (206, 78))),
        on_change: None,
        on_apply: None,
    };
    if let Some(v) = run(&frame, spec, |_| {})
        && let Ok(n) = v.text(2).trim().parse::<usize>()
        && let Ok(a) = app.try_borrow()
    {
        search::goto_line(&a.ws.sci, n.clamp(1, lines.max(1)));
    }
}
