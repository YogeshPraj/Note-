//! The component gallery: every component of the kit on one window, in the light or the dark theme.
//!
//! ```text
//! cargo run -p ui-kit --example gallery            # light
//! cargo run -p ui-kit --example gallery -- dark    # dark
//! ```
//!
//! The "Toggle theme" button switches live. Nothing here has a colour or a font of its own: the whole window
//! is painted from the theme, which is the point.

use std::rc::Rc;

use ui_kit::components::form::{self, Kind, Spec, ctl};
use ui_kit::components::quick_pick::{self, Item as PickItem};
use ui_kit::components::status_bar::Segment;
use ui_kit::components::tab_strip::Tab;
use ui_kit::components::tool_bar::Item as ToolItem;
use ui_kit::controls::{Button, CheckBox, Label, ListBox, ListView, RadioButton, TextBox, ToggleButton, TreeView};
use ui_kit::menu_bar::MenuBar;
use ui_kit::paint::colour;
use ui_kit::status_bar::StatusBar;
use ui_kit::tab_strip::TabStrip;
use ui_kit::theme::{self, Theme};
use ui_kit::tool_bar::ToolBar;
use ui_kit::web_pane::WebPane;
use wxdragon::prelude::*;

const ICON: i32 = 18;

/// A small icon in the theme's text colour, so the tool bar shows something in either theme.
fn icon(theme: &Theme, path: &str) -> Option<Bitmap> {
    let (r, g, b) = theme.colors.text;
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><path d='{path}' fill='none' stroke='#{r:02x}{g:02x}{b:02x}' stroke-width='1.5' stroke-linecap='round'/></svg>"
    );
    BitmapBundle::from_svg_data(svg.as_bytes(), Size::new(ICON, ICON)).and_then(|b| b.get_bitmap(Size::new(ICON, ICON)))
}

fn row(parent: &Panel, caption: &str, add: impl FnOnce(&Panel) -> Box<dyn Fn(&BoxSizer)>) -> BoxSizer {
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let label = Label::new(parent, caption);
    label.set_min_size(Size::new(110, -1));
    row.add(&label, 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 4);
    add(parent)(&row);
    row
}

