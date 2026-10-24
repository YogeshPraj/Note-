//! The folder workspace panel: a lazily-expanding file tree.
//!
//! Directories are only read when first expanded, so opening a huge folder is
//! instant. Double-click (or Enter) on a file asks the host to open it; the
//! host hands every `WM_NOTIFY` from this control to `notify()`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PWSTR, w};

use super::chrome::ui_font_handle;

pub const WIDTH: i32 = 260;
/// Folders that are never worth listing.
const HIDDEN: [&str; 1] = [".git"];
const MAX_PER_DIR: usize = 5000;

struct Entry {
    path: PathBuf,
    is_dir: bool,
    loaded: bool,
}

thread_local! {
    static TREE: Cell<Option<HWND>> = const { Cell::new(None) };
    static ENTRIES: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

pub fn hwnd() -> Option<HWND> {
    TREE.with(|c| c.get())
}

pub fn create(host: HWND) -> HWND {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES,
        });
        // TVS_HASBUTTONS | TVS_HASLINES | TVS_LINESATROOT | TVS_SHOWSELALWAYS
        let style = WS_CHILD | WS_CLIPSIBLINGS | WS_BORDER | WINDOW_STYLE(0x1 | 0x2 | 0x4 | 0x20);
        let tree = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SysTreeView32"),
            None,
            style | WS_TABSTOP,
            0,
            0,
            WIDTH,
            100,
            Some(host),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap();
        SendMessageW(tree, WM_SETFONT, Some(WPARAM(ui_font_handle().0 as usize)), Some(LPARAM(1)));
        TREE.with(|c| c.set(Some(tree)));
        tree
    }
}

/// Add one item under `parent` (None = root) and return its handle.
fn insert(parent: Option<HTREEITEM>, path: &Path, is_dir: bool, label: &str) -> Option<HTREEITEM> {
    let tree = hwnd()?;
    let idx = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        e.push(Entry { path: path.to_path_buf(), is_dir, loaded: false });
        e.len() - 1
    });
    let mut text: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
    let item = TVITEMW {
        mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN,
        pszText: PWSTR(text.as_mut_ptr()),
        cChildren: TVITEMEXW_CHILDREN(is_dir as i32),
        lParam: LPARAM(idx as isize),
        ..Default::default()
    };
    let ins = TVINSERTSTRUCTW {
        hParent: parent.unwrap_or(TVI_ROOT),
        hInsertAfter: TVI_LAST,
        Anonymous: TVINSERTSTRUCTW_0 { item },
    };
    let h = unsafe { SendMessageW(tree, TVM_INSERTITEMW, None, Some(LPARAM(&ins as *const _ as isize))) }.0;
    (h != 0).then_some(HTREEITEM(h))
}

/// Directories first, then files, each alphabetical (case-insensitive).
pub fn list_dir(dir: &Path) -> Vec<(PathBuf, bool)> {
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

fn load_children(parent: HTREEITEM, idx: usize) {
    let path = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let ent = e.get_mut(idx)?;
        if !ent.is_dir || ent.loaded {
            return None;
        }
        ent.loaded = true;
        Some(ent.path.clone())
    });
    let Some(path) = path else { return };
    for (p, is_dir) in list_dir(&path) {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        insert(Some(parent), &p, is_dir, &name);
    }
}

/// Show `root` as the tree's single top-level item, expanded.
pub fn set_root(root: &Path) {
    let Some(tree) = hwnd() else { return };
    unsafe {
        SendMessageW(tree, TVM_DELETEITEM, None, Some(LPARAM(TVI_ROOT.0)));
    }
    ENTRIES.with(|e| e.borrow_mut().clear());
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
    if let Some(item) = insert(None, root, true, &name) {
        load_children(item, 0);
        unsafe {
            SendMessageW(tree, TVM_EXPAND, Some(WPARAM(TVE_EXPAND.0 as usize)), Some(LPARAM(item.0)));
        }
    }
}

fn param_of(item: HTREEITEM) -> Option<usize> {
    let tree = hwnd()?;
    let mut it = TVITEMW { mask: TVIF_PARAM, hItem: item, ..Default::default() };
    let ok = unsafe { SendMessageW(tree, TVM_GETITEMW, None, Some(LPARAM(&mut it as *mut _ as isize))) }.0;
    (ok != 0).then_some(it.lParam.0 as usize)
}

fn selected_file() -> Option<PathBuf> {
    let tree = hwnd()?;
    let sel = unsafe { SendMessageW(tree, TVM_GETNEXTITEM, Some(WPARAM(TVGN_CARET as usize)), None) }.0;
    if sel == 0 {
        return None;
    }
    let idx = param_of(HTREEITEM(sel))?;
    ENTRIES.with(|e| e.borrow().get(idx).filter(|x| !x.is_dir).map(|x| x.path.clone()))
}

/// Handle a `WM_NOTIFY` that came from the tree. Returns a file to open, if any.
pub fn notify(lparam: LPARAM) -> Option<PathBuf> {
    let hdr = unsafe { &*(lparam.0 as *const NMHDR) };
    if Some(hdr.hwndFrom) != hwnd() {
        return None;
    }
    match hdr.code {
        TVN_ITEMEXPANDINGW => {
            let nm = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
            if nm.action == TVE_EXPAND {
                load_children(nm.itemNew.hItem, nm.itemNew.lParam.0 as usize);
            }
            None
        }
        NM_DBLCLK | NM_RETURN => selected_file(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_sorted_and_hides_git() {
        let d = std::env::temp_dir().join("notepp_tree_list");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("zeta")).unwrap();
        std::fs::create_dir_all(d.join("Alpha")).unwrap();
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join("b.txt"), "").unwrap();
        std::fs::write(d.join("A.txt"), "").unwrap();
        let names: Vec<String> = list_dir(&d).into_iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["Alpha", "zeta", "A.txt", "b.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
