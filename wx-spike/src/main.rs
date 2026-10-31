// Feasibility spike: wxWidgets (via wxDragon) with a Scintilla-based StyledTextCtrl,
// driven through raw Scintilla messages the way Note++'s core does it.
use std::ffi::{c_char, c_void, CString};
use wxdragon::prelude::*;

type Direct = unsafe extern "C" fn(*mut c_void, u32, usize, isize) -> isize;

struct Raw {
    f: Direct,
    p: *mut c_void,
}

impl Raw {
    fn send(&self, m: u32, w: usize, l: isize) -> isize {
        unsafe { (self.f)(self.p, m, w, l) }
    }
}

const SCI_SETTEXT: u32 = 2181;
const SCI_GETLENGTH: u32 = 2006;
const SCI_STYLESETFORE: u32 = 2051;
const SCI_CREATEDOCUMENT: u32 = 2375;
const SCI_ADDREFDOCUMENT: u32 = 2376;
const SCI_RELEASEDOCUMENT: u32 = 2377;
const SCI_SETDOCPOINTER: u32 = 2358;
const SCI_GETDOCPOINTER: u32 = 2357;
const SCI_SETILEXER: u32 = 4033;

extern "C" {
    // Lexilla, bundled inside wxWidgets' STC.
    fn CreateLexer(name: *const c_char) -> *mut c_void;
}

fn rgb(r: u8, g: u8, b: u8) -> isize {
    (r as isize) | ((g as isize) << 8) | ((b as isize) << 16)
}

fn main() {
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);
    let _ = wxdragon::main(|_| {
        let frame = Frame::builder().with_title("Note++ wx spike").with_size(Size::new(900, 600)).build();
        let stc = StyledTextCtrl::builder(&frame).build();

        let win = stc.handle_ptr() as *mut wxdragon_sys::wxd_StyledTextCtrl_t;
        let (f, p) = unsafe { (wxdragon_sys::wxd_StyledTextCtrl_GetDirectFunction(win), wxdragon_sys::wxd_StyledTextCtrl_GetDirectPointer(win)) };
        println!("direct fn null: {}  ptr null: {}", f.is_null(), p.is_null());
        let raw = Raw { f: unsafe { std::mem::transmute::<*mut c_void, Direct>(f) }, p };

        // 1. raw text + length
        let text = CString::new("// raw messages\nint main() {\n    return \"x\".len();\n}\n").unwrap();
        raw.send(SCI_SETTEXT, 0, text.as_ptr() as isize);
        println!("length via raw SCI_GETLENGTH: {}", raw.send(SCI_GETLENGTH, 0, 0));

        // 2. documents: the core keeps one view and swaps documents per tab
        let first = raw.send(SCI_GETDOCPOINTER, 0, 0);
        raw.send(SCI_ADDREFDOCUMENT, 0, first); // the view's initial document is owned only by the view
        let second = raw.send(SCI_CREATEDOCUMENT, 0, 0);
        raw.send(SCI_SETDOCPOINTER, 0, second);
        let t2 = CString::new("second tab").unwrap();
        raw.send(SCI_SETTEXT, 0, t2.as_ptr() as isize);
        println!("second doc length: {}", raw.send(SCI_GETLENGTH, 0, 0));
        raw.send(SCI_SETDOCPOINTER, 0, first);
        println!("back on first doc, length: {}  (docs differ: {})", raw.send(SCI_GETLENGTH, 0, 0), first != second);
        raw.send(SCI_RELEASEDOCUMENT, 0, second);

        // 3. a Lexilla lexer attached by us, colours set by style number
        let lexer = unsafe { CreateLexer(c"cpp".as_ptr()) };
        println!("CreateLexer(cpp) null: {}", lexer.is_null());
        raw.send(SCI_SETILEXER, 0, lexer as isize);
        raw.send(SCI_STYLESETFORE, 1, rgb(0x00, 0x80, 0x00)); // comment line
        raw.send(SCI_STYLESETFORE, 5, rgb(0x00, 0x00, 0xFF)); // keyword
        raw.send(SCI_STYLESETFORE, 6, rgb(0xC0, 0x00, 0x00)); // string

        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&stc, 1, SizerFlag::Expand, 0);
        frame.set_sizer(sizer, true);
        frame.show(true);
        frame.centre();
    });
}
