//! The window's chrome: menu bar, toolbar, tab strip and status bar.
//!
//! Each piece is a reusable control in [`crate::widgets`]; this module only supplies the application's
//! content (menu titles, toolbar buttons, status text) and turns their events into one [`Act`] stream.

use std::rc::Rc;

use notepp_core::commands::{ID_CLOSE, ID_SAVE};
use notepp_core::commands::{
    ID_COPY, ID_CUT, ID_ENC_TOGGLE, ID_FIND, ID_LOG_FOLLOW, ID_NEW, ID_NEW_WB, ID_OPEN, ID_PASTE, ID_PREVIEW, ID_PRINT, ID_PW_TOOLBAR, ID_REDO, ID_REPLACE,
    ID_SAVEALL, ID_UNDO, ID_WRAP, ID_ZOOMIN, ID_ZOOMOUT,
};
use notepp_core::workspace::TabInfo;
use wxdragon::prelude::*;

use crate::widgets::menu_bar::{MenuBar, MenuEvent};
use crate::widgets::paint::Rgb;
use crate::widgets::status_bar::{Segment, StatusBar as Bar, StatusEvent};
use crate::widgets::tab_strip::{TabEvent, TabStrip};
use crate::widgets::tool_bar::{Item, Pill, ToolBar, ToolEvent};
use notepp_core::status_bar::{StatusAction, StatusBar};

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
    Menu(usize),
}

/// The status bar's clickable segments are told apart by an id; what each means is `core::status_bar::StatusAction`.
const STATUS_ACTIONS: [StatusAction; 4] = [StatusAction::Encoding, StatusAction::LineEnding, StatusAction::Language, StatusAction::Agents];

fn segment_id(action: StatusAction) -> isize {
    STATUS_ACTIONS.iter().position(|a| *a == action).map_or(0, |i| crate::platform::wx::to_isize(i + 1))
}

/// What a click on segment `id` means.
pub fn status_action(id: isize) -> Option<StatusAction> {
    STATUS_ACTIONS.get(crate::platform::wx::to_usize(id).checked_sub(1)?).copied()
}

pub struct Chrome {
    pub menu: Panel,
    pub tools: Panel,
    pub tabs: Panel,
    pub status: Panel,
    menu_bar: MenuBar,
    tool_bar: ToolBar,
    strip: TabStrip,
    status_bar: Bar,
}
/// Note++'s own SVG icon for toolbar command `id` (the order and tool-tips are `notepp_core::toolbar`'s).
fn toolbar_icon(id: usize) -> &'static [u8] {
    macro_rules! svg {
        ($file:literal) => {
            include_bytes!(concat!("../../../../assets/", $file)) as &[u8]
        };
    }
    match id {
        ID_NEW => svg!("new.svg"),
        ID_NEW_WB => svg!("whiteboard.svg"),
        ID_OPEN => svg!("open.svg"),
        ID_SAVE => svg!("save.svg"),
        ID_SAVEALL => svg!("saveall.svg"),
        ID_CLOSE => svg!("close.svg"),
        ID_PRINT => svg!("print.svg"),
        ID_ENC_TOGGLE => svg!("lock.svg"),
        ID_PW_TOOLBAR => svg!("key.svg"),
        ID_CUT => svg!("cut.svg"),
        ID_COPY => svg!("copy.svg"),
        ID_PASTE => svg!("paste.svg"),
        ID_UNDO => svg!("undo.svg"),
        ID_REDO => svg!("redo.svg"),
        ID_FIND => svg!("find.svg"),
        ID_REPLACE => svg!("replace.svg"),
        ID_ZOOMIN => svg!("zoomin.svg"),
        ID_ZOOMOUT => svg!("zoomout.svg"),
        ID_WRAP => svg!("wordwrap.svg"),
        ID_PREVIEW => svg!("preview.svg"),
        ID_LOG_FOLLOW => svg!("follow.svg"),
        _ => svg!("darkmode.svg"),
    }
}
/// The icon in the colour a command that cannot run is drawn in.
fn faded_svg(svg: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(svg);
    let (r, g, b) = crate::widgets::theme::current().colors.disabled;
    let grey = format!("#{r:02x}{g:02x}{b:02x}");
    let mut out = String::with_capacity(text.len());
    let mut rest: &str = &text;
    while let Some(i) = rest.find("=\"#") {
        let end = rest[i + 3..].find('"').map_or(rest.len(), |e| i + 3 + e);
        let attr_start = rest[..i].rfind(' ').map_or(0, |s| s + 1);
        let name = &rest[attr_start..i];
        out.push_str(&rest[..attr_start]);
        out.push_str(&format!("{name}=\"{grey}\" {name}-opacity=\"0.55\""));
        rest = &rest[(end + 1).min(rest.len())..];
    }
    out.push_str(rest);
    out.into_bytes()
}

