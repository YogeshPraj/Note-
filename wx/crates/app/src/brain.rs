//! Brain: the capture box, the search list, and the commands that open the Brain's file and its folder
//! (docs/brain-plan.md). Everything is local files; the work is in `notepp_core::brain` and the workspace.

use std::cell::RefCell;
use std::rc::Rc;

use notepp_core::brain::{self, Hit, Note, Source};
use notepp_core::commands::{ID_BRAIN_CAPTURE, ID_BRAIN_EXPORT, ID_BRAIN_FOLDER, ID_BRAIN_SEARCH, ID_BRAIN_SELECTION, ID_BRAIN_TODAY};
use notepp_core::search;
use wxdragon::prelude::WxWidget;

use crate::app::{App, Shared};
use crate::widgets::form::{Form, Kind, Spec, ctl, on_edit, run};
use crate::widgets::toast::Tone;

/// How many results the list shows.
const SHOWN: usize = 60;

/// Run a Brain command that needs a window of its own; false for any other command. Nothing is borrowed while a
/// dialog is up, so the rest of the window goes on working.
pub fn run_command(app: &Shared, id: usize) -> bool {
    match id {
        ID_BRAIN_CAPTURE => capture(app, false),
        ID_BRAIN_SELECTION => capture(app, true),
        ID_BRAIN_SEARCH => search_notes(app),
        ID_BRAIN_TODAY => open_today(app),
        ID_BRAIN_FOLDER => open_folder(app),
        ID_BRAIN_EXPORT => export(app),
        _ => return false,
    }
    // The panel shows what these commands may have changed.
    if let Ok(mut a) = app.try_borrow_mut() {
        a.fill_brain();
    }
    true
}

impl App {
    /// Read the Brain again for its panel (when it is showing): after a capture, a change of encryption.
    pub fn fill_brain(&mut self) {
        if self.side.mode != Some(crate::side::Mode::Brain) {
            return;
        }
        if self.ws.brain_locked() {
            self.side.brain.locked();
        } else {
            let store = self.ws.brain_store();
            self.side.brain.reload(&store, self.ws.vault.dek.as_ref());
        }
        self.fill_side();
    }
}

/// The tag of row `n` of the Brain panel, when it is showing and the row is a tag.
pub fn tag_at(app: &Shared, n: usize) -> Option<String> {
    let a = app.try_borrow().ok()?;
    match a.side.content().and_then(|c| c.activate(n)) {
        Some(crate::side::Activate::Tag(t)) if a.side.mode == Some(crate::side::Mode::Brain) => Some(t),
        _ => None,
    }
}

/// Unlock the Brain when it needs it, with nothing else borrowed. False when it stays locked (the user said so).
fn ready(app: &Shared) -> bool {
    let Ok(mut a) = app.try_borrow_mut() else { return false };
    let App { ws, ui, .. } = &mut *a;
    ws.brain_ready(ui)
}

fn told(app: &Shared, text: &str, tone: Tone) {
    if let Ok(a) = app.try_borrow() {
        a.ui.notice(text, tone);
    }
}

fn capture(app: &Shared, selection: bool) {
    if !ready(app) {
        return told(app, "Quick Notes is locked.", Tone::Error);
    }
    let (frame, picked) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.brain_selection())
    };
    if selection && picked.is_none() {
        return told(app, "Select some text first.", Tone::Error);
    }
    let (text, source): (String, Option<Source>) = match picked {
        Some((t, s)) => (t, Some(s)),
        None => (String::new(), None),
    };
    let spec = Spec {
        title: if selection { "Send Selection to Quick Notes" } else { "Capture to Quick Notes" },
        size: (480, 260),
        ctls: vec![
            ctl(1, Kind::Label, "What should you remember? Words with # in front of them (#deploy) are tags.", 14, 12, 452, 20),
            ctl(2, Kind::MultiEdit(false), "", 14, 36, 452, 170),
        ],
        ok: "&Add",
        buttons: None,
        on_change: None,
        on_apply: None,
    };
    let Some(v) = run(&frame, spec, |f| f.set_text(2, &text)) else { return };
    let entry = v.text(2).trim().to_string();
    if entry.is_empty() {
        return;
    }
    let when = crate::platform::current().local_time();
    let added = {
        let Ok(mut a) = app.try_borrow_mut() else { return };
        let App { ws, ui, .. } = &mut *a;
        let r = ws.brain_add(ui, &when, &entry, source.as_ref());
        a.refresh();
        r
    };
    match added {
        Ok(_) => told(app, "Added to Quick Notes", Tone::Info),
        Err(e) => told(app, &format!("Could not add it to Quick Notes.\n\n{e}"), Tone::Error),
    }
}

