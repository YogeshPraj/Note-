//! The window's chrome: menu bar, toolbar, tab strip and status bar.
//!
//! Each piece is a reusable control in [`crate::widgets`]; this module only supplies the application's
//! content (menu titles, toolbar buttons, status text) and turns their events into one [`Act`] stream.

use std::rc::Rc;

use notepp_core::commands::*;
use notepp_core::commands::{ID_CLOSE, ID_SAVE};
use notepp_core::workspace::TabInfo;
use wxdragon::prelude::*;

use crate::widgets::menu_bar::{self, MenuBar, MenuEvent};
use crate::widgets::paint::Rgb;
use crate::widgets::status_bar::{self, Segment, StatusBar, StatusEvent};
use crate::widgets::tab_strip::{self, TabEvent, TabStrip};
use crate::widgets::tool_bar::{self, Item, Pill, ToolBar, ToolEvent};

pub const MENU_H: i32 = menu_bar::HEIGHT;
pub const TOOL_H: i32 = tool_bar::HEIGHT;
pub const TAB_H: i32 = tab_strip::HEIGHT;
pub const STATUS_H: i32 = status_bar::HEIGHT;

const ICON: i32 = 18;

/// What the user did in the chrome.
#[derive(Clone, Copy, Debug)]
pub enum Act {
    Cmd(usize),
    Select(usize),
    Close(usize),
    New,
    Move(usize, usize),
    /// Right-click on tab i at (x, y) in the tab strip.
    Context(usize, i32, i32),
    /// A status segment was clicked; (x, y) is its top-left corner in the status bar.
    Status(isize, i32, i32),
    /// Open menu i; (x, y) is where its popup goes, in the menu bar's coordinates.
    Menu(usize, i32, i32),
}

pub const SEG_ENC: isize = 1;
pub const SEG_EOL: isize = 2;
pub const SEG_LANG: isize = 3;

#[derive(Default, Clone)]
pub struct StatusText {
    pub position: String,
    pub lines: String,
    pub length: String,
    pub enc: String,
    pub eol: String,
    pub lang: String,
}

pub struct Chrome {
    pub menu: Panel,
    pub tools: Panel,
    pub tabs: Panel,
    pub status: Panel,
    menu_bar: MenuBar,
    tool_bar: ToolBar,
    strip: TabStrip,
    status_bar: StatusBar,
}
macro_rules! btn {
    ($cmd:expr, $file:literal, $tip:literal) => {
        (Some($cmd), include_bytes!(concat!("../../assets/", $file)) as &[u8], $tip)
    };
}

fn toolbar_table() -> Vec<(Option<usize>, &'static [u8], &'static str)> {
    let sep = (None, &[][..], "");
    vec![
        btn!(ID_NEW, "new.svg", "New (Ctrl+N)"),
        btn!(ID_NEW_WB, "whiteboard.svg", "New Whiteboard"),
        btn!(ID_OPEN, "open.svg", "Open (Ctrl+O)"),
        btn!(ID_SAVE, "save.svg", "Save (Ctrl+S)"),
        btn!(ID_SAVEALL, "saveall.svg", "Save All (Ctrl+Shift+S)"),
        btn!(ID_CLOSE, "close.svg", "Close (Ctrl+W)"),
        btn!(ID_PRINT, "print.svg", "Print"),
        btn!(ID_ENC_TOGGLE, "lock.svg", "Encrypt this file (or remove its encryption)"),
        btn!(ID_PW_TOOLBAR, "key.svg", "Passwords (Bitwarden)"),
        sep,
        btn!(ID_CUT, "cut.svg", "Cut (Ctrl+X)"),
        btn!(ID_COPY, "copy.svg", "Copy (Ctrl+C)"),
        btn!(ID_PASTE, "paste.svg", "Paste (Ctrl+V)"),
        sep,
        btn!(ID_UNDO, "undo.svg", "Undo (Ctrl+Z)"),
        btn!(ID_REDO, "redo.svg", "Redo (Ctrl+Y)"),
        sep,
        btn!(ID_FIND, "find.svg", "Find (Ctrl+F)"),
        btn!(ID_REPLACE, "replace.svg", "Replace (Ctrl+H)"),
        sep,
        btn!(ID_ZOOMIN, "zoomin.svg", "Zoom In"),
        btn!(ID_ZOOMOUT, "zoomout.svg", "Zoom Out"),
        sep,
        btn!(ID_WRAP, "wordwrap.svg", "Word Wrap"),
        btn!(ID_PREVIEW, "preview.svg", "Live Preview (Ctrl+Shift+V)"),
        btn!(ID_LOG_FOLLOW, "follow.svg", "Log Follow: stream a .log / .out / .err file as it grows"),
        btn!(ID_THEME, "darkmode.svg", "Dark Mode"),
    ]
}

