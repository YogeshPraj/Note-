//! Custom-painted window chrome: menu bar, icon toolbar, tab strip, status bar.
//!
//! These are plain Win32 child windows that paint themselves with GDI, because
//! the stock tab/status/menu controls cannot be themed to match Note++. They
//! hold only display state; every user action is forwarded to the host window
//! as `WM_COMMAND` (menus, toolbar) or `WM_BAR` (tabs, status bar) and handled
//! in `win32.rs`.

use std::cell::{Cell, RefCell};

use crate::core::workspace::TabInfo;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

pub const MENU_H: i32 = 26;
pub const TOOL_H: i32 = 34;
pub const TAB_H: i32 = 32;
pub const STATUS_H: i32 = 24;

/// Messages from tabs / status bar to the host: wParam = kind, lParam = argument.
pub const WM_BAR: u32 = WM_APP + 20;
pub const BAR_TAB_SELECT: usize = 1;
pub const BAR_TAB_CLOSE: usize = 2;
pub const BAR_TAB_NEW: usize = 3;
pub const BAR_STATUS: usize = 4;

pub const SEG_ENC: isize = 1;
pub const SEG_EOL: isize = 2;
pub const SEG_LANG: isize = 3;

type Rgb = (u8, u8, u8);
const TAB_DIRTY: Rgb = (0xC0, 0x00, 0x00);
const ACCENT: Rgb = (0x00, 0x7A, 0xCC);
const STATUS_BG: Rgb = (0x00, 0x7A, 0xCC);
const WHITE: Rgb = (0xFF, 0xFF, 0xFF);
const BLACK: Rgb = (0x00, 0x00, 0x00);

/// How the menu bar, toolbar and tab strip look. They sit on the window's Mica
/// backdrop (when the OS has it) and follow the editor theme.
#[derive(Clone, Copy)]
struct Look {
    mica: bool,
    dark: bool,
    editor_bg: Rgb,
    editor_fg: Rgb,
}

impl Look {
    fn fg(self) -> Rgb {
        if self.dark { (0xE6, 0xE6, 0xE6) } else { (0x1A, 0x1A, 0x1A) }
    }
    fn dim(self) -> Rgb {
        if self.dark { (0xB0, 0xB0, 0xB0) } else { (0x50, 0x50, 0x50) }
    }
    /// Colour of translucent hover / press / divider washes.
    fn ink(self) -> Rgb {
        if self.dark { WHITE } else { BLACK }
    }
    /// Opaque stand-in for the backdrop when Mica is unavailable.
    fn base(self) -> Rgb {
        if self.dark { (0x20, 0x20, 0x20) } else { (0xF3, 0xF3, 0xF3) }
    }
}

thread_local! {
    static LOOK: Cell<Look> = const {
        Cell::new(Look { mica: false, dark: false, editor_bg: WHITE, editor_fg: BLACK })
    };
}

fn look() -> Look {
    LOOK.with(Cell::get)
}

fn col(c: Rgb) -> COLORREF {
    COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}

fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

fn lx(l: LPARAM) -> i32 {
    (l.0 & 0xFFFF) as i16 as i32
}
fn ly(l: LPARAM) -> i32 {
    ((l.0 >> 16) & 0xFFFF) as i16 as i32
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn contains(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

// ---- GDI helpers -------------------------------------------------------

thread_local! {
    static FONT: Cell<Option<HFONT>> = const { Cell::new(None) };
}

fn ui_font() -> HFONT {
    FONT.with(|c| {
        if let Some(f) = c.get() {
            return f;
        }
        let f = unsafe {
            CreateFontW(
                -13, 0, 0, 0, 400, 0, 0, 0,
                FONT_CHARSET(1), FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), CLEARTYPE_QUALITY, 0,
                w!("Segoe UI"),
            )
        };
        c.set(Some(f));
        f
    })
}

pub fn ui_font_handle() -> HFONT {
    ui_font()
}

thread_local! {
    static MICA_FONT: Cell<Option<HFONT>> = const { Cell::new(None) };
}

/// `ui_font` with greyscale antialiasing. ClearType's per-channel coverage
/// can't be expressed as one alpha value, so text over Mica uses this.
fn mica_font() -> HFONT {
    MICA_FONT.with(|c| {
        if let Some(f) = c.get() {
            return f;
        }
        let f = unsafe {
            CreateFontW(
                -13, 0, 0, 0, 400, 0, 0, 0,
                FONT_CHARSET(1), FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), ANTIALIASED_QUALITY, 0,
                w!("Segoe UI"),
            )
        };
        c.set(Some(f));
        f
    })
}

