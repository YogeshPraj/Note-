//! Printing: Scintilla renders the document onto a printer device context
//! itself (`SCI_FORMATRANGEFULL`), so syntax colours, wrapping and fonts match
//! the editor. This file only gets a device context and walks the pages.

use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows::Win32::UI::Controls::Dialogs::*;
use windows::core::PCWSTR;
use std::cell::RefCell;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

use crate::core::sci::{self, Send};

const SCI_FORMATRANGEFULL: u32 = 2777;
const SCI_SETPRINTCOLOURMODE: u32 = 2148;
/// Keep syntax colours but print them on a white page, whatever the editor theme.
const SC_PRINT_COLOURONWHITE: usize = 3;

/// `Sci_RangeToFormatFull` from Scintilla.h (64-bit layout).
#[repr(C)]
struct RangeToFormatFull {
    hdc: isize,
    hdc_target: isize,
    rc: [i32; 4],      // left, top, right, bottom: where text may go
    rc_page: [i32; 4], // the whole page
    cp_min: isize,
    cp_max: isize,
}

/// Page margins in thousandths of an inch.
#[derive(Clone, Copy, PartialEq)]
pub struct Margins {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

struct Setup {
    margins: Margins,
    devmode: HGLOBAL,
    devnames: HGLOBAL,
}

thread_local! {
    static SETUP: RefCell<Setup> = const { RefCell::new(Setup { margins: Margins { l: 750, t: 750, r: 750, b: 750 }, devmode: HGLOBAL(std::ptr::null_mut()), devnames: HGLOBAL(std::ptr::null_mut()) }) };
}

pub fn margins() -> Margins {
    SETUP.with(|s| s.borrow().margins)
}

/// Where text may go on a page of `dc`, in device units (printable-area coordinates).
pub fn text_rect(dc: HDC, m: Margins) -> [i32; 4] {
    let caps = |c: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(dc), c) };
    let (w, h) = (caps(HORZRES), caps(VERTRES));
    let (dpix, dpiy) = (caps(LOGPIXELSX), caps(LOGPIXELSY));
    let (offx, offy) = (caps(PHYSICALOFFSETX), caps(PHYSICALOFFSETY));
    let (physw, physh) = (caps(PHYSICALWIDTH), caps(PHYSICALHEIGHT));
    // Margins are measured from the paper edge, not from the printable area.
    let left = (dpix * m.l / 1000 - offx).max(0);
    let top = (dpiy * m.t / 1000 - offy).max(0);
    let right = (dpix * m.r / 1000 - (physw - w - offx)).max(0);
    let bottom = (dpiy * m.b / 1000 - (physh - h - offy)).max(0);
    [left, top, (w - right).max(left + 1), (h - bottom).max(top + 1)]
}

/// Show the Page Setup dialog (paper, orientation, margins). True if the user accepted.
pub fn page_setup(owner: HWND) -> bool {
    let (m, dm, dn) = SETUP.with(|s| {
        let s = s.borrow();
        (s.margins, s.devmode, s.devnames)
    });
    let mut psd = PAGESETUPDLGW {
        lStructSize: size_of::<PAGESETUPDLGW>() as u32,
        hwndOwner: owner,
        hDevMode: dm,
        hDevNames: dn,
        Flags: PSD_MARGINS | PSD_INTHOUSANDTHSOFINCHES,
        rtMargin: RECT { left: m.l, top: m.t, right: m.r, bottom: m.b },
        ..Default::default()
    };
    let ok = unsafe { PageSetupDlgW(&mut psd) }.as_bool();
    SETUP.with(|s| {
        let mut s = s.borrow_mut();
        if !psd.hDevMode.is_invalid() {
            s.devmode = psd.hDevMode;
        }
        if !psd.hDevNames.is_invalid() {
            s.devnames = psd.hDevNames;
        }
        if ok {
            let r = psd.rtMargin;
            s.margins = Margins { l: r.left.max(0), t: r.top.max(0), r: r.right.max(0), b: r.bottom.max(0) };
        }
    });
    ok
}

