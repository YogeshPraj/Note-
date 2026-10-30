//! The folder workspace panel: a lazily-expanding file tree.
//!
//! Directories are only read when first expanded, so opening a huge folder is
//! instant. Double-click (or Enter) on a file asks the host to open it; the
//! host hands every `WM_NOTIFY` from this control to `notify()`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PWSTR, w};

use super::chrome::ui_font_handle;

pub const WIDTH: i32 = 260;
/// Height of the bar above the tree (folder name and the close button).
const HEADER_H: i32 = 26;
/// The command the close button sends (View > Folder as Workspace toggles the pane).
const CLOSE_ID: usize = 3007;
/// Folders that are never worth listing.
const HIDDEN: [&str; 1] = [".git"];
const MAX_PER_DIR: usize = 5000;

struct Entry {
    path: PathBuf,
    is_dir: bool,
    loaded: bool,
}

thread_local! {
    static HEADER: Cell<Option<HWND>> = const { Cell::new(None) };
    static TITLE: RefCell<String> = const { RefCell::new(String::new()) };
    static CLOSE_HOT: Cell<bool> = const { Cell::new(false) };
    static TREE: Cell<Option<HWND>> = const { Cell::new(None) };
    static ENTRIES: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

pub fn hwnd() -> Option<HWND> {
    TREE.with(|c| c.get())
}

fn close_rect(w: i32) -> RECT {
    RECT { left: w - 26, top: 3, right: w - 4, bottom: HEADER_H - 3 }
}

unsafe extern "system" fn header_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let at = |lp: LPARAM| ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    let on_close = |x: i32, y: i32| {
        let c = close_rect(rc.right);
        x >= c.left && x < c.right && y >= c.top && y < c.bottom
    };
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let p = super::modern::pal();
            let bg = CreateSolidBrush(super::modern::colorref(p.card));
            FillRect(dc, &rc, bg);
            let _ = DeleteObject(bg.into());
            let line = CreateSolidBrush(super::modern::colorref(p.disabled));
            FillRect(dc, &RECT { left: 0, top: rc.bottom - 1, right: rc.right, bottom: rc.bottom }, line);
            let _ = DeleteObject(line.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, super::modern::colorref(p.text));
            let old = SelectObject(dc, ui_font_handle().into());
            let mut title: Vec<u16> = TITLE.with(|t| t.borrow().to_uppercase()).encode_utf16().collect();
            let mut tr = RECT { left: 10, top: 0, right: rc.right - 30, bottom: rc.bottom - 1 };
            DrawTextW(dc, &mut title, &mut tr, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
            // The close button: a cross, with a soft square under it while the pointer is over it.
            let c = close_rect(rc.right);
            if CLOSE_HOT.with(|h| h.get()) {
                let hot = CreateSolidBrush(super::modern::colorref(if p.bg.0 < 0x80 { (0x45, 0x45, 0x45) } else { (0xD8, 0xD8, 0xD8) }));
                FillRect(dc, &c, hot);
                let _ = DeleteObject(hot.into());
            }
            let pen = CreatePen(PS_SOLID, 1, super::modern::colorref(p.text));
            let op = SelectObject(dc, pen.into());
            let (cx, cy) = ((c.left + c.right) / 2, (c.top + c.bottom) / 2);
            let _ = MoveToEx(dc, cx - 4, cy - 4, None);
            let _ = LineTo(dc, cx + 5, cy + 5);
            let _ = MoveToEx(dc, cx + 4, cy - 4, None);
            let _ = LineTo(dc, cx - 5, cy + 5);
            SelectObject(dc, op);
            let _ = DeleteObject(pen.into());
            SelectObject(dc, old);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        },
        WM_MOUSEMOVE => {
            let (x, y) = at(lp);
            let hot = on_close(x, y);
            if CLOSE_HOT.with(|h| h.replace(hot)) != hot {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT};
                    let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut t);
                }
            }
            LRESULT(0)
        }
        0x02A3 => {
            // WM_MOUSELEAVE
            CLOSE_HOT.with(|h| h.set(false));
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = at(lp);
            if on_close(x, y) {
                unsafe {
                    if let Ok(host) = GetParent(hwnd) {
                        let _ = PostMessageW(Some(host), WM_COMMAND, WPARAM(CLOSE_ID), LPARAM(0));
                    }
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Position the pane: the bar on top, the tree under it. `show` false hides both.
pub fn place(x: i32, y: i32, w: i32, h: i32, show: bool) {
    let cmd = if show { SW_SHOW } else { SW_HIDE };
    unsafe {
        if let Some(hd) = HEADER.with(|c| c.get()) {
            let _ = MoveWindow(hd, x, y, w.max(0), HEADER_H, true);
            let _ = ShowWindow(hd, cmd);
        }
        if let Some(t) = hwnd() {
            let _ = MoveWindow(t, x, y + HEADER_H, w.max(0), (h - HEADER_H).max(0), true);
            let _ = ShowWindow(t, cmd);
        }
    }
}

pub fn create(host: HWND) -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppTreeHeader"), &mut wc).is_err() {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(header_proc),
                hInstance: hinst.into(),
                lpszClassName: w!("NoteppTreeHeader"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                ..Default::default()
            });
        }
        if let Ok(hd) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("NoteppTreeHeader"), None, WS_CHILD | WS_CLIPSIBLINGS, 0, 0, WIDTH, HEADER_H, Some(host), None, Some(hinst.into()), None) {
            HEADER.with(|c| c.set(Some(hd)));
        }
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES,
        });
        // TVS_HASBUTTONS | TVS_LINESATROOT | TVS_SHOWSELALWAYS | TVS_FULLROWSELECT (no lines, no border: modern explorer look)
        let style = WS_CHILD | WS_CLIPSIBLINGS | WINDOW_STYLE(0x1 | 0x4 | 0x20 | 0x1000);
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
        SendMessageW(tree, 0x112C, Some(WPARAM(0x44)), Some(LPARAM(0x44))); // TVM_SETEXTENDEDSTYLE: double buffer + fade expando
        SendMessageW(tree, 0x1107, Some(WPARAM(22)), None); // TVM_SETITEMHEIGHT
        super::modern::style_control(tree, super::modern::Ctl::Tree);
        TREE.with(|c| c.set(Some(tree)));
        tree
    }
}