unsafe fn fill(dc: HDC, r: &RECT, c: Rgb) {
    let b = unsafe { CreateSolidBrush(col(c)) };
    unsafe {
        FillRect(dc, r, b);
        let _ = DeleteObject(b.into());
    }
}

unsafe fn text(dc: HDC, s: &str, r: &RECT, c: Rgb, flags: DRAW_TEXT_FORMAT) {
    if s.is_empty() {
        return; // DrawTextW faults on an empty buffer
    }
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    let mut rr = *r;
    unsafe {
        SetTextColor(dc, col(c));
        DrawTextW(dc, &mut buf, &mut rr, flags | DT_NOPREFIX | DT_SINGLELINE | DT_VCENTER);
    }
}

unsafe fn text_width(dc: HDC, s: &str) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let buf: Vec<u16> = s.encode_utf16().collect();
    let mut sz = windows::Win32::Foundation::SIZE::default();
    unsafe {
        let _ = GetTextExtentPoint32W(dc, &buf, &mut sz);
    }
    sz.cx
}

/// Double-buffered paint: `f` draws into a memory DC of the client size.
unsafe fn paint(hwnd: HWND, f: impl FnOnce(HDC, RECT)) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right, rc.bottom);
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, bmp.into());
        let old_font = SelectObject(mem, ui_font().into());
        SetBkMode(mem, TRANSPARENT);
        f(mem, rc);
        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_font);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// A translucent fill composited *under* whatever a Mica bar draws.
struct Wash {
    r: RECT,
    c: Rgb,
    a: u8,
}

fn wash(r: RECT, c: Rgb, a: u8) -> Wash {
    Wash { r, c, a }
}

fn wash_outline(under: &mut Vec<Wash>, r: RECT, c: Rgb, a: u8) {
    under.push(wash(rect(r.left, r.top, r.right, r.top + 1), c, a));
    under.push(wash(rect(r.left, r.bottom - 1, r.right, r.bottom), c, a));
    under.push(wash(rect(r.left, r.top + 1, r.left + 1, r.bottom - 1), c, a));
    under.push(wash(rect(r.right - 1, r.top + 1, r.right, r.bottom - 1), c, a));
}

/// Paint a bar that sits on the Mica backdrop.
///
/// GDI can't write alpha, so `f` draws twice — over black, then over white —
/// and each pixel's coverage falls out of the difference: opaque pixels match,
/// untouched ones are 0 vs 255, antialiased edges land in between. The black
/// pass is then already the premultiplied colour. Washes `f` pushes go under
/// the result (first pushed is uppermost); without Mica, an opaque base colour
/// goes under everything.
unsafe fn paint_mica(hwnd: HWND, f: impl Fn(HDC, RECT, &mut Vec<Wash>)) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right, rc.bottom);
        if let (Some((black, kb)), Some((white, wb))) = (dib(w, h), dib(w, h)) {
            let mem = CreateCompatibleDC(Some(hdc));
            let old = SelectObject(mem, white.into());
            let old_font = SelectObject(mem, mica_font().into());
            SetBkMode(mem, TRANSPARENT);
            fill(mem, &rc, WHITE);
            f(mem, rc, &mut Vec::new());
            SelectObject(mem, black.into());
            fill(mem, &rc, BLACK);
            let mut under = Vec::new();
            f(mem, rc, &mut under);
            let _ = GdiFlush();

            let n = (w * h * 4) as usize;
            let px = std::slice::from_raw_parts_mut(kb, n);
            let over_white = std::slice::from_raw_parts(wb, n);
            for (p, q) in px.chunks_exact_mut(4).zip(over_white.chunks_exact(4)) {
                let spread: i32 = (0..3).map(|i| q[i] as i32 - p[i] as i32).sum();
                let a = (255 - spread / 3).clamp(0, 255) as u8;
                for c in &mut p[..3] {
                    *c = (*c).min(a);
                }
                p[3] = a;
            }
            let lk = look();
            if !lk.mica {
                under.push(wash(rc, lk.base(), 255));
            }
            for s in &under {
                composite_under(px, w, h, s);
            }

            let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old_font);
            SelectObject(mem, old);
            let _ = DeleteDC(mem);
            let _ = DeleteObject(black.into());
            let _ = DeleteObject(white.into());
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

