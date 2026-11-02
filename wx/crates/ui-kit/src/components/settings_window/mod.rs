//! The settings window: every setting in one dialog, with category navigation on the left and the chosen page on the right.
//!
//! The navigation is a vertical [`TabStrip`](crate::tab_strip::TabStrip). Pages are plain data ([`Page`]); the
//! window draws their rows from the theme tokens and reports each change at once, so there is no OK button: the
//! application applies and saves what it is told. A page can also hold a whole editor the application builds
//! itself ([`Control::Custom`]).
//!
//! ```ignore
//! let make: MakePages = Rc::new(|| vec![Page::new("editor", "Editor", 0, vec![Row::new("wrap", "Word wrap", "Wrap long lines.", Control::Toggle(true))])]);
//! settings_window::show(&frame, "Settings", &make, "editor", &Rc::new(|key, value| { /* apply */ false }));
//! ```

mod model;
mod page;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use wxdragon::prelude::Dialog as WxDialog;
use wxdragon::prelude::*;

pub use model::{Builder, Control, Custom, Page, Row, Value, page_index, parse_number};
pub use page::Report;

use crate::controls::{Dialog, Panel as ThemedPanel};
use crate::defer::later;
use crate::tab_strip::{Tab, TabEvent, TabStrip};
use crate::theme;

/// Called with a row's key and its new value whenever the user changes a setting; returns true when the pages must be
/// built again (something they show has changed).
pub type OnChange = Rc<dyn Fn(&str, Value) -> bool>;

/// Builds the pages from the current state of the application.
pub type MakePages = Rc<dyn Fn() -> Vec<Page>>;

struct Window {
    owner: Frame,
    nav: TabStrip,
    host: ThemedPanel,
    make: MakePages,
    on_change: OnChange,
    views: RefCell<Vec<page::View>>,
    tabs: RefCell<Vec<Tab>>,
    current: Cell<usize>,
}

impl Window {
    /// (Re)build every page and show the one called `start`, or the one that was showing.
    fn fill(self: &Rc<Self>, start: Option<&str>) {
        let pages = (self.make)();
        let weak: Weak<Window> = Rc::downgrade(self);
        let report: page::Report = Rc::new(move |key, value| {
            let Some(w) = weak.upgrade() else { return };
            if (w.on_change)(key, value) {
                // The page that reported is still handling its event: rebuild afterwards.
                let again = w.clone();
                later(&w.owner.clone(), move || again.fill(None));
            }
        });
        for v in self.views.borrow_mut().drain(..) {
            v.window().destroy();
        }
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        let views: Vec<_> = pages.iter().map(|p| page::build(&self.host, p, &report)).collect();
        for v in &views {
            sizer.add(&v.window(), 1, SizerFlag::Expand, 0);
        }
        self.host.set_sizer(sizer, true);
        *self.views.borrow_mut() = views;
        *self.tabs.borrow_mut() =
            pages.iter().map(|p| Tab { title: p.title.clone(), dirty: false, badge: None, pinned: false, color: None, level: p.level }).collect();
        if let Some(id) = start {
            self.current.set(page_index(&pages, id));
        }
        self.select(self.current.get());
    }

    fn select(&self, i: usize) {
        self.current.set(i);
        for (n, v) in self.views.borrow().iter().enumerate() {
            v.window().show(n == i);
        }
        self.nav.set_tabs(self.tabs.borrow().clone(), i);
        self.host.layout();
    }
}

/// Show the window (modal) on page `start`.
pub fn show(owner: &Frame, title: &str, make: &MakePages, start: &str, on_change: &OnChange) {
    let t = theme::current();
    let dialog = Dialog::adopt(
        WxDialog::builder(owner, title).with_size(t.px(1000), t.px(700)).with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder).build(),
    );
    let nav = TabStrip::vertical(&dialog, t.px(230));
    nav.set_font(t.ui_font());
    let host = ThemedPanel::new(&dialog);
    let root = BoxSizer::builder(Orientation::Horizontal).build();
    root.add(&nav.panel(), 0, SizerFlag::Expand, 0);
    root.add(&host, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(root, true);
    let w = Rc::new(Window {
        owner: *owner,
        nav: nav.clone(),
        host,
        make: make.clone(),
        on_change: on_change.clone(),
        views: RefCell::new(Vec::new()),
        tabs: RefCell::new(Vec::new()),
        current: Cell::new(0),
    });
    w.fill(Some(start));
    let on_nav = w.clone();
    nav.on_event(move |e| {
        if let TabEvent::Select(i) = e {
            on_nav.select(i);
        }
    });
    dialog.centre();
    dialog.show_modal();
    dialog.destroy();
}
