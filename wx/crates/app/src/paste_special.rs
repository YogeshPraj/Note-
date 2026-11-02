//! Edit > Paste Special: paste the clipboard's HTML or RTF source as text, and copy, cut or paste text holding NUL
//! bytes (which ordinary text copying cuts short) through a private clipboard format.

use notepp_core::commands as c;
use notepp_core::{sci, search, textools};
use wxdragon::prelude::WxWidget;

use crate::app::App;
use crate::command::CommandHandler;

/// The private clipboard format for text with NUL bytes (the same name the Win32 build used).
const BINARY_FORMAT: &str = "Note++ Binary Text";

pub struct PasteSpecialCommands;

impl CommandHandler<App> for PasteSpecialCommands {
    fn name(&self) -> &'static str {
        "paste-special"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        let owner = app.frame.get_handle() as isize;
        let os = crate::platform::current();
        match id {
            c::ID_PS_HTML | c::ID_PS_RTF => {
                let (format, what) = if id == c::ID_PS_HTML { ("HTML Format", "HTML") } else { ("Rich Text Format", "RTF") };
                match os.clipboard_format(owner, format) {
                    Some(b) if id == c::ID_PS_HTML => app.ws.insert_text(&textools::html_fragment(&b)),
                    Some(b) => app.ws.insert_text(String::from_utf8_lossy(&b).trim_end_matches('\0')),
                    None => app.flash = Some(format!("No {what} content on the clipboard")),
                }
            }
            c::ID_PS_COPY_BIN | c::ID_PS_CUT_BIN => {
                let text = search::selected_text(&app.ws.sci);
                app.flash = Some(if text.is_empty() {
                    "Select some text first".into()
                } else if os.set_clipboard_format(owner, BINARY_FORMAT, text.as_bytes(), &textools::binary_as_text(text.as_bytes())) {
                    if id == c::ID_PS_CUT_BIN {
                        notepp_core::sci::Send::send(&app.ws.sci, sci::SCI_CLEAR, 0, 0);
                    }
                    format!("{} byte(s) on the clipboard", text.len())
                } else {
                    "The clipboard is busy".into()
                });
            }
            c::ID_PS_PASTE_BIN => match os.clipboard_format(owner, BINARY_FORMAT) {
                Some(b) => app.ws.insert_bytes(&b),
                None => app.flash = Some("No binary content on the clipboard".into()),
            },
            _ => return false,
        }
        true
    }
}
