//! Command Palette and Quick Open: a borderless type-to-filter list over the editor.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use notepp_core::commands::{ID_CHARSET_BASE, ID_CONVERT_BASE, ID_ENCODE_BASE, ID_LANG_BASE, PALETTE_COMMANDS};
use notepp_core::doc::CHARSETS;
use notepp_core::files;
use notepp_core::lang::LANGUAGES;
use wxdragon::event::WindowEventData;
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::tables::ENCODE_IN;

const WIDTH: i32 = 640;
const ROW_H: i32 = 26;
const MAX_ROWS: i32 = 11;
const EDIT_H: i32 = 30;

#[derive(Clone)]
pub struct Item {
    pub label: String,
    /// Right-hand detail: a shortcut, or the folder of a file.
    pub hint: String,
    /// Command id, or index into the file list.
    pub id: usize,
}

/// Every word of `query` must occur in `text`; earlier hits score better.
pub fn matches(query: &str, text: &str) -> Option<usize> {
    let t = text.to_lowercase();
    let mut score = 0;
    for word in query.to_lowercase().split_whitespace() {
        score += t.find(word)?;
    }
    Some(score)
}

/// Show the list over `frame`; the id of the chosen item, if any.
fn pick(frame: &Frame, items: Vec<Item>) -> Option<usize> {
    let d = Dialog::builder(frame, "").with_style(DialogStyle::StayOnTop).build();
    let list_h = ROW_H * MAX_ROWS;
    d.set_client_size(Size::new(WIDTH, EDIT_H + list_h));
    let edit = TextCtrl::builder(&d).with_pos(Point::new(0, 0)).with_size(Size::new(WIDTH, EDIT_H)).with_style(TextCtrlStyle::ProcessEnter).build();
    let list = ListBox::builder(&d).with_pos(Point::new(0, EDIT_H)).with_size(Size::new(WIDTH, list_h)).build();
    let fp = frame.get_position();
    let fs = frame.get_size();
    d.move_window(fp.x + (fs.width - WIDTH) / 2, fp.y + 60);

    let items = Rc::new(items);
    let shown: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let refilter = {
        let (items, shown) = (items.clone(), shown.clone());
        move || {
            let q = edit.get_value();
            let mut scored: Vec<(usize, usize)> = items.iter().enumerate().filter_map(|(i, it)| matches(&q, &format!("{} {}", it.label, it.hint)).map(|s| (s, i))).collect();
            scored.sort_by_key(|(s, i)| (*s, *i));
            let rows: Vec<usize> = scored.into_iter().take(300).map(|(_, i)| i).collect();
            list.clear();
            for i in &rows {
                let it = &items[*i];
                list.append(&if it.hint.is_empty() { it.label.clone() } else { format!("{}   \u{2014} {}", it.label, it.hint) });
            }
            if !rows.is_empty() {
                list.set_selection(0, true);
            }
            *shown.borrow_mut() = rows;
        }
    };
    refilter();
    {
        let r = refilter.clone();
        edit.on_text_updated(move |_| r());
    }
    let chosen: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    let accept = {
        let (shown, chosen, items) = (shown.clone(), chosen.clone(), items.clone());
        move || {
            if let Some(sel) = list.get_selection() {
                if let Some(i) = shown.borrow().get(sel as usize) {
                    *chosen.borrow_mut() = Some(items[*i].id);
                    d.end_modal(ID_OK);
                }
            }
        }
    };
    {
        let a = accept.clone();
        edit.on_enter_pressed(move |_| a());
    }
    {
        let a = accept.clone();
        list.on_item_double_clicked(move |_| a());
    }
    edit.on_key_down(move |e| {
        let key = match &e {
            WindowEventData::Keyboard(k) => k.get_key_code().unwrap_or(0),
            _ => 0,
        };
        let count = list.get_count() as i32;
        let sel = list.get_selection().map_or(-1, |s| s as i32);
        let go = |to: i32| {
            if count > 0 {
                list.set_selection(to.clamp(0, count - 1) as u32, true);
            }
        };
        match key {
            315 => go(sel - 1),          // up
            317 => go(sel + 1),          // down
            366 => go(sel - MAX_ROWS),   // page up
            367 => go(sel + MAX_ROWS),   // page down
            27 => d.end_modal(ID_CANCEL), // escape
            _ => e.skip(true),
        }
    });
    edit.set_focus();
    d.show_modal();
    let r = *chosen.borrow();
    r
}

/// The effective shortcut text of a command (the user's own, else the default).
fn shortcut_text(app: &crate::app::App, id: usize, default: &str) -> String {
    app.ws.settings.shortcuts.get(&id.to_string()).cloned().unwrap_or_else(|| default.to_string())
}

pub fn open_commands(app: &Shared) {
    let (frame, items) = {
        let Ok(a) = app.try_borrow() else { return };
        let mut v: Vec<Item> = PALETTE_COMMANDS.iter().map(|(l, id, k)| Item { label: l.to_string(), hint: shortcut_text(&a, *id, k), id: *id }).collect();
        for (i, e) in ENCODE_IN.iter().enumerate() {
            v.push(Item { label: format!("Encoding: Encode in {}", e.label()), hint: String::new(), id: ID_ENCODE_BASE + i });
            v.push(Item { label: format!("Encoding: Convert to {}", e.label()), hint: String::new(), id: ID_CONVERT_BASE + i });
        }
        for (i, c) in CHARSETS.iter().enumerate() {
            v.push(Item { label: format!("Encoding: Character set {} ({})", c.label, c.group), hint: String::new(), id: ID_CHARSET_BASE + i });
        }
        for (i, (label, _)) in LANGUAGES.iter().enumerate() {
            v.push(Item { label: format!("Language: {label}"), hint: String::new(), id: ID_LANG_BASE + i });
        }
        (a.frame, v)
    };
    if let Some(id) = pick(&frame, items) {
        crate::app::run_command(app, id);
    }
}

pub fn open_files(app: &Shared) {
    let (frame, files_list, items) = {
        let Ok(a) = app.try_borrow() else { return };
        let ws = &a.ws;
        // Open tabs, then recent files, then everything under the project root.
        let mut list: Vec<PathBuf> = Vec::new();
        let mut push = |p: &PathBuf, list: &mut Vec<PathBuf>| {
            if !list.contains(p) {
                list.push(p.clone());
            }
        };
        for t in &ws.tabs {
            if let Some(p) = &t.path {
                push(p, &mut list);
            }
        }
        for p in &ws.recent {
            push(p, &mut list);
        }
        let start = ws.tabs[ws.active].path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let root = ws.workspace.clone().or_else(|| start.as_ref().map(|d| files::project_root(d)));
        if let Some(root) = &root {
            for p in files::scan_files(root, 8000) {
                push(&p, &mut list);
            }
        }
        let items: Vec<Item> = list
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let dir = p.parent().map(|d| root.as_ref().and_then(|r| d.strip_prefix(r).ok()).unwrap_or(d));
                Item {
                    label: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    hint: dir.map(|d| d.display().to_string()).unwrap_or_default(),
                    id: i,
                }
            })
            .collect();
        (a.frame, list, items)
    };
    if let Some(i) = pick(&frame, items) {
        if let Some(p) = files_list.get(i).cloned() {
            if let Ok(mut a) = app.try_borrow_mut() {
                let crate::app::App { ws, ui, .. } = &mut *a;
                ws.open_path(ui, &p);
                a.refresh();
                a.stc.set_focus();
            }
        }
    }
}
