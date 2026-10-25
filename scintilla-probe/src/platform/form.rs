//! A small modal form: labels, edit boxes, check boxes, radio buttons, lists and
//! buttons laid out at fixed positions, run with its own message loop. Drawn in the
//! modern look from `modern`: sections are rounded cards, push buttons are rounded
//! (the default one filled with the accent colour), and everything follows the
//! light/dark theme.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::modern::{self, Ctl as Style};

/// The colour and glyph of a message dialog's badge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Info,
    Question,
    Warning,
    Error,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// A round coloured badge with a glyph ("i", "?", "!", "x"); used by message dialogs.
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
        TICKS.with(|t| t.borrow().get(&id).is_some_and(|k| k.on))
    }
    pub fn set_checked(&self, id: usize, on: bool) {
        set_tick(self, id, on);
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
    /// Section cards of the open form: (title, outer rectangle).
    static SECTIONS: RefCell<Vec<(String, RECT)>> = const { RefCell::new(Vec::new()) };
    /// Badges of the open form: control id -> tone.
    static BADGES: RefCell<HashMap<usize, Tone>> = RefCell::new(HashMap::new());
    /// The button Enter presses and that is filled with the accent colour.
    static DEFAULT_BUTTON: Cell<usize> = const { Cell::new(1) };
    /// Check boxes and radio buttons of the open form (they are drawn by `modern`).
    static TICKS: RefCell<HashMap<usize, Tick>> = RefCell::new(HashMap::new());
}

/// State of one owner-drawn check box or radio button.
struct Tick {
    radio: bool,
    /// Radio buttons in the same group exclude each other.
    group: usize,
    on: bool,
}

/// Set a check box / radio button, clearing its radio group mates, and repaint.
fn set_tick(form: &Form, id: usize, on: bool) {
    let changed: Vec<usize> = TICKS.with(|t| {
        let mut t = t.borrow_mut();
        let Some(me) = t.get(&id) else { return Vec::new() };
        let (radio, group) = (me.radio, me.group);
        let mut changed = vec![id];
        if radio && on {
            for (other, k) in t.iter_mut() {
                if *other != id && k.radio && k.group == group && k.on {
                    k.on = false;
                    changed.push(*other);
                }
            }
        }
        if let Some(me) = t.get_mut(&id) {
            me.on = on;
        }
        changed
    });
    for c in changed {
        if let Some(h) = form.controls.get(&c) {
            unsafe {
                let _ = InvalidateRect(Some(*h), None, false);
            }
        }
    }
}

/// Whether a form is currently open (forms cannot be nested).
pub fn is_open() -> bool {
    LIVE.with(|l| l.borrow().is_some())
}

/// Make button `id` the default one of the form being built (call from the `init` closure).
pub fn set_default_button(id: usize) {
    DEFAULT_BUTTON.with(|d| d.set(id));
}

/// Width in pixels `text` needs on one line in the dialog font.
pub fn text_width(text: &str) -> i32 {
    unsafe {
        let dc = GetDC(None);
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT::default();
        if !buf.is_empty() {
            DrawTextW(dc, &mut buf, &mut r, DT_CALCRECT | DT_SINGLELINE);
        }
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        r.right - r.left
    }
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
                    // A check box or radio button changes its own state first.
                    if let Some((radio, on)) = TICKS.with(|t| t.borrow().get(&id).map(|k| (k.radio, k.on))) {
                        LIVE.with(|l| {
                            if let Some((f, _)) = l.borrow().as_ref() {
                                set_tick(f, id, if radio { true } else { !on });
                            }
                        });
                    }
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
            if let Some(r) = modern::header_custom_draw(lp) {
                return r;
            }
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
        WM_ERASEBKGND => {
            let dc = HDC(wp.0 as *mut core::ffi::c_void);
            let mut r = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut r);
            }
            let p = modern::pal();
            modern::fill(dc, &r, p.bg);
            SECTIONS.with(|s| {
                for (title, rect) in s.borrow().iter() {
                    modern::round_rect(dc, rect, 10, p.card, p.card_border);
                    unsafe {
                        let old = SelectObject(dc, modern::semibold_font().into());
                        let t = RECT { left: rect.left + 14, top: rect.top + 4, right: rect.right - 8, bottom: rect.top + 22 };
                        modern::draw_text(dc, title, &t, p.text, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
                        SelectObject(dc, old);
                    }
                }
            });
            LRESULT(1)
        }
        // Labels are transparent over the card; edit boxes get the field colour.
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => modern::ctl_color(msg, wp, lp),
        WM_DRAWITEM => {
            let dis = unsafe { &*(lp.0 as *const DRAWITEMSTRUCT) };
            match dis.CtlType.0 {
                4 => {
                    // ODT_BUTTON: a check box / radio button, or a push button
                    let tick = TICKS.with(|t| t.borrow().get(&(dis.CtlID as usize)).map(|k| (k.radio, k.on)));
                    match tick {
                        Some((radio, on)) => modern::paint_tick(dis, radio, on, backdrop(hwnd, dis.hwndItem)),
                        None => modern::paint_button(dis, dis.CtlID as usize == DEFAULT_BUTTON.with(|d| d.get())),
                    }
                }
                5 => {
                    // ODT_STATIC: a message badge
                    if let Some(tone) = BADGES.with(|b| b.borrow().get(&(dis.CtlID as usize)).copied()) {
                        draw_badge(dis, tone);
                    }
                }
                _ => {}
            }
            LRESULT(1)
        }
        // DM_GETDEFID: tells IsDialogMessage which button Enter presses (owner-drawn buttons can't say so themselves).
        0x0400 => LRESULT(((0x534B_u32 << 16) | DEFAULT_BUTTON.with(|d| d.get()) as u32) as isize),
        WM_CLOSE => {
            DONE.with(|d| d.set(Some(false)));
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// The colour behind control `item`: the card it sits on, or the window background.
fn backdrop(form: HWND, item: HWND) -> modern::Rgb {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(item, &mut r);
        let mut p = windows::Win32::Foundation::POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(form, &mut p);
        let pal = modern::pal();
        let inside = SECTIONS.with(|s| s.borrow().iter().any(|(_, c)| p.x >= c.left && p.x < c.right && p.y >= c.top && p.y < c.bottom));
        if inside { pal.card } else { pal.bg }
    }
}

