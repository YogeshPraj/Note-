//! The settings window: every setting in one dialog, with category navigation on the left and the chosen page on the right.
//!
//! The navigation is a vertical [`TabStrip`](crate::tab_strip::TabStrip). Pages are plain data ([`Page`]); the
//! window draws their rows from the theme tokens and reports each change at once, so there is no OK button: the
//! application applies and saves what it is told.
//!
//! ```ignore
//! let pages = vec![Page::new("editor", "Editor", 0, vec![Row::new("wrap", "Word wrap", "Wrap long lines.", Control::Toggle(true))])];
//! settings_window::show(&frame, "Settings", &pages, "editor", &Rc::new(|key, value| { /* apply */ }));
//! ```

mod model;
mod page;

use std::rc::Rc;

use wxdragon::prelude::Dialog as WxDialog;
use wxdragon::prelude::*;

pub use model::{Control, Page, Row, Value, page_index, parse_number};

use crate::controls::{Dialog, Panel as ThemedPanel};
use crate::tab_strip::{Tab, TabEvent, TabStrip};
use crate::theme;

/// Called with a row's key and its new value whenever the user changes a setting.
pub type OnChange = Rc<dyn Fn(&str, Value)>;

/// Show the window (modal) on page `start`.
pub fn show(owner: &Frame, title: &str, pages: &[Page], start: &str, on_change: &OnChange) {
    let t = theme::current();
    let dialog = Dialog::adopt(
        WxDialog::builder(owner, title).with_size(t.px(1000), t.px(700)).with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder).build(),
    );
    let nav = TabStrip::vertical(&dialog, t.px(230));
    nav.set_font(t.ui_font());
    let host = ThemedPanel::new(&dialog);
    let views: Vec<_> = pages.iter().map(|p| page::build(&host, p, on_change)).collect();
    let page_host = BoxSizer::builder(Orientation::Vertical).build();
    for v in &views {
        page_host.add(&v.window(), 1, SizerFlag::Expand, 0);
    }
    host.set_sizer(page_host, true);
    let root = BoxSizer::builder(Orientation::Horizontal).build();
    root.add(&nav.panel(), 0, SizerFlag::Expand, 0);
    root.add(&host, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(root, true);

    let tabs: Vec<Tab> = pages.iter().map(|p| Tab { title: p.title.clone(), dirty: false, badge: None, pinned: false, color: None, level: p.level }).collect();
    let select = {
        let views = views.clone();
        let (nav, host) = (nav.clone(), host);
        move |i: usize| {
            for (n, v) in views.iter().enumerate() {
                v.window().show(n == i);
            }
            nav.set_tabs(tabs.clone(), i);
            host.layout();
        }
    };
    select(page_index(pages, start));
    nav.on_event(move |e| {
        if let TabEvent::Select(i) = e {
            select(i);
        }
    });
    dialog.centre();
    dialog.show_modal();
    dialog.destroy();
}
