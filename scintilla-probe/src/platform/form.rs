//! A small modal form: labels, edit boxes, check boxes, radio buttons, lists and
//! buttons laid out at fixed positions, run with its own message loop. All controls are
//! the system's native ones (themed by the app manifest); `modern` only switches them
//! between the light and dark themes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::modern::{self, Ctl as Style};

/// Which system icon a message dialog shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Info,
    Question,
    Warning,
    Error,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// The system's information / question / warning / error icon; used by message dialogs.
    Badge(Tone),
    Label,
    Edit,
    /// A multi-line edit box; `true` makes it read-only.
    MultiEdit(bool),
    Check,
    /// `true` starts a new group of radio buttons.
    Radio(bool),
    Group,
    Button,
    /// A push button that Enter presses.
    DefButton,
    /// A report-style list view (fill it with `Form::list_fill`).
    List,
    /// Like `List`, but several rows can be selected.
    MultiList,
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

    /// Every selected row, top to bottom.
    pub fn list_selected_all(&self, id: usize) -> Vec<usize> {
        use windows::Win32::UI::Controls::*;
        let Some(h) = self.controls.get(&id) else { return Vec::new() };
        let mut rows = Vec::new();
        let mut from = usize::MAX;
        loop {
            let n = unsafe { SendMessageW(*h, LVM_GETNEXTITEM, Some(WPARAM(from)), Some(LPARAM(LVNI_SELECTED as isize))) }.0;
            if n < 0 {
                return rows;
            }
            rows.push(n as usize);
            from = n as usize;
        }
    }

    pub fn list_selected(&self, id: usize) -> Option<usize> {
        use windows::Win32::UI::Controls::*;
        let h = self.controls.get(&id)?;
        let n = unsafe { SendMessageW(*h, LVM_GETNEXTITEM, Some(WPARAM(usize::MAX)), Some(LPARAM(LVNI_SELECTED as isize))) }.0;
        (n >= 0).then_some(n as usize)
    }

    /// Select and show row `row` of a list.
    pub fn list_select(&self, id: usize, row: usize) {
        use windows::Win32::UI::Controls::*;
        let Some(h) = self.controls.get(&id) else { return };
        let both = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let item = LVITEMW { stateMask: both, state: both, ..Default::default() };
        unsafe {
            SendMessageW(*h, LVM_SETITEMSTATE, Some(WPARAM(row)), Some(LPARAM(&item as *const _ as isize)));
            SendMessageW(*h, LVM_ENSUREVISIBLE, Some(WPARAM(row)), Some(LPARAM(0)));
        }
    }

    /// Close the form as if OK (`true`) or Cancel was pressed.
    pub fn close(&self, ok: bool) {
        DONE.with(|d| d.set(Some(ok)));
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

/// Whether a form is currently open (forms cannot be nested).
pub fn is_open() -> bool {
    LIVE.with(|l| l.borrow().is_some())
}

/// Height in pixels `text` needs when wrapped to `width` in the dialog font.
pub fn text_height(text: &str, width: i32) -> i32 {
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT { left: 0, top: 0, right: width, bottom: 0 };
        if !buf.is_empty() {
            DrawTextW(dc, &mut buf, &mut r, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
        }
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        r.bottom - r.top
    }
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
            // Dark mode: themed radio buttons draw their text in a near-invisible grey, so
            // paint the label again in the normal text colour after the control has drawn.
            if hdr.code == (-12i32) as u32 && modern::is_dark() {
                let nm = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::NMCUSTOMDRAW) };
                let h = hdr.hwndFrom;
                let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32;
                let mut class = [0u16; 8];
                let n = unsafe { GetClassNameW(h, &mut class) }.max(0) as usize;
                let radio = String::from_utf16_lossy(&class[..n]).eq_ignore_ascii_case("Button") && matches!(style & 0xF, 4 | 9);
                if radio {
                    match nm.dwDrawStage.0 {
                        1 => return LRESULT(0x10), // CDRF_NOTIFYPOSTPAINT
                        2 => unsafe {
                            let mut text = [0u16; 128];
                            let len = GetWindowTextW(h, &mut text).max(0) as usize;
                            let p = modern::pal();
                            let mut r = nm.rc;
                            r.left += 20;
                            FillRect(nm.hdc, &r, modern::brush(p.bg));
                            let font = SendMessageW(h, WM_GETFONT, None, None).0;
                            let old = SelectObject(nm.hdc, HGDIOBJ(font as *mut _));
                            SetBkMode(nm.hdc, TRANSPARENT);
                            SetTextColor(nm.hdc, modern::colorref(p.text));
                            DrawTextW(nm.hdc, &mut text[..len], &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
                            SelectObject(nm.hdc, old);
                        },
                        _ => {}
                    }
                }
                return LRESULT(0);
            }            if hdr.code == windows::Win32::UI::Controls::LVN_ITEMCHANGED {
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
        WM_ERASEBKGND => modern::erase_background(hwnd, wp),
        // Labels are transparent over the window colour; edit boxes get the field colour.
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => modern::ctl_color(msg, wp, lp),
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
            // No class brush: WM_ERASEBKGND paints the themed background and the cards.
            ..Default::default()
        });
    }
}

