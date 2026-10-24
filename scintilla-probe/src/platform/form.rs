//! A tiny modal form: labels, edit boxes, check boxes and radio buttons laid
//! out at fixed positions, run with its own message loop. Used by the Column
//! Editor and the custom date/time format prompt.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Label,
    Edit,
    /// A multi-line edit box; `true` makes it read-only.
    MultiEdit(bool),
    Check,
    /// `true` starts a new group of radio buttons.
    Radio(bool),
    Group,
    Button,
    /// A report-style list view (fill it with `Form::list_fill`).
    List,
}

pub struct Ctl {
    pub id: usize,
    pub kind: Kind,
    pub text: String,
    pub rect: (i32, i32, i32, i32),
}

pub fn ctl(id: usize, kind: Kind, text: &str, x: i32, y: i32, w: i32, h: i32) -> Ctl {
    Ctl { id, kind, text: text.into(), rect: (x, y, w, h) }
}

/// Live access to the controls while the form is open.
pub struct Form {
    controls: HashMap<usize, HWND>,
    hwnd: HWND,
}

impl Form {
    /// The form window, for dialogs opened on top of it.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Replace a list's columns and rows.
    pub fn list_fill(&self, id: usize, columns: &[(&str, i32)], rows: &[Vec<String>]) {
        use windows::Win32::UI::Controls::*;
        let Some(h) = self.controls.get(&id) else { return };
        unsafe {
            SendMessageW(*h, LVM_DELETEALLITEMS, None, None);
            while SendMessageW(*h, LVM_DELETECOLUMN, Some(WPARAM(0)), None).0 != 0 {}
            for (i, (name, width)) in columns.iter().enumerate() {
                let mut t = wide(name);
                let col = LVCOLUMNW { mask: LVCF_TEXT | LVCF_WIDTH, cx: *width, pszText: windows::core::PWSTR(t.as_mut_ptr()), ..Default::default() };
                SendMessageW(*h, LVM_INSERTCOLUMNW, Some(WPARAM(i)), Some(LPARAM(&col as *const _ as isize)));
            }
            for (r, row) in rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    self.list_set_cell(id, r, c, cell);
                }
            }
        }
    }

    pub fn list_set_cell(&self, id: usize, row: usize, col: usize, text: &str) {
        use windows::Win32::UI::Controls::*;
        let Some(h) = self.controls.get(&id) else { return };
        let mut t = wide(text);
        let mut item = LVITEMW { mask: LVIF_TEXT, iItem: row as i32, iSubItem: col as i32, pszText: windows::core::PWSTR(t.as_mut_ptr()), ..Default::default() };
        unsafe {
            let msg = if col == 0 { LVM_INSERTITEMW } else { LVM_SETITEMTEXTW };
            SendMessageW(*h, msg, Some(WPARAM(row)), Some(LPARAM(&mut item as *mut _ as isize)));
        }
    }

    pub fn list_selected(&self, id: usize) -> Option<usize> {
        use windows::Win32::UI::Controls::*;
        let h = self.controls.get(&id)?;
        let n = unsafe { SendMessageW(*h, LVM_GETNEXTITEM, Some(WPARAM(usize::MAX)), Some(LPARAM(LVNI_SELECTED as isize))) }.0;
        (n >= 0).then_some(n as usize)
    }

    pub fn set_text(&self, id: usize, s: &str) {
        if let Some(h) = self.controls.get(&id) {
            let t = wide(s);
            unsafe {
                let _ = SetWindowTextW(*h, PCWSTR(t.as_ptr()));
            }
        }
    }
    pub fn text(&self, id: usize) -> String {
        self.controls.get(&id).map(|h| text_of(*h)).unwrap_or_default()
    }
    pub fn checked(&self, id: usize) -> bool {
        self.controls.get(&id).is_some_and(|h| unsafe { SendMessageW(*h, 0x00F0, None, None).0 == 1 }) // BM_GETCHECK
    }
    pub fn set_checked(&self, id: usize, on: bool) {
        if let Some(h) = self.controls.get(&id) {
            unsafe {
                SendMessageW(*h, 0x00F1, Some(WPARAM(on as usize)), None); // BM_SETCHECK
            }
        }
    }
    pub fn enable(&self, id: usize, on: bool) {
        if let Some(h) = self.controls.get(&id) {
            unsafe {
                let _ = EnableWindow(*h, on);
            }
        }
    }
    pub fn focus(&self, id: usize) {
        if let Some(h) = self.controls.get(&id) {
            unsafe {
                let _ = SetFocus(Some(*h));
                SendMessageW(*h, 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1))); // EM_SETSEL all
            }
        }
    }
}

