//! The picture editor: `assets/imageeditor.html` in a web view. The app hands it a picture (as the file's bytes, or as
//! raw pixels from the clipboard) and the page answers with the edited picture as a PNG.
//!
//! Messages from the page: `ready`, `state:<0|1>:<base64 PNG>` (1 when the user changed something), `save`,
//! `copy-image:<w>:<h>:<base64 RGBA>`, and the window's own keys (`tab:next`, `tab:prev`, `menu:<letter>`).

use crate::widgets::web_pane::WebPane;

const PAGE: &str = include_str!("../../../../assets/imageeditor.html");

pub fn page_html() -> &'static str {
    PAGE
}

/// Show a picture given as base64 file bytes.
pub fn load(pane: &WebPane, content: &str) {
    let mime = notepp_core::image::content_mime(content);
    pane.call_big("window.np.load", &format!("'{mime}',"), content);
}

/// Show a picture given as base64 RGBA pixels (a bitmap from the clipboard).
pub fn load_pixels(pane: &WebPane, width: u32, height: u32, rgba_base64: &str) {
    pane.call_big("window.np.loadPixels", &format!("{width},{height},"), rgba_base64);
}

pub fn set_theme(pane: &WebPane, dark: bool) {
    pane.eval(&format!("window.np.setTheme({dark})"));
}

/// What the page said.
#[derive(Debug, PartialEq)]
pub enum Message {
    Ready,
    /// The picture as a PNG (base64), and whether the user changed it (as opposed to the page converting it).
    State {
        edited: bool,
        content: String,
    },
    Save,
    /// `copy-image:...`, handled like the preview's copy.
    CopyImage(String),
    Tab(i32),
    Menu(String),
}

pub fn parse(msg: &str) -> Option<Message> {
    Some(match msg {
        "ready" => Message::Ready,
        "save" => Message::Save,
        "tab:next" => Message::Tab(1),
        "tab:prev" => Message::Tab(-1),
        m if m.starts_with("menu:") => Message::Menu(m[5..].to_string()),
        m if m.starts_with("copy-image:") => Message::CopyImage(m.to_string()),
        m => {
            let rest = m.strip_prefix("state:")?;
            let (flag, content) = rest.split_once(':')?;
            Message::State { edited: flag == "1", content: content.to_string() }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_speaks_in_prefixed_strings() {
        assert_eq!(parse("ready"), Some(Message::Ready));
        assert_eq!(parse("save"), Some(Message::Save));
        assert_eq!(parse("tab:prev"), Some(Message::Tab(-1)));
        assert_eq!(parse("menu:f"), Some(Message::Menu("f".into())));
        assert_eq!(parse("menu:"), Some(Message::Menu(String::new())));
        assert_eq!(parse("state:1:AAAA"), Some(Message::State { edited: true, content: "AAAA".into() }));
        assert_eq!(parse("state:0:QQ=="), Some(Message::State { edited: false, content: "QQ==".into() }));
        assert_eq!(parse("copy-image:1:1:AAAAAA=="), Some(Message::CopyImage("copy-image:1:1:AAAAAA==".into())));
        assert_eq!(parse("state:oops"), None);
        assert_eq!(parse("something else"), None);
    }
}
