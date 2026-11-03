//! The workspace tree (left) and the side list (right): Character Panel, Clipboard History, Document List and
//! Function List.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use notepp_core::commands as c;
use notepp_core::{functions, sci, search};
use wxdragon::prelude::*;

use crate::app::{App, Shared};
use crate::command::CommandHandler;
use crate::side::{Activate, Mode, SIDE_W};
use crate::widgets::controls::{Button, Label, ListView, Panel, TreeView};
use crate::widgets::theme::Role;

pub const TREE_W: i32 = 260;
const HEADER_H: i32 = 26;
const MAX_PER_DIR: usize = 5000;
const HIDDEN: [&str; 1] = [".git"];
/// The placeholder child that makes an unread folder expandable.
const DUMMY: &str = "\u{2026}";

#[derive(Clone, Copy)]
pub struct Panels {
    pub tree_box: Panel,
    pub tree: TreeView,
    title: Label,
    pub side: ListView,
}

thread_local! {
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Directories first, then files, each alphabetical (case-insensitive).
pub(crate) fn list_dir(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, bool, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            (!(is_dir && HIDDEN.contains(&name.as_str()))).then(|| (e.path(), is_dir, name.to_lowercase()))
        })
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    v.into_iter().take(MAX_PER_DIR).map(|(p, d, _)| (p, d)).collect()
}

/// Build the (initially hidden) panels inside `parent`; the caller places them in its layout.
pub fn create(parent: &wxdragon::prelude::Panel) -> Panels {
    // These look like the editor, so they take its colours from the theme.
    let tree_box = Panel::adopt_as(wxdragon::prelude::Panel::builder(parent).build(), Role::Editor);
    let title = Label::adopt_as(StaticText::builder(&tree_box).with_label("").build(), Role::Editor);
    let close = Button::adopt_as(wxdragon::prelude::Button::builder(&tree_box).with_label("\u{2715}").build(), Role::Editor);
    let tree = TreeView::adopt_as(TreeCtrl::builder(&tree_box).with_style(TreeCtrlStyle::HasButtons | TreeCtrlStyle::LinesAtRoot).build(), Role::Editor);
    let head = BoxSizer::builder(Orientation::Horizontal).build();
    head.add(&title, 1, SizerFlag::AlignCenterVertical | SizerFlag::Left, 8);
    head.add(&close, 0, SizerFlag::All, 2);
    let col = BoxSizer::builder(Orientation::Vertical).build();
    col.add_sizer(&head, 0, SizerFlag::Expand, 0);
    col.add(&tree, 1, SizerFlag::Expand, 0);
    tree_box.set_sizer(col, true);
    tree_box.set_min_size(Size::new(TREE_W, -1));
    tree_box.show(false);
    close.set_min_size(Size::new(24, HEADER_H - 4));

    let side = ListView::adopt_as(ListCtrl::builder(parent).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build(), Role::Editor);
    side.set_min_size(Size::new(SIDE_W, -1));
    side.show(false);
    Panels { tree_box, tree, title, side }
}

pub fn wire(app: &Shared) {
    let p = app.borrow().panels;
    // Expanding a folder reads it; a file is opened by double-click or Enter.
    let tree = p.tree;
    p.tree.on_item_expanding(move |e| {
        if let Some(item) = e.get_item() {
            crate::tree_reader::expand(&tree, &item);
        }
    });
    let a = app.clone();
    p.tree.on_item_activated(move |e| {
        let Some(item) = e.get_item() else { return };
        let Some(path) = path_of(&tree, &item) else { return };
        if path.is_file()
            && let Ok(mut app) = a.try_borrow_mut()
        {
            let App { ws, ui, .. } = &mut *app;
            ws.open_path(ui, &path);
            app.refresh();
            app.stc.set_focus();
        }
    });
    let a = app.clone();
    let side = p.side;
    p.side.on_item_activated(move |e| {
        let n = e.get_item_index();
        if n < 0 {
            return;
        }
        if let Ok(mut app) = a.try_borrow_mut() {
            app.side_activate(crate::platform::wx::to_usize(n));
        }
    });
    let a = app.clone();
    p.side.on_key_down(move |e| {
        // Delete removes the selected row where the content allows it (the clipboard history).
        if e.get_key_code().unwrap_or(0) != 127 {
            return;
        }
        let Some(n) = side.selected() else { return };
        if let Ok(mut app) = a.try_borrow_mut()
            && app.side.content_mut().is_some_and(|c| c.remove(n))
        {
            app.fill_side();
        }
    });
}