/// Re-apply light/dark styling after a theme change.
pub fn restyle() {
    if let Some(tree) = hwnd() {
        super::modern::style_control(tree, super::modern::Ctl::Tree);
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
    TITLE.with(|t| *t.borrow_mut() = name.clone());
    if let Some(hd) = HEADER.with(|c| c.get()) {
        unsafe {
            let _ = InvalidateRect(Some(hd), None, true);
        }
    }
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

/// Expand the tree down to `file` (which must be under `root`) and select it. False if it is not there.
pub fn reveal(root: &Path, file: &Path) -> bool {
    let Some(tree) = hwnd() else { return false };
    let Ok(rel) = file.strip_prefix(root) else { return false };
    let send = |m: u32, w: usize, l: isize| unsafe { SendMessageW(tree, m, Some(WPARAM(w)), Some(LPARAM(l))).0 };
    let mut item = send(TVM_GETNEXTITEM, TVGN_ROOT as usize, 0);
    if item == 0 {
        return false;
    }
    for comp in rel.components() {
        // Expanding loads the folder's entries (the host passes the notification to `notify`).
        send(TVM_EXPAND, TVE_EXPAND.0 as usize, item);
        let want = comp.as_os_str().to_string_lossy().to_lowercase();
        let mut child = send(TVM_GETNEXTITEM, TVGN_CHILD as usize, item);
        loop {
            if child == 0 {
                return false;
            }
            let name = param_of(HTREEITEM(child))
                .and_then(|i| ENTRIES.with(|e| e.borrow().get(i).and_then(|x| x.path.file_name().map(|n| n.to_string_lossy().to_lowercase()))));
            if name.as_deref() == Some(want.as_str()) {
                break;
            }
            child = send(TVM_GETNEXTITEM, TVGN_NEXT as usize, child);
        }
        item = child;
    }
    send(TVM_SELECTITEM, TVGN_CARET as usize, item);
    send(TVM_ENSUREVISIBLE, 0, item);
    true
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