fn toolbar_items() -> Vec<Item> {
    let svg = |svg: &[u8]| BitmapBundle::from_svg_data(svg, Size::new(ICON, ICON)).and_then(|b| b.get_bitmap(Size::new(ICON, ICON)));
    toolbar_table()
        .into_iter()
        .map(|(cmd, data, tip)| match cmd {
            Some(cmd) => Item::Button { cmd, tip, icon: svg(data) },
            None => Item::Separator,
        })
        .collect()
}

impl Chrome {
    pub fn new(parent: &Frame) -> Chrome {
        let menu_bar = MenuBar::new(parent);
        let tool_bar = ToolBar::new(parent, toolbar_items());
        let strip = TabStrip::new(parent);
        strip.set_font(Font::builder().with_face_name("Segoe UI").with_point_size(9).build());
        let status_bar = StatusBar::new(parent);
        Chrome { menu: menu_bar.panel(), tools: tool_bar.panel(), tabs: strip.panel(), status: status_bar.panel(), menu_bar, tool_bar, strip, status_bar }
    }

    pub fn set_pill(&self, p: Option<(String, Rgb, usize)>) {
        self.tool_bar.set_pill(p.map(|(text, colour, cmd)| Pill { text, colour, cmd }));
    }

    pub fn set_menu_titles(&self, titles: Vec<String>) {
        self.menu_bar.set_titles(titles);
    }

    /// Where menu `i`'s popup goes (screen coordinates) and how many menus there are, usable while the
    /// application is busy showing one.
    pub fn menu_locator(&self) -> (Box<dyn Fn(i32) -> (i32, i32)>, i32) {
        self.menu_bar.locator()
    }

    /// Route every control's events to `h`.
    pub fn set_handler(&self, h: Rc<dyn Fn(Act)>) {
        let a = h.clone();
        self.strip.on_event(move |e| {
            a(match e {
                TabEvent::Select(i) => Act::Select(i),
                TabEvent::Close(i) => Act::Close(i),
                TabEvent::New => Act::New,
                TabEvent::Move(from, to) => Act::Move(from, to),
                TabEvent::Context(i, x, y) => Act::Context(i, x, y),
            })
        });
        let a = h.clone();
        self.menu_bar.on_event(move |MenuEvent::Open(i, x, y)| a(Act::Menu(i, x, y)));
        let a = h.clone();
        self.tool_bar.on_event(move |ToolEvent::Command(c)| a(Act::Cmd(c)));
        self.status_bar.on_event(move |StatusEvent::Clicked(id, x, y)| h(Act::Status(id, x, y)));
    }

    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        self.strip.set_tabs(tabs, active);
    }

    pub fn set_status(&self, t: StatusText) {
        self.status_bar.set(
            t.position,
            vec![Segment::text("INS"), Segment::button(t.lang, SEG_LANG), Segment::button(t.eol, SEG_EOL), Segment::button(t.enc, SEG_ENC), Segment::text(t.length), Segment::text(t.lines)],
        );
    }

    pub fn set_checked(&self, cmd: usize, on: bool) {
        self.tool_bar.set_checked(cmd, on);
    }
}