/// Show the form modally. Returns the entered values on OK, `None` on Cancel.
/// `init` runs once the controls exist (to set defaults and enable states).
pub fn run(owner: HWND, spec: Spec, init: impl FnOnce(&Form)) -> Option<Values> {
    register();
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
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
        // Edit boxes and lists use the themed (client-edge) border, which Windows 11 draws rounded.
        let sunken = |h: HWND| {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_CLIENTEDGE.0 as isize);
            let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
        };
        modern::frame(hwnd);
        for c in &spec.ctls {
            let h = match c.kind {
                Kind::Group => {
                    // A section heading: a native label with a native etched line under it
                    // (a themed group box would paint an opaque fill under its children in dark mode).
                    let (x, y, w, _) = c.rect;
                    let line = make(w!("STATIC"), "", 0x10, 0, (x, y + 18, w, 2)); // SS_ETCHEDHORZ
                    let _ = line;
                    make(w!("STATIC"), &c.text, 0, c.id, (x, y, w, 18))
                }
                Kind::Label => make(w!("STATIC"), &c.text, 0, c.id, c.rect),
                Kind::Badge(tone) => {
                    // SS_ICON with the system's own information / question / warning / error icon
                    let h = make(w!("STATIC"), "", 0x3, c.id, c.rect);
                    let icon = match tone {
                        Tone::Info => IDI_INFORMATION,
                        Tone::Question => IDI_QUESTION,
                        Tone::Warning => IDI_WARNING,
                        Tone::Error => IDI_ERROR,
                    };
                    if let Ok(i) = LoadIconW(None, icon) {
                        SendMessageW(h, 0x0170, Some(WPARAM(i.0 as usize)), None); // STM_SETICON
                    }
                    h
                }
                Kind::Edit => {
                    let h = make(w!("EDIT"), &c.text, TAB | 0x80, c.id, c.rect); // ES_AUTOHSCROLL
                    sunken(h);
                    modern::style_control(h, Style::Edit);
                    first_edit.get_or_insert(c.id);
                    h
                }
                Kind::Check => {
                    let h = make(w!("BUTTON"), &c.text, TAB | 0x3, c.id, c.rect); // BS_AUTOCHECKBOX
                    modern::style_control(h, Style::Button);
                    h
                }
                Kind::Radio(group) => {
                    let h = make(w!("BUTTON"), &c.text, 0x9 | if group { GROUP | TAB } else { 0 }, c.id, c.rect); // BS_AUTORADIOBUTTON
                    modern::style_control(h, Style::Button);
                    h
                }
                Kind::Button | Kind::DefButton => {
                    // BS_PUSHBUTTON, or BS_DEFPUSHBUTTON for the one Enter presses
                    let h = make(w!("BUTTON"), &c.text, TAB | if c.kind == Kind::DefButton { 0x1 } else { 0 }, c.id, c.rect);
                    modern::style_control(h, Style::Button);
                    h
                }
                // ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | WS_VSCROLL (| ES_READONLY)
                Kind::MultiEdit(readonly) => {
                    let h = make(w!("EDIT"), &c.text, TAB | 0x0020_0000 | 0x4 | 0x40 | 0x1000 | if readonly { 0x800 } else { 0 }, c.id, c.rect);
                    sunken(h);
                    modern::style_control(h, Style::Edit);
                    h
                }
                Kind::List | Kind::MultiList => {
                    // LVS_REPORT | LVS_SHOWSELALWAYS (| LVS_SINGLESEL)
                    let single = if c.kind == Kind::List { 0x4 } else { 0 };
                    let h = make(w!("SysListView32"), "", TAB | 0x1 | 0x8 | single, c.id, c.rect);
                    sunken(h);
                    // LVM_SETEXTENDEDLISTVIEWSTYLE: full-row select + double buffering
                    SendMessageW(h, 0x1036, Some(WPARAM(0x1_0020)), Some(LPARAM(0x1_0020)));
                    modern::style_control(h, Style::List);
                    h
                }
            };
            controls.insert(c.id, h);
        }
        let (bw, bh, by) = (80, 26, spec.size.1 - 36);
        if spec.ok.is_empty() {
            // The form supplies its own buttons; keep a hidden Cancel so Esc still closes it.
            let cancel = make(w!("BUTTON"), "Cancel", 0, 2, (0, 0, 0, 0));
            let _ = ShowWindow(cancel, SW_HIDE);
            controls.insert(2, cancel);
        } else {
            let ok = make(w!("BUTTON"), spec.ok, TAB | GROUP | 0x1, 1, (spec.size.0 - 2 * bw - 24, by, bw, bh)); // BS_DEFPUSHBUTTON
            let cancel = make(w!("BUTTON"), "Cancel", TAB, 2, (spec.size.0 - bw - 12, by, bw, bh));
            modern::style_control(ok, Style::Button);
            modern::style_control(cancel, Style::Button);
            controls.insert(1, ok);
            controls.insert(2, cancel);
        }

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