/// Blend a wash beneath the premultiplied BGRA pixels already in `px`.
fn composite_under(px: &mut [u8], w: i32, h: i32, s: &Wash) {
    let a = s.a as u32;
    let c = [s.c.2, s.c.1, s.c.0].map(|v| v as u32 * a / 255);
    for y in s.r.top.max(0)..s.r.bottom.min(h) {
        for x in s.r.left.max(0)..s.r.right.min(w) {
            let p = &mut px[((y * w + x) * 4) as usize..][..4];
            let room = 255 - p[3] as u32;
            for i in 0..3 {
                p[i] = (p[i] as u32 + c[i] * room / 255) as u8;
            }
            p[3] = (p[3] as u32 + a * room / 255) as u8;
        }
    }
}

fn track_leave(hwnd: HWND) {
    let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
    unsafe {
        let _ = TrackMouseEvent(&mut t);
    }
}

fn host_of(hwnd: HWND) -> HWND {
    unsafe { GetParent(hwnd).unwrap_or_default() }
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn register(name: PCWSTR, proc: WNDPROC) {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), name, &mut wc).is_ok() {
            return;
        }
        RegisterClassW(&WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW, // repaint the whole bar on resize, not just the new strip
            lpfnWndProc: proc,
            hInstance: hinst.into(),
            lpszClassName: name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            ..Default::default()
        });
    }
}

fn make(host: HWND, class: PCWSTR) -> HWND {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            10,
            10,
            Some(host),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap()
    }
}

/// A top-down 32-bit DIB section and a pointer to its BGRA pixels.
fn dib(w: i32, h: i32) -> Option<(HBITMAP, *mut u8)> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    Some((bmp, bits as *mut u8))
}

// ---- icons -------------------------------------------------------------

/// Rasterise an SVG to a premultiplied-alpha 32-bit DIB.
fn load_icon(svg: &[u8], px: i32) -> Option<HBITMAP> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pm = resvg::tiny_skia::Pixmap::new(px as u32, px as u32)?;
    let size = tree.size();
    let t = resvg::tiny_skia::Transform::from_scale(px as f32 / size.width(), px as f32 / size.height());
    resvg::render(&tree, t, &mut pm.as_mut());

    let (bmp, bits) = dib(px, px)?;
    unsafe {
        let dst = std::slice::from_raw_parts_mut(bits, (px * px * 4) as usize);
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]); // RGBA -> BGRA
        }
        Some(bmp)
    }
}

unsafe fn draw_icon(dc: HDC, bmp: HBITMAP, x: i32, y: i32, px: i32) {
    unsafe {
        let mem = CreateCompatibleDC(Some(dc));
        let old = SelectObject(mem, bmp.into());
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        let _ = AlphaBlend(dc, x, y, px, px, mem, 0, 0, px, px, blend);
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
    }
}

// ---- menu bar ------------------------------------------------------------

struct MenuState {
    items: Vec<(String, HMENU)>,
    rects: Vec<RECT>,
    hot: Option<usize>,
}