/// A filled circle with a white glyph, in the tone's colour.
fn draw_badge(dis: &DRAWITEMSTRUCT, tone: Tone) {
    let (colour, glyph) = match tone {
        Tone::Info => ((0x00, 0x67, 0xC0), "i"),
        Tone::Question => ((0x00, 0x67, 0xC0), "?"),
        Tone::Warning => ((0xC4, 0x7F, 0x00), "!"),
        Tone::Error => ((0xC4, 0x2B, 0x1C), "\u{00D7}"),
    };
    let (dc, r) = (dis.hDC, dis.rcItem);
    modern::fill(dc, &r, modern::pal().bg);
    modern::round_rect(dc, &r, r.right - r.left, colour, colour);
    unsafe {
        let old = SelectObject(dc, modern::semibold_font().into());
        modern::draw_text(dc, glyph, &r, (0xFF, 0xFF, 0xFF), DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, old);
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
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME;
        // Room below the button row, so the rounded buttons are not tight against the edge.
        let mut rc = RECT { left: 0, top: 0, right: spec.size.0, bottom: spec.size.1 + 8 };
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
        const BS_OWNERDRAW: u32 = 0xB;
        // Edit boxes and lists use the themed (client-edge) border, which Windows 11 draws rounded.
        let sunken = |h: HWND| {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_CLIENTEDGE.0 as isize);
            let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
        };
        SECTIONS.with(|s| s.borrow_mut().clear());
        BADGES.with(|b| b.borrow_mut().clear());
        TICKS.with(|t| t.borrow_mut().clear());
        let mut radio_group = 0usize;
        DEFAULT_BUTTON.with(|d| d.set(1));
        modern::frame(hwnd);
        for c in &spec.ctls {
            if let Kind::Group = c.kind {
                // A section is a rounded card with its title inside, drawn by the form itself.
                let (x, y, w, h) = c.rect;
                SECTIONS.with(|s| s.borrow_mut().push((c.text.clone(), RECT { left: x, top: y, right: x + w, bottom: y + h })));
                continue;
            }
            let h = match c.kind {
                Kind::Group => unreachable!(),
                Kind::Label => make(w!("STATIC"), &c.text, 0, c.id, c.rect),
                Kind::Badge(tone) => {
                    BADGES.with(|b| b.borrow_mut().insert(c.id, tone));
                    make(w!("STATIC"), "", 0xD, c.id, c.rect) // SS_OWNERDRAW
                }
                Kind::Edit => {
                    let h = make(w!("EDIT"), &c.text, TAB | 0x80, c.id, c.rect); // ES_AUTOHSCROLL
                    sunken(h);
                    modern::style_control(h, Style::Edit);
                    first_edit.get_or_insert(c.id);
                    h
                }
                Kind::Check | Kind::Radio(_) => {
                    let radio = matches!(c.kind, Kind::Radio(_));
                    if let Kind::Radio(true) = c.kind {
                        radio_group += 1;
                    }
                    let style = if let Kind::Radio(true) = c.kind { TAB | GROUP | BS_OWNERDRAW } else { TAB | BS_OWNERDRAW };
                    // The box and its gap take 28px; never let the label be cut off.
                    let (x, y, w, h) = c.rect;
                    let h_ctl = make(w!("BUTTON"), &c.text, style, c.id, (x, y, w.max(text_width(&c.text) + 32), h));
                    let h = h_ctl;
                    modern::track_hover(h);
                    TICKS.with(|t| t.borrow_mut().insert(c.id, Tick { radio, group: if radio { radio_group } else { 0 }, on: false }));
                    h
                }
                Kind::Button => {
                    let h = make(w!("BUTTON"), &c.text, TAB | BS_OWNERDRAW, c.id, c.rect);
                    modern::track_hover(h);
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
        let (bw, bh, by) = (88, 30, spec.size.1 - 36);
        if spec.ok.is_empty() {
            // Buttons are the form's own controls; keep a hidden Cancel so Esc still closes it.
            let cancel = make(w!("BUTTON"), "Cancel", 0, 2, (0, 0, 0, 0));
            let _ = ShowWindow(cancel, SW_HIDE);
            controls.insert(2, cancel);
        } else {
            let ok = make(w!("BUTTON"), spec.ok, TAB | GROUP | BS_OWNERDRAW, 1, (spec.size.0 - 2 * bw - 28, by, bw, bh));
            let cancel = make(w!("BUTTON"), "Cancel", TAB | BS_OWNERDRAW, 2, (spec.size.0 - bw - 16, by, bw, bh));
            modern::track_hover(ok);
            modern::track_hover(cancel);
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