/// The path an item stands for: the root's path plus the item names above it.
pub(crate) fn path_of(tree: &TreeCtrl, item: &TreeItemId) -> Option<PathBuf> {
    let root = ROOT.with(|r| r.borrow().clone())?;
    let mut names = Vec::new();
    let mut cur = item.clone();
    // The root has no parent; every other item contributes its name.
    while let Some(parent) = tree.get_item_parent(&cur).filter(wxdragon::TreeItemId::is_ok) {
        names.push(tree.get_item_text(&cur)?);
        cur = parent;
    }
    let mut p = root;
    for n in names.iter().rev() {
        p.push(n);
    }
    Some(p)
}

/// Read a folder's items now (Reveal in File Tree needs them at once; expanding goes through `tree_reader`).
fn load_children(tree: &TreeCtrl, item: &TreeItemId) {
    if let Some(path) = path_of(tree, item).filter(|_| needs_reading(tree, item)) {
        fill(tree, item, list_dir(&path));
    }
}

/// Only a folder whose single child is the placeholder still needs reading.
pub(crate) fn needs_reading(tree: &TreeCtrl, item: &TreeItemId) -> bool {
    tree.get_first_child(item).is_some_and(|(first, _)| tree.get_item_text(&first).as_deref() == Some(DUMMY))
}

/// Replace a folder's placeholder with what it holds (`entries` from [`list_dir`]).
pub(crate) fn fill(tree: &TreeCtrl, item: &TreeItemId, entries: Vec<(PathBuf, bool)>) {
    if !needs_reading(tree, item) {
        return;
    }
    tree.delete_children(item);
    for (p, is_dir) in entries {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(child) = tree.append_item(item, &name, None, None)
            && is_dir
        {
            tree.append_item(&child, DUMMY, None, None);
        }
    }
}

/// Is there a placeholder-free file at `rel` under the tree? Expand down to it and select it.
fn reveal(tree: &TreeCtrl, root: &Path, file: &Path) -> bool {
    let Ok(rel) = file.strip_prefix(root) else { return false };
    let Some(mut item) = tree.get_root_item() else { return false };
    for comp in rel.components() {
        load_children(tree, &item);
        tree.expand(&item);
        let want = comp.as_os_str().to_string_lossy().to_lowercase();
        let Some((mut child, mut cookie)) = tree.get_first_child(&item) else { return false };
        loop {
            if tree.get_item_text(&child).is_some_and(|t| t.to_lowercase() == want) {
                break;
            }
            match tree.get_next_child(&item, &mut cookie) {
                Some(n) => child = n,
                None => return false,
            }
        }
        item = child;
    }
    tree.select_item(&item);
    tree.ensure_visible(&item);
    true
}

impl App {
    pub fn tree_visible(&self) -> bool {
        self.panels.tree_box.is_shown()
    }

    /// Show `dir` in the workspace tree and make it the root for Quick Open / Find in Files.
    pub fn open_workspace(&mut self, dir: PathBuf) {
        let t = self.panels.tree;
        ROOT.with(|r| *r.borrow_mut() = Some(dir.clone()));
        let name = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
        self.panels.title.set_label(&name);
        crate::tree_reader::forget();
        t.delete_all_items();
        if let Some(root) = t.add_root(&name, None, None) {
            t.append_item(&root, DUMMY, None, None);
            load_children(&t, &root);
            t.expand(&root);
        }
        // Quick Open lists this folder: start now, in the background, so it is ready when asked for.
        self.file_index.start(&dir);
        self.ws.workspace = Some(dir);
        self.set_tree(true);
    }

    pub fn set_tree(&mut self, on: bool) {
        if self.panels.tree_box.is_shown() != on {
            self.panels.tree_box.show(on);
            self.editor_pane.layout();
        }
    }

    pub fn reveal_in_tree(&mut self) {
        let Some(p) = self.ws.tabs[self.ws.active].path.clone() else {
            self.flash = Some("Save the file first".into());
            return;
        };
        let inside = self.ws.workspace.as_ref().is_some_and(|r| p.starts_with(r));
        if !inside && let Some(d) = p.parent() {
            self.open_workspace(d.to_path_buf());
        }
        self.set_tree(true);
        if let Some(root) = self.ws.workspace.clone()
            && !reveal(&self.panels.tree, &root, &p)
        {
            self.flash = Some("The file was not found in the tree".into());
        }
    }

    /// Switch the side list to `want`, or hide it when it already shows that.
    pub fn toggle_side(&mut self, want: Mode) {
        if self.side.toggle(want) {
            if want == Mode::Clipboard {
                self.poll_clipboard();
            }
            self.side_key = None;
            self.panels.side.show(true);
            self.fill_side();
        } else {
            self.panels.side.show(false);
        }
        self.editor_pane.layout();
    }

