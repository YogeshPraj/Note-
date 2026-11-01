//! The workspace tree (left) and the side list (right): Character Panel, Clipboard History, Document List and
//! Function List.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use notepp_core::commands as c;
use notepp_core::{functions, sci, search};
use wxdragon::prelude::*;

use crate::app::{App, Shared};

pub const TREE_W: i32 = 260;
pub const SIDE_W: i32 = 250;
const HEADER_H: i32 = 26;
const MAX_HISTORY: usize = 50;
const MAX_PER_DIR: usize = 5000;
const HIDDEN: [&str; 1] = [".git"];
/// The placeholder child that makes an unread folder expandable.
const DUMMY: &str = "\u{2026}";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Characters,
    Clipboard,
    Documents,
    Functions,
}

#[derive(Clone, Copy)]
pub struct Panels {
    pub tree_box: Panel,
    pub tree: TreeCtrl,
    title: StaticText,
    pub side: ListCtrl,
}

thread_local! {
    static MODE: Cell<Option<Mode>> = const { Cell::new(None) };
    static HISTORY: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Rows of the Documents / Functions views: cell texts and the target they open.
    static ROWS: RefCell<Vec<(Vec<String>, usize)>> = const { RefCell::new(Vec::new()) };
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn mode() -> Option<Mode> {
    MODE.with(|m| m.get())
}

const CONTROL_NAMES: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS",
    "US",
];

/// The character a table row stands for (Windows-1252 above 127).
fn char_for(n: u8) -> char {
    if n < 128 {
        n as char
    } else {
        encoding_rs::WINDOWS_1252.decode(&[n]).0.chars().next().unwrap_or('?')
    }
}

fn label_for(n: u8) -> String {
    match n {
        0..=31 => CONTROL_NAMES[n as usize].to_string(),
        127 => "DEL".into(),
        128..=159 if char_for(n) == '\u{FFFD}' || char_for(n).is_control() => String::new(),
        _ => char_for(n).to_string(),
    }
}

/// One-line, length-limited form of a clipboard entry.
fn preview(s: &str) -> String {
    let flat: String = s.chars().map(|c| if c == '\r' || c == '\n' { '\u{21B5}' } else if c == '\t' { ' ' } else { c }).collect();
    let mut out: String = flat.chars().take(120).collect();
    if flat.chars().count() > 120 {
        out.push('\u{2026}');
    }
    out
}

/// Directories first, then files, each alphabetical (case-insensitive).
fn list_dir(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, bool, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (!(is_dir && HIDDEN.contains(&name.as_str()))).then(|| (e.path(), is_dir, name.to_lowercase()))
        })
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    v.into_iter().take(MAX_PER_DIR).map(|(p, d, _)| (p, d)).collect()
}

/// Build the (initially hidden) panels inside `parent`; the caller places them in its layout.
pub fn create(parent: &Panel) -> Panels {
    let tree_box = Panel::builder(parent).build();
    let title = StaticText::builder(&tree_box).with_label("").build();
    let close = Button::builder(&tree_box).with_label("\u{2715}").build();
    let tree = TreeCtrl::builder(&tree_box).with_style(TreeCtrlStyle::HasButtons | TreeCtrlStyle::LinesAtRoot).build();
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

    let side = ListCtrl::builder(parent).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build();
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
            load_children(&tree, &item);
        }
    });
    let a = app.clone();
    p.tree.on_item_activated(move |e| {
        let Some(item) = e.get_item() else { return };
        let Some(path) = path_of(&tree, &item) else { return };
        if path.is_file() {
            if let Ok(mut app) = a.try_borrow_mut() {
                let App { ws, ui, .. } = &mut *app;
                ws.open_path(ui, &path);
                app.refresh();
                app.stc.set_focus();
            }
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
            app.side_activate(n as usize);
        }
    });
    let a = app.clone();
    p.side.on_key_down(move |e| {
        let key = e.get_key_code().unwrap_or(0);
        if key == 127 && mode() == Some(Mode::Clipboard) {
            let n = side.get_first_selected_item();
            if n >= 0 {
                HISTORY.with(|h| {
                    let mut h = h.borrow_mut();
                    if (n as usize) < h.len() {
                        h.remove(n as usize);
                    }
                });
                if let Ok(app) = a.try_borrow() {
                    app.fill_side();
                }
                return;
            }
        }
    });
}

