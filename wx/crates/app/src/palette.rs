//! Command Palette and Quick Open: a borderless type-to-filter list over the editor.

use std::path::PathBuf;

use notepp_core::commands::{ID_CHARSET_BASE, ID_CONVERT_BASE, ID_ENCODE_BASE, ID_LANG_BASE, PALETTE_COMMANDS};
use notepp_core::doc::CHARSETS;
use notepp_core::files;
use notepp_core::lang::LANGUAGES;

use crate::app::Shared;
use notepp_core::tables::ENCODE_IN;
use wxdragon::prelude::WxWidget;
use crate::widgets::quick_pick::{Item, pick};

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
        let push = |p: &PathBuf, list: &mut Vec<PathBuf>| {
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
            && let Ok(mut a) = app.try_borrow_mut() {
                let crate::app::App { ws, ui, .. } = &mut *a;
                ws.open_path(ui, &p);
                a.refresh();
                a.stc.set_focus();
            }
}