    /// Clear a copied password after 30 seconds, and lock the vault when idle.
    pub fn clip_tick(&mut self) {
        self.clipboard.expire_secret();
        if notepp_core::bitwarden::tick_auto_lock() {
            self.flash = Some("Passwords locked (idle)".into());
        }
    }

    /// Remember a copied text (newest first, no repeats).
    pub fn poll_clipboard(&mut self) {
        let Some(text) = crate::services::clipboard::paste() else { return };
        // A copied password stays out of the history.
        if self.clipboard.secret.is_secret(&text) {
            return;
        }
        if self.side.history.record(&text) && self.side.mode == Some(Mode::Clipboard) {
            self.fill_side();
        }
    }
    /// Show the current content in the side list.
    pub fn fill_side(&self) {
        let l = self.panels.side;
        match self.side.content() {
            Some(c) => {
                l.set_columns(&c.columns());
                l.fill(&c.rows());
            }
            None => l.fill(&[]),
        }
    }

    /// Replace the rows of the Documents / Functions views; the list is refilled only if they changed.
    fn set_side_rows(&mut self, rows: Vec<(Vec<String>, usize)>) {
        let changed = match self.side.mode {
            Some(Mode::Documents) => self.side.documents.set_rows(rows),
            Some(Mode::Functions) => self.side.functions.set_rows(rows),
            _ => false,
        };
        if changed {
            self.fill_side();
        }
    }

    /// Keep the Documents / Functions lists in step with the workspace.
    pub fn refresh_side(&mut self) {
        match self.side.mode {
            Some(Mode::Documents) => {
                let rows = self
                    .ws
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let name = format!("{}{}", t.title, if self.ws.is_dirty(i) { " *" } else { "" });
                        (vec![name, t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()], i)
                    })
                    .collect();
                self.set_side_rows(rows);
            }
            Some(Mode::Functions) => {
                if self.ws.is_whiteboard() {
                    self.set_side_rows(Vec::new());
                    self.side_key = None;
                    return;
                }
                let key = (self.ws.tabs[self.ws.active].doc, sci::length(&self.ws.sci));
                if self.side_key == Some(key) {
                    return;
                }
                self.side_key = Some(key);
                let text = sci::get_text(&self.ws.sci);
                let rows = functions::extract(&text, self.ws.tabs[self.ws.active].lang)
                    .into_iter()
                    .map(|e| (vec![e.name, (e.line + 1).to_string()], e.line))
                    .collect();
                self.set_side_rows(rows);
            }
            _ => {}
        }
    }

    /// What a double-click in the side list does.
    fn side_activate(&mut self, n: usize) {
        match self.side.content().and_then(|c| c.activate(n)) {
            Some(Activate::Insert(text)) => self.ws.insert_text(&text),
            Some(Activate::Document(i)) => self.ws.activate(i),
            Some(Activate::Goto(line)) => {
                search::goto_line(&self.ws.sci, line + 1);
            }
            None => {}
        }
        self.refresh();
        self.stc.set_focus();
    }
}

/// Side panels: workspace tree, document and function lists, clipboard history, character panel.
pub struct PanelCommands;

impl CommandHandler<App> for PanelCommands {
    fn name(&self) -> &'static str {
        "panels"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_CHAR_PANEL => app.toggle_side(Mode::Characters),
            c::ID_CLIP_HISTORY => app.toggle_side(Mode::Clipboard),
            c::ID_DOC_LIST => app.toggle_side(Mode::Documents),
            c::ID_FUNC_LIST => app.toggle_side(Mode::Functions),
            c::ID_WORKSPACE_TOGGLE => {
                if app.ws.workspace.is_some() {
                    let on = !app.tree_visible();
                    app.set_tree(on);
                } else {
                    app.flash = Some("Open a folder as workspace first".into());
                }
            }
            c::ID_OPEN_WORKSPACE => {
                if let Some(p) = crate::widgets::ask::folder(&app.frame, "Open Folder as Workspace") {
                    app.open_workspace(p);
                }
            }
            c::ID_OPEN_FOLDER_WS => match app.ws.tabs[app.ws.active].path.clone() {
                Some(p) => {
                    if let Some(d) = p.parent() {
                        app.open_workspace(d.to_path_buf());
                    }
                }
                None => app.flash = Some("Save the file first".into()),
            },
            c::ID_TAB_REVEAL => app.reveal_in_tree(),
            _ => return false,
        }
        true
    }
}
