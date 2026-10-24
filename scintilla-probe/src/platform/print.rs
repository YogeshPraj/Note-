//! Printing: Scintilla renders the document onto a printer device context
//! itself (`SCI_FORMATRANGEFULL`), so syntax colours, wrapping and fonts match
//! the editor. This file only gets a device context and walks the pages.

use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows::Win32::UI::Controls::Dialogs::*;
use windows::core::PCWSTR;

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
    let (dpix, dpiy) = (caps(LOGPIXELSX), caps(LOGPIXELSY));
    let (offx, offy) = (caps(PHYSICALOFFSETX), caps(PHYSICALOFFSETY));
    // 0.75 inch margins, measured from the paper edge rather than the printable area.
    let mx = (dpix * 3 / 4 - offx).max(0);
    let my = (dpiy * 3 / 4 - offy).max(0);

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
        rc: [mx, my, w - mx, h - my],
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
    pd.nCopies = 1;
    if !unsafe { PrintDlgW(&mut pd) }.as_bool() {
        free(pd.hDevMode);
        free(pd.hDevNames);
        return if dialog { Ok(Outcome::Cancelled) } else { Err("No default printer is set up.".into()) };
    }
    let dc = pd.hDC;
    let result = print_to_dc(sci_view, dc, title, None);
    unsafe {
        let _ = DeleteDC(dc);
    }
    free(pd.hDevMode);
    free(pd.hDevNames);
    result.map(|pages| Outcome::Printed { pages })
}
