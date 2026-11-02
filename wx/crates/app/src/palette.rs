//! Command Palette and Quick Open: a borderless type-to-filter list over the editor.

use std::path::PathBuf;

use notepp_core::files;

use crate::app::Shared;
use crate::widgets::quick_pick::{Item, pick};
use wxdragon::prelude::WxWidget;

/// The effective shortcut text of a command (the user's own, else the default).
fn shortcut_text(app: &crate::app::App, id: usize, default: &str) -> String {
    app.keymap.shown(id).map_or_else(|| default.to_string(), str::to_string)
}

pub fn open_commands(app: &Shared) {
    let (frame, items) = {
        let Ok(a) = app.try_borrow() else { return };
        let mut v: Vec<Item> =
            notepp_core::registry::palette().map(|c| Item { label: c.palette.to_string(), hint: shortcut_text(&a, c.id, c.hint()), id: c.id }).collect();
        v.extend(
            notepp_core::menu_provider::palette_items().into_iter().filter_map(|i| Some(Item { label: i.palette?, hint: String::new(), id: i.command.id() })),
        );
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
        // A set for the duplicate check: thousands of scanned files against a list was quadratic, on the UI thread.
        let mut seen = std::collections::HashSet::new();
        let mut push = |p: &PathBuf, list: &mut Vec<PathBuf>| {
            if seen.insert(p.clone()) {
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
        let start = ws.tabs[ws.active].path.as_ref().and_then(|p| p.parent().map(std::path::Path::to_path_buf));
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
    if let Some(i) = pick(&frame, items)
        && let Some(p) = files_list.get(i).cloned()
        && let Ok(mut a) = app.try_borrow_mut()
    {
        let crate::app::App { ws, ui, .. } = &mut *a;
        ws.open_path(ui, &p);
        a.refresh();
        a.stc.set_focus();
    }
}