/// What the user entered, read just before the form closed.
pub struct Values {
    text: HashMap<usize, String>,
    checked: HashMap<usize, bool>,
}

impl Values {
    pub fn text(&self, id: usize) -> &str {
        self.text.get(&id).map(String::as_str).unwrap_or("")
    }
    pub fn checked(&self, id: usize) -> bool {
        self.checked.get(&id).copied().unwrap_or(false)
    }
}

pub struct Spec<'a> {
    pub title: &'a str,
    pub size: (i32, i32),
    pub ctls: Vec<Ctl>,
    pub ok: &'a str,
    /// Called after a button, check box or radio button is clicked, or a list
    /// selection changes; gets the control's id.
    pub on_change: Option<fn(&Form, usize)>,
    /// Called when OK is pressed; returning true keeps the form open (like a Find button).
    pub on_apply: Option<&'a dyn Fn(&Values, &Form) -> bool>,
}

thread_local! {
    static DONE: Cell<Option<bool>> = const { Cell::new(None) };
    static LIVE: RefCell<Option<(Form, Option<fn(&Form, usize)>)>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn text_of(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

unsafe extern "system" fn form_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let (id, code) = (wp.0 & 0xFFFF, (wp.0 >> 16) & 0xFFFF);
            match id {
                1 => DONE.with(|d| d.set(Some(true))),
                2 => DONE.with(|d| d.set(Some(false))),
                _ if code == 0 => {
                    // BN_CLICKED: let the form react (e.g. enable/disable groups).
                    let cb = LIVE.with(|l| l.borrow().as_ref().and_then(|(_, cb)| *cb));
                    if let Some(cb) = cb {
                        LIVE.with(|l| {
                            if let Some((f, _)) = l.borrow().as_ref() {
                                cb(f, id)
                            }
                        });
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            // A different row was selected in a list.
            let hdr = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::NMHDR) };
            if hdr.code == windows::Win32::UI::Controls::LVN_ITEMCHANGED {
                let nm = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::NMLISTVIEW) };
                if nm.uNewState & 2 != 0 && nm.uOldState & 2 == 0 {
                    let cb = LIVE.with(|l| l.borrow().as_ref().and_then(|(_, cb)| *cb));
                    if let Some(cb) = cb {
                        LIVE.with(|l| {
                            if let Some((f, _)) = l.borrow().as_ref() {
                                cb(f, hdr.idFrom)
                            }
                        });
                    }
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            DONE.with(|d| d.set(Some(false)));
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn register() {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppForm"), &mut wc).is_ok() {
            return;
        }
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(form_proc),
            hInstance: hinst.into(),
            lpszClassName: w!("NoteppForm"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH(16 as *mut core::ffi::c_void), // COLOR_BTNFACE + 1
            ..Default::default()
        });
    }
}