fn main() {
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);
    let start_dark = std::env::args().any(|a| a == "dark");
    let _ = wxdragon::main(move |_| {
        let frame = Frame::builder().with_title("Note++ UI kit - gallery").with_size(Size::new(980, 760)).build();
        let initial = if start_dark { Theme::dark() } else { Theme::light() };
        theme::set(initial);
        theme::style_native(&frame);
        {
            let f = frame;
            theme::subscribe(move |t| {
                f.set_background_color(colour(t.colors.surface));
                theme::style_native(&f);
                f.refresh(true, None);
            });
        }

        let menu = MenuBar::new(&frame);
        menu.set_titles(["File", "Edit", "Search", "View", "Help"].iter().map(|s| (*s).to_string()).collect());

        let tools = ToolBar::new(
            &frame,
            vec![
                ToolItem::Button {
                    cmd: 1,
                    tip: "New (Ctrl+N)".into(),
                    icon: icon(&initial, "M4 2h6l3 3v9H4z"),
                    faded: icon(&Theme::light(), "M4 2h6l3 3v9H4z"),
                },
                ToolItem::Button { cmd: 2, tip: "Open (Ctrl+O)".into(), icon: icon(&initial, "M2 5h5l1 2h6v6H2z"), faded: None },
                ToolItem::Separator,
                ToolItem::Button { cmd: 3, tip: "Find (Ctrl+F)".into(), icon: icon(&initial, "M7 2a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM11 11l3 3"), faded: None },
            ],
        );
        tools.set_checked(3, true);
        tools.set_enabled(2, false);

        let tabs = TabStrip::new(&frame);
        tabs.set_tabs(
            vec![
                Tab { title: "readme.md".into(), dirty: false, badge: None, pinned: true, color: None },
                Tab { title: "main.rs".into(), dirty: true, badge: None, pinned: false, color: Some((0xE0, 0x6C, 0x75)) },
                Tab { title: "whiteboard-1.json".into(), dirty: false, badge: Some("wb"), pinned: false, color: None },
            ],
            1,
        );

        let status = StatusBar::new(&frame);
        status.set("Ln 12, Col 4".into(), vec![Segment::text("lines: 120"), Segment::button("UTF-8", 1), Segment::button("Rust", 2)]);

        let body = Panel::builder(&frame).build();
        theme::own_background(body);
        let column = BoxSizer::builder(Orientation::Vertical).build();
        let b = body;

        column.add_sizer(
            &row(&b, "Buttons", |p| {
                let (ok, toggle, check, radio) = (
                    Button::new(p, "Button"),
                    ToggleButton::new(p, "Toggle"),
                    CheckBox::new(p, "Check box"),
                    RadioButton::adopt(wxdragon::prelude::RadioButton::builder(p).with_label("Radio").build()),
                );
                Box::new(move |r| {
                    r.add(&ok, 0, SizerFlag::All, 4);
                    r.add(&toggle, 0, SizerFlag::All, 4);
                    r.add(&check, 0, SizerFlag::All, 4);
                    r.add(&radio, 0, SizerFlag::All, 4);
                })
            }),
            0,
            SizerFlag::Expand,
            0,
        );

        column.add_sizer(
            &row(&b, "Text box", |p| {
                let t = TextBox::new(p, "Type here", false);
                t.set_min_size(Size::new(260, -1));
                Box::new(move |r| {
                    r.add(&t, 0, SizerFlag::All, 4);
                })
            }),
            0,
            SizerFlag::Expand,
            0,
        );

        column.add_sizer(
            &row(&b, "List box", |p| {
                let l = ListBox::new(p);
                for s in ["one", "two", "three"] {
                    l.append(s);
                }
                l.set_min_size(Size::new(260, 70));
                Box::new(move |r| {
                    r.add(&l, 0, SizerFlag::All, 4);
                })
            }),
            0,
            SizerFlag::Expand,
            0,
        );

        column.add_sizer(
            &row(&b, "List view", |p| {
                let v = ListView::adopt(ListCtrl::builder(p).with_style(ListCtrlStyle::Report).build());
                v.set_columns(&[("Name", 140), ("Size", 80)]);
                v.fill(&[vec!["main.rs".into(), "4 KB".into()], vec!["lib.rs".into(), "12 KB".into()]]);
                v.set_min_size(Size::new(260, 80));
                Box::new(move |r| {
                    r.add(&v, 0, SizerFlag::All, 4);
                })
            }),
            0,
            SizerFlag::Expand,
            0,
        );

        column.add_sizer(
            &row(&b, "Tree view", |p| {
                let t = TreeView::adopt(TreeCtrl::builder(p).build());
                if let Some(root) = t.add_root("workspace", None, None) {
                    let _ = t.append_item(&root, "src", None, None);
                }
                t.set_min_size(Size::new(260, 70));
                Box::new(move |r| {
                    r.add(&t, 0, SizerFlag::All, 4);
                })
            }),
            0,
            SizerFlag::Expand,
            0,
        );

        let actions = BoxSizer::builder(Orientation::Horizontal).build();
        let toggle = Button::new(&b, "Toggle theme");
        let dialog = Button::new(&b, "Form dialog...");
        let pick = Button::new(&b, "Quick pick...");
        for w in [&toggle, &dialog, &pick] {
            actions.add(w, 0, SizerFlag::All, 6);
        }
        column.add_sizer(&actions, 0, SizerFlag::Expand, 0);

        let web = WebPane::new(&b);
        web.load_html("<body style='font:14px Segoe UI,sans-serif;padding:8px'>A <b>WebPane</b> hosts HTML; the host tells it the theme.</body>");
        web.set_min_size(Size::new(-1, 70));
        column.add(&*web, 1, SizerFlag::Expand | SizerFlag::All, 6);
        body.set_sizer(column, true);

        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&menu.panel(), 0, SizerFlag::Expand, 0);
        sizer.add(&tools.panel(), 0, SizerFlag::Expand, 0);
        sizer.add(&tabs.panel(), 0, SizerFlag::Expand, 0);
        sizer.add(&body, 1, SizerFlag::Expand, 0);
        sizer.add(&status.panel(), 0, SizerFlag::Expand, 0);
        frame.set_sizer(sizer, true);

        let web = Rc::new(web);
        {
            let web = web.clone();
            theme::subscribe(move |t| {
                web.sync_dark(t.dark);
            });
        }
        toggle.on_click(move |_| {
            let next = if theme::is_dark() { Theme::light() } else { Theme::dark() };
            theme::set(next);
        });
        dialog.on_click(move |_| {
            let spec = Spec {
                title: "Form dialog",
                size: (320, 150),
                ctls: vec![ctl(10, Kind::Label, "Name:", 12, 14, 80, 20), ctl(11, Kind::Edit, "", 96, 12, 200, 24)],
                ok: "OK",
                buttons: None,
                on_change: None,
                on_apply: None,
            };
            let _ = form::run(&frame, spec, |f| f.focus(11));
        });
        pick.on_click(move |_| {
            let items = ["File: New", "File: Open...", "Search: Find"]
                .iter()
                .enumerate()
                .map(|(i, l)| PickItem { label: (*l).to_string(), hint: String::new(), id: i })
                .collect();
            let _ = quick_pick::pick(&frame, items);
        });

        frame.show(true);
        frame.centre();
    });
}