/// The path an item stands for: the root's path plus the item names above it.
fn path_of(tree: &TreeCtrl, item: &TreeItemId) -> Option<PathBuf> {
    let root = ROOT.with(|r| r.borrow().clone())?;
    let mut names = Vec::new();
    let mut cur = item.clone();
    // The root has no parent; every other item contributes its name.
    while let Some(parent) = tree.get_item_parent(&cur).filter(|p| p.is_ok()) {
        names.push(tree.get_item_text(&cur)?);
        cur = parent;
    }
    let mut p = root;
    for n in names.iter().rev() {
        p.push(n);
    }
    Some(p)
}

fn load_children(tree: &TreeCtrl, item: &TreeItemId) {
    let Some(path) = path_of(tree, item) else { return };
    // Only a folder whose single child is the placeholder still needs reading.
    let Some((first, _)) = tree.get_first_child(item) else { return };
    if tree.get_item_text(&first).as_deref() != Some(DUMMY) {
        return;
    }
    tree.delete_children(item);
    for (p, is_dir) in list_dir(&path) {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(child) = tree.append_item(item, &name, None, None) {
            if is_dir {
                tree.append_item(&child, DUMMY, None, None);
            }
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

fn side_columns(side: &ListCtrl, cols: &[(&str, i32)]) {
    while side.get_column_count() > 0 {
        // No delete_column in the binding: rebuild the control's columns by clearing everything.
        side.clear_all();
        break;
    }
    for (i, (name, w)) in cols.iter().enumerate() {
        if (i as i32) < side.get_column_count() {
            side.set_column_width(i as i64, *w);
        } else {
            side.insert_column(i as i64, name, ListColumnFormat::Left, *w);
        }
    }
}

impl App {
    pub fn tree_visible(&self) -> bool {
        self.panels.tree_box.is_shown()
    }

    /// Show `dir` in the workspace tree and make it the root for Quick Open / Find in Files.
    pub fn open_workspace(&mut self, dir: PathBuf) {
        let t = self.panels.tree;
        ROOT.with(|r| *r.borrow_mut() = Some(dir.clone()));
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string());
        self.panels.title.set_label(&name);
        t.delete_all_items();
        if let Some(root) = t.add_root(&name, None, None) {
            t.append_item(&root, DUMMY, None, None);
            load_children(&t, &root);
            t.expand(&root);
        }
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
        if !inside {
            if let Some(d) = p.parent() {
                self.open_workspace(d.to_path_buf());
            }
        }
        self.set_tree(true);
        if let Some(root) = self.ws.workspace.clone() {
            if !reveal(&self.panels.tree, &root, &p) {
                self.flash = Some("The file was not found in the tree".into());
            }
        }
    }

    /// Switch the side list to `want`, or hide it when it already shows that.
    pub fn toggle_side(&mut self, want: Mode) {
        if mode() == Some(want) {
            MODE.with(|m| m.set(None));
            self.panels.side.show(false);
        } else {
            if want == Mode::Clipboard {
                self.poll_clipboard();
            }
            MODE.with(|m| m.set(Some(want)));
            self.side_key = None;
            self.panels.side.show(true);
            self.fill_side();
        }
        self.editor_pane.layout();
    }

    /// Remember a copied text (newest first, no repeats).
    pub fn poll_clipboard(&mut self) {
        let Some(text) = Clipboard::get().get_text() else { return };
        if text.is_empty() || HISTORY.with(|h| h.borrow().first() == Some(&text)) {
            return;
        }
        HISTORY.with(|h| {
            let mut h = h.borrow_mut();
            h.retain(|x| x != &text);
            h.insert(0, text.clone());
            h.truncate(MAX_HISTORY);
        });
        if mode() == Some(Mode::Clipboard) {
            self.fill_side();
        }
    }

    pub fn fill_side(&self) {
        let l = self.panels.side;
        l.delete_all_items();
        match mode() {
            Some(Mode::Characters) => {
                side_columns(&l, &[("Value", 60), ("Hex", 60), ("Char", 100)]);
                for n in 0..=255u8 {
                    l.insert_item(n as i64, &n.to_string(), None);
                    l.set_item_text_by_column(n as i64, 1, &format!("{n:02X}"));
                    l.set_item_text_by_column(n as i64, 2, &label_for(n));
                }
            }
            Some(Mode::Clipboard) => {
                side_columns(&l, &[("Clipboard History", SIDE_W - 24)]);
                HISTORY.with(|h| {
                    for (i, s) in h.borrow().iter().enumerate() {
                        l.insert_item(i as i64, &preview(s), None);
                    }
                });
            }
            Some(m) => {
                let cols: &[(&str, i32)] = if m == Mode::Documents { &[("Name", 100), ("Path", SIDE_W - 124)] } else { &[("Function", SIDE_W - 74), ("Line", 50)] };
                side_columns(&l, cols);
                ROWS.with(|r| {
                    for (i, (cells, _)) in r.borrow().iter().enumerate() {
                        l.insert_item(i as i64, cells.first().map_or("", String::as_str), None);
                        for (cidx, text) in cells.iter().enumerate().skip(1) {
                            l.set_item_text_by_column(i as i64, cidx as i32, text);
                        }
                    }
                });
            }
            None => {}
        }
    }

    /// Replace the rows of the Documents / Functions views. Does nothing if unchanged.
    fn set_side_rows(&self, rows: Vec<(Vec<String>, usize)>) {
        if ROWS.with(|r| *r.borrow() != rows) {
            ROWS.with(|r| *r.borrow_mut() = rows);
            if matches!(mode(), Some(Mode::Documents | Mode::Functions)) {
                self.fill_side();
            }
        }
    }

    /// Keep the Documents / Functions lists in step with the workspace.
    pub fn refresh_side(&mut self) {
        match mode() {
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
                let rows = functions::extract(&text, self.ws.tabs[self.ws.active].lang).into_iter().map(|e| (vec![e.name, (e.line + 1).to_string()], e.line)).collect();
                self.set_side_rows(rows);
            }
            _ => {}
        }
    }

    /// Theme the tree and the list like the editor.
    pub fn style_panels(&self) {
        let th = &self.ws.theme;
        let (bg, fg) = (Colour::rgb(th.bg.0, th.bg.1, th.bg.2), Colour::rgb(th.fg.0, th.fg.1, th.fg.2));
        for w in [&self.panels.tree as &dyn WxWidget, &self.panels.side] {
            w.set_background_color(bg);
            w.set_foreground_color(fg);
        }
        self.panels.tree_box.set_background_color(bg);
        self.panels.title.set_foreground_color(fg);
    }

    /// What a double-click in the side list does.
    fn side_activate(&mut self, n: usize) {
        match mode() {
            Some(Mode::Characters) => self.ws.insert_text(&char_for(n as u8).to_string()),
            Some(Mode::Clipboard) => {
                if let Some(t) = HISTORY.with(|h| h.borrow().get(n).cloned()) {
                    self.ws.insert_text(&t);
                }
            }
            Some(Mode::Documents) => {
                if let Some(i) = ROWS.with(|r| r.borrow().get(n).map(|x| x.1)) {
                    self.ws.activate(i);
                }
            }
            Some(Mode::Functions) => {
                if let Some(line) = ROWS.with(|r| r.borrow().get(n).map(|x| x.1)) {
                    search::goto_line(&self.ws.sci, line + 1);
                }
            }
            None => {}
        }
        self.refresh();
        self.stc.set_focus();
    }

    /// Commands of the panels; true when `id` was one of them.
    pub fn panel_cmd(&mut self, id: usize) -> bool {
        match id {
            c::ID_CHAR_PANEL => self.toggle_side(Mode::Characters),
            c::ID_CLIP_HISTORY => self.toggle_side(Mode::Clipboard),
            c::ID_DOC_LIST => self.toggle_side(Mode::Documents),
            c::ID_FUNC_LIST => self.toggle_side(Mode::Functions),
            c::ID_WORKSPACE_TOGGLE => {
                if self.ws.workspace.is_some() {
                    let on = !self.tree_visible();
                    self.set_tree(on);
                } else {
                    self.flash = Some("Open a folder as workspace first".into());
                }
            }
            c::ID_OPEN_WORKSPACE => {
                let d = DirDialog::builder(&self.frame, "Open Folder as Workspace", "").build();
                if d.show_modal() == ID_OK {
                    if let Some(p) = d.get_path() {
                        self.open_workspace(PathBuf::from(p));
                    }
                }
            }
            c::ID_OPEN_FOLDER_WS => match self.ws.tabs[self.ws.active].path.clone() {
                Some(p) => {
                    if let Some(d) = p.parent() {
                        self.open_workspace(d.to_path_buf());
                    }
                }
                None => self.flash = Some("Save the file first".into()),
            },
            c::ID_TAB_REVEAL => self.reveal_in_tree(),
            _ => return false,
        }
        true
    }
}