/// An information context for the chosen (else default) printer, used to measure pages.
pub fn printer_ic() -> Option<HDC> {
    let (dm, dn) = SETUP.with(|s| {
        let s = s.borrow();
        (s.devmode, s.devnames)
    });
    unsafe {
        let mut name: Vec<u16> = Vec::new();
        if !dn.is_invalid() {
            let p = GlobalLock(dn) as *const u8;
            if !p.is_null() {
                let names = &*(p as *const DEVNAMES);
                let dev = p.add(names.wDeviceOffset as usize * 2) as *const u16;
                let mut i = 0;
                while *dev.add(i) != 0 {
                    name.push(*dev.add(i));
                    i += 1;
                }
                let _ = GlobalUnlock(dn);
            }
        }
        if name.is_empty() {
            let mut buf = [0u16; 260];
            let mut n = buf.len() as u32;
            if !GetDefaultPrinterW(Some(windows::core::PWSTR(buf.as_mut_ptr())), &mut n).as_bool() {
                return None;
            }
            name = buf[..buf.iter().position(|c| *c == 0).unwrap_or(0)].to_vec();
        }
        name.push(0);
        let devmode = if dm.is_invalid() { std::ptr::null() } else { GlobalLock(dm) as *const DEVMODEW };
        let ic = CreateICW(windows::core::w!("WINSPOOL"), PCWSTR(name.as_ptr()), PCWSTR::null(), if devmode.is_null() { None } else { Some(devmode) });
        if !dm.is_invalid() {
            let _ = GlobalUnlock(dm);
        }
        (!ic.is_invalid()).then_some(ic)
    }
}

/// Start position of every page when the document is laid out on the paper of `ic`.
pub fn paginate<S: Send>(sci_view: &S, ic: HDC) -> Vec<isize> {
    let len = sci::length(sci_view).max(0) as isize;
    let mut starts = vec![0isize];
    if len == 0 {
        return starts;
    }
    let caps = |c: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(ic), c) };
    let (w, h) = (caps(HORZRES), caps(VERTRES));
    sci_view.send(SCI_SETPRINTCOLOURMODE, SC_PRINT_COLOURONWHITE, 0);
    let mut fr = RangeToFormatFull { hdc: ic.0 as isize, hdc_target: ic.0 as isize, rc: text_rect(ic, margins()), rc_page: [0, 0, w, h], cp_min: 0, cp_max: len };
    let mut pos = 0isize;
    while pos < len && starts.len() < 5000 {
        fr.cp_min = pos;
        let next = sci_view.send(SCI_FORMATRANGEFULL, 0, &mut fr as *mut _ as isize);
        if next <= pos || next >= len {
            break;
        }
        starts.push(next);
        pos = next;
    }
    sci_view.send(SCI_FORMATRANGEFULL, 0, 0);
    starts
}

/// Draw the page that starts at `start` onto `dc`, whose logical units are printer pixels of `ic`.
pub fn render_page<S: Send>(sci_view: &S, dc: HDC, ic: HDC, start: isize) {
    let len = sci::length(sci_view).max(0) as isize;
    let caps = |c: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(ic), c) };
    let (w, h) = (caps(HORZRES), caps(VERTRES));
    sci_view.send(SCI_SETPRINTCOLOURMODE, SC_PRINT_COLOURONWHITE, 0);
    let mut fr = RangeToFormatFull { hdc: dc.0 as isize, hdc_target: ic.0 as isize, rc: text_rect(ic, margins()), rc_page: [0, 0, w, h], cp_min: start, cp_max: len };
    sci_view.send(SCI_FORMATRANGEFULL, 1, &mut fr as *mut _ as isize);
    sci_view.send(SCI_FORMATRANGEFULL, 0, 0);
}