fn toolbar_items() -> Vec<Item> {
    let side = crate::widgets::tool_bar::icon_size();
    let bitmap = |svg: &[u8]| BitmapBundle::from_svg_data(svg, Size::new(side, side)).and_then(|b| b.get_bitmap(Size::new(side, side)));
    notepp_core::toolbar::LAYOUT
        .iter()
        .map(|tool| match *tool {
            notepp_core::toolbar::Tool::Button { id, .. } => {
                let data = toolbar_icon(id);
                Item::Button { cmd: id, tip: notepp_core::toolbar::tip(id), icon: bitmap(data), faded: bitmap(&faded_svg(data)) }
            }
            notepp_core::toolbar::Tool::Separator => Item::Separator,
        })
        .collect()
}
impl Chrome {
    pub fn new(parent: &Frame) -> Chrome {
        let menu_bar = MenuBar::new(parent);
        let tool_bar = ToolBar::new(parent, toolbar_items());
        let strip = TabStrip::new(parent);
        strip.set_font(crate::widgets::theme::current().ui_font());
        let status_bar = Bar::new(parent);
        let chrome =
            Chrome { menu: menu_bar.panel(), tools: tool_bar.panel(), tabs: strip.panel(), status: status_bar.panel(), menu_bar, tool_bar, strip, status_bar };
        // Clicking the bars must not take the focus from the editor: typing carries on where it was.
        for bar in [chrome.menu, chrome.tools, chrome.tabs, chrome.status] {
            bar.set_can_focus(false);
        }
        chrome
    }

    pub fn set_pill(&self, p: Option<(String, Rgb, usize)>) {
        self.tool_bar.set_pill(p.map(|(text, colour, cmd)| Pill { text, colour, cmd }));
    }

    pub fn set_menu_titles(&self, titles: Vec<String>) {
        self.menu_bar.set_titles(titles);
    }

    pub fn menu_titles(&self) -> Vec<String> {
        self.menu_bar.titles()
    }

    /// The pointer is at this screen position while a menu holds the mouse.
    pub fn menu_pointer(&self, x: i32, y: i32) {
        self.menu_bar.pointer_moved(x, y);
    }

    /// Mark the menu that is showing, so that moving over another title switches to it.
    pub fn set_menu_open(&self, i: Option<usize>) {
        self.menu_bar.set_open(i);
    }

    /// Where menu `i`'s popup goes (screen coordinates) and how many menus there are, usable while the
    /// application is busy showing one.
    pub fn menu_locator(&self) -> (crate::widgets::menu_bar::Locator, i32) {
        self.menu_bar.locator()
    }

    /// The bars F6 moves through, in order.
    fn bars(&self) -> Vec<&crate::widgets::keynav::KeyNav> {
        [Some(self.tool_bar.keys()), self.strip.keys(), Some(self.status_bar.keys())].into_iter().flatten().collect()
    }

    /// Which bar has the keyboard.
    pub fn bar_in_use(&self) -> Option<usize> {
        self.bars().iter().position(|b| b.in_use())
    }

    /// Bring the keyboard into bar `n`. False if it is hidden or has nothing to land on.
    pub fn enter_bar(&self, n: usize) -> bool {
        self.bars().get(n).is_some_and(|b| b.enter())
    }

    /// Told when the keyboard should leave a bar.
    pub fn set_bar_exit(&self, f: &Rc<dyn Fn(crate::widgets::keynav::Exit)>) {
        for b in self.bars() {
            b.on_exit(f.clone());
        }
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
            });
        });
        let a = h.clone();
        self.menu_bar.on_event(move |MenuEvent::Open(i, ..)| a(Act::Menu(i)));
        let a = h.clone();
        self.tool_bar.on_event(move |ToolEvent::Command(c)| a(Act::Cmd(c)));
        self.status_bar.on_event(move |StatusEvent::Clicked(id, x, y)| h(Act::Status(id, x, y)));
    }

    /// Tool tip text for a tab that is not the active one.
    pub fn set_tab_preview(&self, f: impl Fn(usize) -> String + 'static) {
        self.strip.set_preview(f);
    }

    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        self.strip.set_tabs(
            tabs.into_iter()
                .map(|t| crate::widgets::tab_strip::Tab {
                    title: t.title,
                    dirty: t.dirty,
                    badge: t.badge,
                    pinned: t.pinned,
                    color: t.color,
                    level: 0,
                    beside: t.beside,
                })
                .collect(),
            active,
        );
    }

    /// Draw the bar `core` laid out.
    pub fn set_status(&self, bar: StatusBar) {
        let segments = bar
            .segments
            .into_iter()
            .map(|s| match s.action {
                Some(action) => Segment::button(s.text, segment_id(action)),
                None => Segment::text(s.text),
            })
            .collect();
        self.status_bar.set(bar.left, segments);
    }

    /// Tool-tips that come from the registry show the keymap's effective shortcut.
    pub fn refresh_tips(&self, keymap: &notepp_core::keymap::KeyMap) {
        for tool in notepp_core::toolbar::LAYOUT {
            if let notepp_core::toolbar::Tool::Button { id, tip: None } = *tool
                && let Some(spec) = notepp_core::registry::spec(id)
            {
                self.tool_bar.set_tip(id, spec.tooltip_with(keymap.shown(id)));
            }
        }
    }

    pub fn set_enabled(&self, cmd: usize, on: bool) {
        self.tool_bar.set_enabled(cmd, on);
    }

    pub fn set_checked(&self, cmd: usize, on: bool) {
        self.tool_bar.set_checked(cmd, on);
    }
}