fn open_today(app: &Shared) {
    if !ready(app) {
        return told(app, "Quick Notes is locked.", Tone::Error);
    }
    let when = crate::platform::current().local_time();
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let App { ws, ui, .. } = &mut *a;
    let r = ws.brain_open_day(ui, &when);
    finish_open(&mut a, r);
}

/// A note was opened (or could not be): show it, at `line` when there is one.
fn finish_open(a: &mut App, opened: Result<(std::path::PathBuf, usize), notepp_core::error::Error>) {
    match opened {
        Ok((_, line)) => {
            if line > 0 {
                search::goto_line(&a.ws.sci, line);
            }
            a.refresh();
            a.stc().set_focus();
        }
        Err(e) => a.ui.notice(&format!("Could not open the note.\n\n{e}"), Tone::Error),
    }
}

fn open_folder(app: &Shared) {
    let Ok(a) = app.try_borrow() else { return };
    let root = a.ws.brain_store().root;
    let _ = std::fs::create_dir_all(&root);
    crate::platform::current().reveal_in_file_manager(&root);
}

/// Save the whole Brain as a zip. Encrypted notes stay encrypted in it, so nothing needs unlocking.
fn export(app: &Shared) {
    let (frame, store) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.brain_store())
    };
    let name = format!("quick-notes-{}.zip", brain::Date::of(&crate::platform::current().local_time()));
    let Some(dest) = crate::widgets::ask::save_file(&frame, "Export Quick Notes", &name, "Zip file|*.zip") else { return };
    match store.export(&dest) {
        Ok(n) => told(app, &format!("Exported {n} file(s) to {}", dest.display()), Tone::Info),
        Err(e) => told(app, &format!("Could not export Quick Notes.\n\n{e}"), Tone::Error),
    }
}
const QUERY: usize = 2;
const LIST: usize = 3;

/// What the search window keeps while it is open.
struct Found {
    notes: Vec<Note>,
    hits: Vec<Hit>,
}

fn refill(f: &Form) {
    let today = brain::Date::of(&crate::platform::current().local_time());
    let query = brain::parse_query(&f.text(QUERY), today);
    let rows = f
        .with_state(|s: &mut Found| {
            s.hits = brain::search(&s.notes, &query, SHOWN);
            s.hits.iter().map(|h| vec![h.title.clone(), h.snippet.clone()]).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    f.list_fill(LIST, &[("When", 150), ("Note", 430)], &rows);
    if !rows.is_empty() {
        f.list_select(LIST, 0);
    }
}

fn on_typed(f: &Form, id: usize) {
    if id == QUERY {
        refill(f);
    }
}

fn search_notes(app: &Shared) {
    search_for(app, "");
}

/// The search window, with irst already typed in it.
pub fn search_for(app: &Shared, first: &str) {
    if !ready(app) {
        return told(app, "Quick Notes is locked.", Tone::Error);
    }
    let (frame, notes) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.brain_notes())
    };
    let notes = match notes {
        Ok(n) => n,
        Err(e) => return told(app, &format!("Could not read Quick Notes.\n\n{e}"), Tone::Error),
    };
    let chosen: Rc<RefCell<Option<Hit>>> = Rc::new(RefCell::new(None));
    let keep = chosen.clone();
    let spec = Spec {
        title: "Search Quick Notes",
        size: (620, 420),
        ctls: vec![
            ctl(1, Kind::Label, "Words, #tags, and today, yesterday, week, month or date:2026-11:", 14, 12, 592, 20),
            ctl(QUERY, Kind::Edit, "", 14, 36, 592, 26),
            ctl(LIST, Kind::List, "", 14, 72, 592, 300),
        ],
        ok: "&Open",
        buttons: None,
        on_change: None,
        on_apply: Some(Rc::new(move |_, f| {
            let hit = f.list_selected(LIST).and_then(|i| f.with_state(|s: &mut Found| s.hits.get(i).cloned()).flatten());
            let none = hit.is_none();
            *keep.borrow_mut() = hit;
            none
        })),
    };
    on_edit(on_typed);
    let _ = run(&frame, spec, |f| {
        f.set_state(Found { notes, hits: Vec::new() });
        // Up and Down in the box move through the results without leaving it.
        f.on_key(QUERY, |f, key, _, _, _| {
            let (up, down) = (key == 315, key == 317);
            if !(up || down) {
                return false;
            }
            let at = f.list_selected(LIST).unwrap_or(0);
            f.list_select(LIST, if up { at.saturating_sub(1) } else { at + 1 });
            true
        });
        f.set_text(QUERY, first);
        refill(f);
    });
    let Some(hit) = chosen.borrow_mut().take() else { return };
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let App { ws, ui, .. } = &mut *a;
    ws.open_path(ui, &hit.path);
    finish_open(&mut a, Ok((hit.path, hit.line)));
}