/// Keep the printer choice the Print dialog returned for next time.
fn remember(pd: &PRINTDLGW) {
    SETUP.with(|s| {
        let mut s = s.borrow_mut();
        if !pd.hDevMode.is_invalid() {
            s.devmode = pd.hDevMode;
        }
        if !pd.hDevNames.is_invalid() {
            s.devnames = pd.hDevNames;
        }
    });
}

pub enum Outcome {
    Printed { pages: usize },
    /// The user closed the print dialog.
    Cancelled,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Print the whole document to `dc`. `output` writes to a file instead of the
/// printer's port (used with "Microsoft Print to PDF" for testing).
pub fn print_to_dc<S: Send>(sci_view: &S, dc: HDC, title: &str, output: Option<&str>) -> Result<usize, String> {
    let len = sci::length(sci_view).max(0) as isize;
    if len == 0 {
        return Err("There is nothing to print.".into());
    }
    let caps = |c: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(dc), c) };
    let (w, h) = (caps(HORZRES), caps(VERTRES));
    let rc = text_rect(dc, margins());


    let name = wide(title);
    let out = output.map(wide);
    let info = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(name.as_ptr()),
        lpszOutput: out.as_ref().map(|o| PCWSTR(o.as_ptr())).unwrap_or(PCWSTR::null()),
        ..Default::default()
    };
    if unsafe { StartDocW(dc, &info) } <= 0 {
        return Err("The printer could not start the document.".into());
    }

    sci_view.send(SCI_SETPRINTCOLOURMODE, SC_PRINT_COLOURONWHITE, 0);
    let mut fr = RangeToFormatFull {
        hdc: dc.0 as isize,
        hdc_target: dc.0 as isize,
        rc,
        rc_page: [0, 0, w, h],
        cp_min: 0,
        cp_max: len,
    };
    let (mut pos, mut pages) = (0isize, 0usize);
    while pos < len {
        unsafe {
            StartPage(dc);
        }
        fr.cp_min = pos;
        let next = sci_view.send(SCI_FORMATRANGEFULL, 1, &mut fr as *mut _ as isize);
        unsafe {
            EndPage(dc);
        }
        pages += 1;
        if next <= pos {
            break; // no progress: avoid printing forever
        }
        pos = next;
    }
    sci_view.send(SCI_FORMATRANGEFULL, 0, 0); // release Scintilla's layout cache
    unsafe {
        EndDoc(dc);
    }
    Ok(pages)
}

fn free(h: HGLOBAL) {
    if !h.is_invalid() {
        unsafe {
            let _ = GlobalFree(Some(h));
        }
    }
}

/// Print the document. With `dialog` the standard Print dialog is shown;
/// otherwise ("Print Now") the default printer is used directly.
pub fn print<S: Send>(owner: HWND, sci_view: &S, title: &str, dialog: bool) -> Result<Outcome, String> {
    let mut pd = PRINTDLGW { lStructSize: size_of::<PRINTDLGW>() as u32, hwndOwner: owner, ..Default::default() };
    pd.Flags = if dialog {
        PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS | PD_USEDEVMODECOPIESANDCOLLATE
    } else {
        PD_RETURNDC | PD_RETURNDEFAULT
    };
    if dialog {
        SETUP.with(|s| {
            let s = s.borrow();
            pd.hDevMode = s.devmode;
            pd.hDevNames = s.devnames;
        });
    }
    pd.nCopies = 1;
    if !unsafe { PrintDlgW(&mut pd) }.as_bool() {
        if dialog {
            remember(&pd);
        } else {
            free(pd.hDevMode);
            free(pd.hDevNames);
        }
        return if dialog { Ok(Outcome::Cancelled) } else { Err("No default printer is set up.".into()) };
    }
    let dc = pd.hDC;
    let result = print_to_dc(sci_view, dc, title, None);
    unsafe {
        let _ = DeleteDC(dc);
    }
    if dialog {
        remember(&pd);
    } else {
        free(pd.hDevMode);
        free(pd.hDevNames);
    }
    result.map(|pages| Outcome::Printed { pages })
}