thread_local! {
    static MENU: RefCell<MenuState> = const { RefCell::new(MenuState { items: Vec::new(), rects: Vec::new(), hot: None }) };
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe {
                paint_mica(hwnd, |dc, rc, under| {
                    let lk = look();
                    MENU.with(|m| {
                        let mut m = m.borrow_mut();
                        let mut x = 6;
                        let mut rects = Vec::new();
                        for (i, (label, _)) in m.items.iter().enumerate() {
                            let wd = text_width(dc, label) + 20;
                            let r = rect(x, 2, x + wd, rc.bottom - 2);
                            if m.hot == Some(i) {
                                under.push(wash(r, lk.ink(), 0x1C));
                            }
                            text(dc, label, &r, lk.fg(), DT_CENTER);
                            rects.push(r);
                            x += wd;
                        }
                        m.rects = rects;
                    });
                });
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = (lx(lp), ly(lp));
            let changed = MENU.with(|m| {
                let mut m = m.borrow_mut();
                let hot = m.rects.iter().position(|r| contains(r, x, y));
                let c = hot != m.hot;
                m.hot = hot;
                c
            });
            track_leave(hwnd);
            if changed {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        0x02A3 => {
            // WM_MOUSELEAVE
            MENU.with(|m| m.borrow_mut().hot = None);
            redraw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = (lx(lp), ly(lp));
            let hit = MENU.with(|m| {
                let m = m.borrow();
                m.rects.iter().position(|r| contains(r, x, y)).map(|i| (m.items[i].1, m.rects[i]))
            });
            if let Some((menu, r)) = hit {
                let mut pt = POINT { x: r.left, y: r.bottom };
                unsafe {
                    let _ = ClientToScreen(hwnd, &mut pt);
                    let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, Some(0), host_of(hwnd), None);
                }
                redraw(hwnd);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---- toolbar -----------------------------------------------------------

pub enum ToolItem {
    Btn { cmd: usize, svg: &'static [u8], tip: &'static str },
    Sep,
}

struct ToolState {
    items: Vec<(Option<(usize, HBITMAP)>, RECT)>,
    hot: Option<usize>,
    down: Option<usize>,
    checked: Vec<usize>,
}

thread_local! {
    static TOOLS: RefCell<ToolState> = const { RefCell::new(ToolState { items: Vec::new(), hot: None, down: None, checked: Vec::new() }) };
}

const ICON: i32 = 18;
const BTN: i32 = 28;

unsafe extern "system" fn tool_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe {
                paint_mica(hwnd, |dc, _rc, under| {
                    let lk = look();
                    TOOLS.with(|t| {
                        let t = t.borrow();
                        for (i, (item, r)) in t.items.iter().enumerate() {
                            match item {
                                None => under.push(wash(rect(r.left + 3, r.top + 6, r.left + 4, r.bottom - 6), lk.ink(), 0x40)),
                                Some((cmd, bmp)) => {
                                    if t.down == Some(i) {
                                        under.push(wash(*r, lk.ink(), 0x30));
                                    } else if t.hot == Some(i) {
                                        under.push(wash(*r, lk.ink(), 0x18));
                                    }
                                    if t.checked.contains(cmd) {
                                        wash_outline(under, *r, ACCENT, 0xB0);
                                        under.push(wash(*r, ACCENT, 0x40));
                                    }
                                    draw_icon(dc, *bmp, r.left + (BTN - ICON) / 2, r.top + (BTN - ICON) / 2, ICON);
                                }
                            }
                        }
                    });
                });
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = (lx(lp), ly(lp));
            let changed = TOOLS.with(|t| {
                let mut t = t.borrow_mut();
                let hot = t.items.iter().position(|(it, r)| it.is_some() && contains(r, x, y));
                let c = hot != t.hot;
                t.hot = hot;
                c
            });
            track_leave(hwnd);
            if changed {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        0x02A3 => {
            TOOLS.with(|t| {
                let mut t = t.borrow_mut();
                t.hot = None;
                t.down = None;
            });
            redraw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            TOOLS.with(|t| {
                let mut t = t.borrow_mut();
                t.down = t.hot;
            });
            redraw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let cmd = TOOLS.with(|t| {
                let mut t = t.borrow_mut();
                let hit = t.down.filter(|d| t.hot == Some(*d)).and_then(|d| t.items[d].0.as_ref().map(|i| i.0));
                t.down = None;
                hit
            });
            redraw(hwnd);
            if let Some(cmd) = cmd {
                unsafe {
                    let _ = PostMessageW(Some(host_of(hwnd)), WM_COMMAND, WPARAM(cmd), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn build_toolbar(hwnd: HWND, items: &[ToolItem]) {
    let mut x = 6;
    let mut list = Vec::new();
    let mut tips = Vec::new();
    for it in items {
        match it {
            ToolItem::Sep => {
                list.push((None, rect(x, 3, x + 8, 3 + BTN)));
                x += 8;
            }
            ToolItem::Btn { cmd, svg, tip } => {
                if let Some(bmp) = load_icon(svg, ICON) {
                    let r = rect(x, 3, x + BTN, 3 + BTN);
                    tips.push((list.len(), r, *tip));
                    list.push((Some((*cmd, bmp)), r));
                    x += BTN + 1;
                }
            }
        }
    }
    TOOLS.with(|t| t.borrow_mut().items = list);

    // Native tooltips relay the toolbar's mouse messages via TTF_SUBCLASS.
    unsafe {
        let tt = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("tooltips_class32"),
            PCWSTR::null(),
            WS_POPUP | WINDOW_STYLE(0x01 | 0x02), // TTS_ALWAYSTIP | TTS_NOPREFIX
            0,
            0,
            0,
            0,
            Some(hwnd),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        );
        let Ok(tt) = tt else { return };
        for (i, r, tip) in tips {
            let mut text = wide(tip);
            let ti = TTTOOLINFOW {
                cbSize: size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_SUBCLASS,
                hwnd,
                uId: i,
                rect: r,
                lpszText: windows::core::PWSTR(text.as_mut_ptr()),
                ..Default::default()
            };
            SendMessageW(tt, TTM_ADDTOOLW, None, Some(LPARAM(&ti as *const _ as isize)));
        }
    }
}

// ---- tab strip -------------------------------------------------------------

struct TabState {
    tabs: Vec<TabInfo>,
    active: usize,
    /// (index, hovering its close button)
    hot: Option<(usize, bool)>,
    offset: i32,
    layout: Vec<RECT>,
    plus: RECT,
    /// Scroll arrows, shown only while the tabs overflow; `area` is where tabs may show.
    arrows: Option<(RECT, RECT)>,
    area: i32,
    /// Bring the active tab into view on the next paint (not while the user scrolls by hand).
    follow: bool,
}

thread_local! {
    static TABS: RefCell<TabState> = const { RefCell::new(TabState { tabs: Vec::new(), active: 0, hot: None, offset: 0, layout: Vec::new(), plus: RECT { left: 0, top: 0, right: 0, bottom: 0 }, arrows: None, area: 0, follow: true }) };
}

const ARROW_W: i32 = 24;

fn close_rect(r: &RECT) -> RECT {
    rect(r.right - 24, r.top + 7, r.right - 8, r.bottom - 7)
}

/// Recompute tab rectangles and keep the active tab scrolled into view.
unsafe fn layout_tabs(dc: HDC, width: i32, s: &mut TabState) {
    let mut x = 4;
    s.layout.clear();
    for info in &s.tabs {
        let extra = if info.badge.is_some() { 30 } else { 0 };
        let wd = (unsafe { text_width(dc, &info.title) } + 56 + extra).clamp(96, 260);
        s.layout.push(rect(x, 4, x + wd, TAB_H));
        x += wd + 2;
    }
    let total = x;
    let overflow = total > width - 34;
    let avail = if overflow { width - 34 - 2 * ARROW_W } else { width - 34 };
    let pad = if overflow { ARROW_W + 2 } else { 0 };
    s.area = pad + avail + 4;
    if !overflow {
        s.offset = 0;
        s.arrows = None;
    } else {
        if s.follow {
            if let Some(a) = s.layout.get(s.active) {
                if a.right - s.offset > avail {
                    s.offset = a.right - avail;
                }
                if a.left - s.offset < 0 {
                    s.offset = a.left;
                }
            }
        }
        s.offset = s.offset.clamp(0, (total - avail).max(0));
        s.follow = false;
        s.arrows = Some((rect(2, 6, 2 + ARROW_W, TAB_H - 2), rect(width - ARROW_W - 2, 6, width - 2, TAB_H - 2)));
    }
    let shift = pad - s.offset;
    for r in &mut s.layout {
        r.left += shift;
        r.right += shift;
    }
    let plus_x = pad + (total - s.offset).min(avail) + 4;
    s.plus = rect(plus_x, 6, plus_x + 24, TAB_H - 2);
}

unsafe extern "system" fn tab_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe {
                paint_mica(hwnd, |dc, rc, under| {
                    let lk = look();
                    TABS.with(|s| {
                        let mut s = s.borrow_mut();
                        layout_tabs(dc, rc.right, &mut s);
                        under.push(wash(rect(0, rc.bottom - 1, rc.right, rc.bottom), lk.ink(), 0x30));
                        let (clip_left, clip_right) = if let Some((l, _)) = s.arrows { (l.right, s.area) } else { (0, rc.right) };
                        let saved = SaveDC(dc);
                        IntersectClipRect(dc, clip_left, 0, clip_right, rc.bottom);
                        for (i, info) in s.tabs.iter().enumerate() {
                            let (name, dirty) = (&info.title, &info.dirty);
                            let r = s.layout[i];
                            let active = i == s.active;
                            let hot = s.hot.map(|h| h.0) == Some(i);
                            let fg = if active { lk.editor_fg } else { lk.dim() };
                            if active {
                                // Opaque, in the editor's own colour, so it runs into the editor below.
                                fill(dc, &rect(r.left, r.top, r.right, rc.bottom), lk.editor_bg);
                                fill(dc, &rect(r.left, r.top, r.right, r.top + 2), ACCENT);
                            } else if hot {
                                under.push(wash(rect(r.left, r.top, r.right, r.bottom - 1), lk.ink(), 0x18));
                            }
                            if !active && !hot && s.active != i + 1 && s.hot.map(|h| h.0) != Some(i + 1) {
                                under.push(wash(rect(r.right, r.top + 8, r.right + 1, r.bottom - 8), lk.ink(), 0x30));
                            }
                            let mut tx = r.left + 10;
                            if *dirty {
                                let dot = rect(tx, r.top + 12, tx + 8, r.top + 20);
                                let br = CreateSolidBrush(col(TAB_DIRTY));
                                let pen = CreatePen(PS_SOLID, 1, col(TAB_DIRTY));
                                let (ob, op) = (SelectObject(dc, br.into()), SelectObject(dc, pen.into()));
                                let _ = Ellipse(dc, dot.left, dot.top, dot.right, dot.bottom);
                                SelectObject(dc, ob);
                                SelectObject(dc, op);
                                let _ = DeleteObject(br.into());
                                let _ = DeleteObject(pen.into());
                                tx += 14;
                            }
                            if let Some(badge) = info.badge {
                                // Small blue pill (e.g. "wb" for whiteboards).
                                let pill = rect(tx, r.top + 8, tx + 24, r.top + 24);
                                let br = CreateSolidBrush(col((0x4A, 0x7A, 0xFF)));
                                let pen = CreatePen(PS_SOLID, 1, col((0x4A, 0x7A, 0xFF)));
                                let (ob, op) = (SelectObject(dc, br.into()), SelectObject(dc, pen.into()));
                                let _ = RoundRect(dc, pill.left, pill.top, pill.right, pill.bottom, 8, 8);
                                SelectObject(dc, ob);
                                SelectObject(dc, op);
                                let _ = DeleteObject(br.into());
                                let _ = DeleteObject(pen.into());
                                text(dc, badge, &pill, WHITE, DT_CENTER);
                                tx += 30;
                            }
                            text(dc, name, &rect(tx, r.top, r.right - 26, r.bottom), fg, DT_LEFT | DT_END_ELLIPSIS);
                            if active || hot {
                                let c = close_rect(&r);
                                if s.hot == Some((i, true)) {
                                    if active {
                                        // Washes go under opaque pixels, so draw this one solid.
                                        fill(dc, &c, if lk.dark { (0x3A, 0x3A, 0x3A) } else { (0xE0, 0xE0, 0xE0) });
                                    } else {
                                        under.push(wash(c, lk.ink(), 0x28));
                                    }
                                }
                                let pen = CreatePen(PS_SOLID, 1, col(fg));
                                let op = SelectObject(dc, pen.into());
                                let (cx, cy) = ((c.left + c.right) / 2, (c.top + c.bottom) / 2);
                                let _ = MoveToEx(dc, cx - 3, cy - 3, None);
                                let _ = LineTo(dc, cx + 4, cy + 4);
                                let _ = MoveToEx(dc, cx + 3, cy - 3, None);
                                let _ = LineTo(dc, cx - 4, cy + 4);
                                SelectObject(dc, op);
                                let _ = DeleteObject(pen.into());
                            }
                        }
                        RestoreDC(dc, saved);
                        let p = s.plus;
                        text(dc, "+", &p, lk.fg(), DT_CENTER);
                        if let Some((l, r)) = s.arrows {
                            let can_l = s.offset > 0;
                            let can_r = s.layout.last().is_some_and(|t| t.right > s.area);
                            for b in [l, r] {
                                let pen = CreatePen(PS_SOLID, 1, col(lk.dim()));
                                let (op, ob) = (SelectObject(dc, pen.into()), SelectObject(dc, GetStockObject(NULL_BRUSH)));
                                let _ = Rectangle(dc, b.left, b.top, b.right, b.bottom);
                                SelectObject(dc, op);
                                SelectObject(dc, ob);
                                let _ = DeleteObject(pen.into());
                            }
                            text(dc, "\u{25C0}", &l, if can_l { lk.fg() } else { lk.dim() }, DT_CENTER);
                            text(dc, "\u{25B6}", &r, if can_r { lk.fg() } else { lk.dim() }, DT_CENTER);
                        }
                    });
                });
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = (lx(lp), ly(lp));
            let changed = TABS.with(|s| {
                let mut s = s.borrow_mut();
                let hot = s.layout.iter().position(|r| contains(r, x, y) && s.arrows.is_none_or(|(l, _)| x >= l.right && x < s.area)).map(|i| (i, contains(&close_rect(&s.layout[i]), x, y)));
                let c = hot != s.hot;
                s.hot = hot;
                c
            });
            track_leave(hwnd);
            if changed {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        0x02A3 => {
            TABS.with(|s| s.borrow_mut().hot = None);
            redraw(hwnd);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
            TABS.with(|s| s.borrow_mut().offset -= delta / 2);
            redraw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_MBUTTONUP | WM_LBUTTONDBLCLK => {
            let (x, y) = (lx(lp), ly(lp));
            let host = host_of(hwnd);
            let (hit, plus) = TABS.with(|s| {
                let s = s.borrow();
                (s.layout.iter().position(|r| contains(r, x, y) && s.arrows.is_none_or(|(l, _)| x >= l.right && x < s.area)).map(|i| (i, contains(&close_rect(&s.layout[i]), x, y) && (i == s.active || s.hot.map(|h| h.0) == Some(i)))), contains(&s.plus, x, y))
            });
            let post = |kind: usize, arg: usize| unsafe {
                let _ = PostMessageW(Some(host), WM_BAR, WPARAM(kind), LPARAM(arg as isize));
            };
            let arrow = msg == WM_LBUTTONDOWN
                && TABS.with(|s| {
                    let mut s = s.borrow_mut();
                    match s.arrows {
                        Some((l, _)) if contains(&l, x, y) => {
                            s.offset -= 150;
                            true
                        }
                        Some((_, r)) if contains(&r, x, y) => {
                            s.offset += 150;
                            true
                        }
                        _ => false,
                    }
                });
            if arrow {
                redraw(hwnd);
                return LRESULT(0);
            }
            match (msg, hit) {
                (WM_MBUTTONUP, Some((i, _))) => post(BAR_TAB_CLOSE, i),
                (WM_LBUTTONDOWN, Some((i, true))) => post(BAR_TAB_CLOSE, i),
                (WM_LBUTTONDOWN, Some((i, false))) => post(BAR_TAB_SELECT, i),
                (WM_LBUTTONDBLCLK, None) | (WM_LBUTTONDOWN, None) if plus => post(BAR_TAB_NEW, 0),
                (WM_LBUTTONDBLCLK, None) => post(BAR_TAB_NEW, 0),
                _ => {}
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---- status bar ---------------------------------------------------------

#[derive(Default, Clone)]
pub struct StatusText {
    pub position: String,
    pub lines: String,
    pub length: String,
    pub enc: String,
    pub eol: String,
    pub lang: String,
    pub ins: String,
}

#[derive(Default)]
struct StatusState {
    text: StatusText,
    /// Clickable segments: (rect, segment id).
    hits: Vec<(RECT, isize)>,
}

thread_local! {
    static STATUS: RefCell<StatusState> = RefCell::new(StatusState::default());
}

unsafe extern "system" fn status_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe {
                paint(hwnd, |dc, rc| {
                    fill(dc, &rc, STATUS_BG);
                    STATUS.with(|s| {
                        let mut s = s.borrow_mut();
                        let t = s.text.clone();
                        text(dc, &t.position, &rect(10, 0, rc.right / 3, rc.bottom), WHITE, DT_LEFT);
                        let mut hits = Vec::new();
                        let mut x = rc.right - 8;
                        let segs: [(&str, isize); 6] =
                            [(&t.ins, 0), (&t.lang, SEG_LANG), (&t.eol, SEG_EOL), (&t.enc, SEG_ENC), (&t.length, 0), (&t.lines, 0)];
                        for (label, id) in segs {
                            let wd = text_width(dc, label) + 22;
                            let r = rect(x - wd, 0, x, rc.bottom);
                            text(dc, label, &r, WHITE, DT_CENTER);
                            fill(dc, &rect(r.left, 5, r.left + 1, rc.bottom - 5), (0x4D, 0xA3, 0xDA));
                            if id != 0 {
                                hits.push((r, id));
                            }
                            x -= wd;
                        }
                        s.hits = hits;
                    });
                });
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let mut pt = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
                let _ = ScreenToClient(hwnd, &mut pt);
            }
            let over = STATUS.with(|s| s.borrow().hits.iter().any(|(r, _)| contains(r, pt.x, pt.y)));
            unsafe {
                SetCursor(LoadCursorW(None, if over { IDC_HAND } else { IDC_ARROW }).ok());
            }
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = (lx(lp), ly(lp));
            let seg = STATUS.with(|s| s.borrow().hits.iter().find(|(r, _)| contains(r, x, y)).map(|h| h.1));
            if let Some(id) = seg {
                unsafe {
                    let _ = PostMessageW(Some(host_of(hwnd)), WM_BAR, WPARAM(BAR_STATUS), LPARAM(id));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---- public API ------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Chrome {
    pub menu: HWND,
    pub tools: HWND,
    pub tabs: HWND,
    pub status: HWND,
}

pub fn create(host: HWND, menus: Vec<(String, HMENU)>, tools: &[ToolItem]) -> Chrome {
    register(w!("NoteppMenuBar"), Some(menu_proc));
    register(w!("NoteppToolBar"), Some(tool_proc));
    register(w!("NoteppTabs"), Some(tab_proc));
    register(w!("NoteppStatus"), Some(status_proc));
    MENU.with(|m| m.borrow_mut().items = menus);

    // Mica behind the title bar, and — through the extended frame — behind the
    // menu bar, toolbar and tab strip, which paint with alpha (`paint_mica`).
    let mica = super::dwm::apply(host, look().dark);
    if mica {
        super::dwm::extend_top(host, MENU_H + TOOL_H + TAB_H);
    }
    LOOK.with(|l| l.set(Look { mica, ..l.get() }));

    let c = Chrome {
        menu: make(host, w!("NoteppMenuBar")),
        tools: make(host, w!("NoteppToolBar")),
        tabs: make(host, w!("NoteppTabs")),
        status: make(host, w!("NoteppStatus")),
    };
    build_toolbar(c.tools, tools);
    c
}

pub fn set_tabs(c: &Chrome, tabs: Vec<TabInfo>, active: usize) {
    TABS.with(|s| {
        let mut s = s.borrow_mut();
        s.follow = s.follow || s.active != active || s.tabs.len() != tabs.len();
        s.tabs = tabs;
        s.active = active;
    });
    redraw(c.tabs);
}

/// Follow the editor theme: the Mica tint (via the window's dark-mode flag),
/// the bar text, and the active tab, which continues the editor's background.
pub fn set_theme(c: &Chrome, dark: bool, editor_bg: (u8, u8, u8), editor_fg: (u8, u8, u8)) {
    super::modern::set_dark(dark);
    let lk = look();
    if lk.dark == dark && lk.editor_bg == editor_bg && lk.editor_fg == editor_fg {
        return;
    }
    LOOK.with(|l| l.set(Look { dark, editor_bg, editor_fg, ..lk }));
    if lk.dark != dark {
        super::dwm::set_dark(host_of(c.menu), dark);
    }
    for bar in [c.menu, c.tools, c.tabs] {
        redraw(bar);
    }
}

pub fn set_status(c: &Chrome, t: StatusText) {
    STATUS.with(|s| s.borrow_mut().text = t);
    redraw(c.status);
}

pub fn set_checked(c: &Chrome, cmd: usize, on: bool) {
    TOOLS.with(|t| {
        let mut t = t.borrow_mut();
        t.checked.retain(|x| *x != cmd);
        if on {
            t.checked.push(cmd);
        }
    });
    redraw(c.tools);
}