/// Show the form modally. Returns the entered values on OK, `None` on Cancel.
/// `init` runs once the controls exist (to set defaults and enable states).
pub fn run(owner: HWND, spec: Spec, init: impl FnOnce(&Form)) -> Option<Values> {
    register();
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME;
        let mut rc = RECT { left: 0, top: 0, right: spec.size.0, bottom: spec.size.1 };
        let _ = AdjustWindowRectEx(&mut rc, style, false, ex);
        let (ww, wh) = (rc.right - rc.left, rc.bottom - rc.top);
        let mut orc = RECT::default();
        let _ = GetWindowRect(owner, &mut orc);
        let title = wide(spec.title);
        let hwnd = CreateWindowExW(
            ex,
            w!("NoteppForm"),
            PCWSTR(title.as_ptr()),
            style,
            orc.left + ((orc.right - orc.left) - ww) / 2,
            orc.top + ((orc.bottom - orc.top) - wh) / 3,
            ww,
            wh,
            Some(owner),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .ok()?;

        let font = super::chrome::ui_font_handle();
        let mut controls = HashMap::new();
        let mut first_edit = None;
        let make = |class: PCWSTR, text: &str, style: u32, id: usize, r: (i32, i32, i32, i32)| {
            let t = wide(text);
            let h = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                PCWSTR(t.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                r.0,
                r.1,
                r.2,
                r.3,
                Some(hwnd),
                Some(HMENU(id as *mut core::ffi::c_void)),
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
            .unwrap();
            SendMessageW(h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            h
        };
        const TAB: u32 = 0x0001_0000;
        const GROUP: u32 = 0x0002_0000;
        for c in &spec.ctls {
            let h = match c.kind {
                Kind::Label => make(w!("STATIC"), &c.text, 0, c.id, c.rect),
                Kind::Edit => {
                    let h = make(w!("EDIT"), &c.text, TAB | 0x0080_0000 | 0x80, c.id, c.rect); // WS_BORDER | ES_AUTOHSCROLL
                    first_edit.get_or_insert(c.id);
                    h
                }
                Kind::Check => make(w!("BUTTON"), &c.text, TAB | 0x3, c.id, c.rect), // BS_AUTOCHECKBOX
                Kind::Radio(group) => make(w!("BUTTON"), &c.text, 0x9 | if group { GROUP | TAB } else { 0 }, c.id, c.rect), // BS_AUTORADIOBUTTON
                Kind::Group => make(w!("BUTTON"), &c.text, 0x7, c.id, c.rect), // BS_GROUPBOX
                Kind::Button => make(w!("BUTTON"), &c.text, TAB, c.id, c.rect),
                // ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | WS_VSCROLL | WS_BORDER (| ES_READONLY)
                Kind::MultiEdit(readonly) => make(w!("EDIT"), &c.text, TAB | 0x0020_0000 | 0x0080_0000 | 0x4 | 0x40 | 0x1000 | if readonly { 0x800 } else { 0 }, c.id, c.rect),
                Kind::List => {
                    // LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS
                    let h = make(w!("SysListView32"), "", TAB | 0x0080_0000 | 0x1 | 0x4 | 0x8, c.id, c.rect);
                    SendMessageW(h, 0x1036, Some(WPARAM(0x20)), Some(LPARAM(0x20))); // LVM_SETEXTENDEDLISTVIEWSTYLE: full-row select
                    h
                }
            };
            controls.insert(c.id, h);
        }
        let (bw, bh, by) = (80, 26, spec.size.1 - 36);
        let ok = make(w!("BUTTON"), spec.ok, TAB | GROUP | 0x1, 1, (spec.size.0 - 2 * bw - 24, by, bw, bh)); // BS_DEFPUSHBUTTON
        let cancel = make(w!("BUTTON"), "Cancel", TAB | 0x0, 2, (spec.size.0 - bw - 12, by, bw, bh));
        controls.insert(1, ok);
        controls.insert(2, cancel);

        let form = Form { controls, hwnd };
        init(&form);
        if let Some(id) = first_edit {
            form.focus(id);
        }
        LIVE.with(|l| *l.borrow_mut() = Some((form, spec.on_change)));
        DONE.with(|d| d.set(None));

        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let collect = |form: &Form| {
            let mut values = Values { text: HashMap::new(), checked: HashMap::new() };
            for c in &spec.ctls {
                match c.kind {
                    Kind::Edit | Kind::MultiEdit(_) => drop(values.text.insert(c.id, form.text(c.id))),
                    Kind::Check | Kind::Radio(_) => drop(values.checked.insert(c.id, form.checked(c.id))),
                    _ => {}
                }
            }
            values
        };
        let mut msg = MSG::default();
        let accepted = loop {
            while DONE.with(|d| d.get()).is_none() {
                if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    PostQuitMessage(msg.wParam.0 as i32);
                    DONE.with(|d| d.set(Some(false)));
                    break;
                }
                if !IsDialogMessageW(hwnd, &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let ok = DONE.with(|d| d.get()) == Some(true);
            if ok {
                if let Some(apply) = spec.on_apply {
                    let keep_open = LIVE.with(|l| l.borrow().as_ref().map(|(f, _)| apply(&collect(f), f)).unwrap_or(false));
                    if keep_open {
                        DONE.with(|d| d.set(None));
                        continue;
                    }
                }
            }
            break ok;
        };

        let live = LIVE.with(|l| l.borrow_mut().take());
        let values = live.as_ref().map(|(f, _)| collect(f)).unwrap_or(Values { text: HashMap::new(), checked: HashMap::new() });
        let _ = EnableWindow(owner, true);
        let _ = DestroyWindow(hwnd);
        let _ = SetForegroundWindow(owner);
        accepted.then_some(values)
    }
}
