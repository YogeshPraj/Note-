//! One page of the settings window: its title and rows, laid out from the theme tokens.

use std::rc::Rc;

use wxdragon::prelude::*;

use super::model::{Control, Page, Row, Value};
use crate::controls::{Button, CheckBox, Label, Panel as ThemedPanel, RadioButton, TextBox};
use crate::paint::colour;
use crate::theme::{self, Role};

/// What a page tells the window when a value changes.
pub type Report = Rc<dyn Fn(&str, Value)>;

/// A page's scrolling window.
#[derive(Clone, Copy)]
pub struct View(ScrolledWindow);

impl View {
    pub fn window(&self) -> ScrolledWindow {
        self.0
    }
}

/// How wide a description wraps, before scaling: in a two-column group, and across a whole page.
const WRAP_CELL: i32 = 300;
const WRAP_FULL: i32 = 560;
/// Space around the page's content and between its groups, before scaling.
const MARGIN: i32 = 16;

pub fn build(parent: &impl WxWidget, page: &Page, on_change: &Report) -> View {
    let t = theme::current();
    let sw = ScrolledWindow::builder(parent).build();
    theme::adopt(sw, Role::Surface);
    sw.set_scroll_rate(0, t.px(12));
    let col = BoxSizer::builder(Orientation::Vertical).build();
    let title = Label::new(&sw, &page.title);
    if let Some(f) = t.title_font() {
        title.set_font(&f);
    }
    col.add(&title, 0, SizerFlag::All, t.px(MARGIN));
    let mut cells: Vec<BoxSizer> = Vec::new();
    for row in &page.rows {
        match row.control {
            Control::Heading => {
                flush(&sw, &col, &mut cells);
                add_heading(&sw, &col, row);
            }
            Control::Custom(_) => {
                flush(&sw, &col, &mut cells);
                let line = cell(&sw, row, on_change, WRAP_FULL);
                col.add_sizer(&line, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, t.px(MARGIN));
            }
            _ => cells.push(cell(&sw, row, on_change, WRAP_CELL)),
        }
    }
    flush(&sw, &col, &mut cells);
    col.add_spacer(t.px(MARGIN));
    sw.set_sizer(col, true);
    sw.layout();
    sw.show(false);
    View(sw)
}

fn strong(label: &Label) {
    if let Some(f) = theme::current().strong_font() {
        label.set_font(&f);
    }
}

/// The rows collected so far, laid out two to a line.
fn flush(_sw: &ScrolledWindow, col: &BoxSizer, cells: &mut Vec<BoxSizer>) {
    if cells.is_empty() {
        return;
    }
    let t = theme::current();
    let grid = GridSizer::builder(0, 2).with_vgap(t.px(12)).with_hgap(t.px(MARGIN)).build();
    for c in cells.drain(..) {
        grid.add_sizer(&c, 1, SizerFlag::Expand, 0);
    }
    col.add_sizer(&grid, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, t.px(MARGIN));
}

fn desc(sw: &ScrolledWindow, text: &str, wrap: i32) -> Option<Label> {
    if text.is_empty() {
        return None;
    }
    let l = Label::new(sw, text);
    l.wrap(theme::current().px(wrap));
    theme::subscribe_while(move |t| {
        if !l.is_valid() {
            return false;
        }
        l.set_foreground_color(colour(t.colors.text_muted));
        true
    });
    Some(l)
}

fn add_heading(sw: &ScrolledWindow, col: &BoxSizer, row: &Row) {
    let t = theme::current();
    let h = Label::new(sw, &row.title);
    strong(&h);
    col.add(&h, 0, SizerFlag::Left | SizerFlag::Top | SizerFlag::Right, t.px(MARGIN));
    if let Some(d) = desc(sw, &row.desc, WRAP_FULL) {
        col.add(&d, 0, SizerFlag::Left | SizerFlag::Top | SizerFlag::Right, t.px(MARGIN));
    }
}

/// One setting: its title, description and control.
fn cell(sw: &ScrolledWindow, row: &Row, on_change: &Report, wrap: i32) -> BoxSizer {
    let t = theme::current();
    let line = BoxSizer::builder(Orientation::Horizontal).build();
    let body = BoxSizer::builder(Orientation::Vertical).build();
    let key = row.key;
    let report = {
        let f = on_change.clone();
        move |v: Value| f(key, v)
    };
    if let Control::Toggle(on) = &row.control {
        let c = CheckBox::new(sw, &row.title);
        c.set_value(*on);
        c.on_toggled(move |_| report(Value::Bool(c.is_checked())));
        body.add(&c, 0, SizerFlag::Bottom, t.px(4));
        add_desc(sw, &body, &row.desc, wrap);
    } else {
        let title = Label::new(sw, &row.title);
        body.add(&title, 0, SizerFlag::Bottom, t.px(4));
        add_desc(sw, &body, &row.desc, wrap);
        add_control(sw, &body, &row.control, report, on_change);
    }
    line.add_sizer(&body, 1, SizerFlag::Expand, 0);
    line
}

fn add_desc(sw: &ScrolledWindow, body: &BoxSizer, text: &str, wrap: i32) {
    if let Some(d) = desc(sw, text, wrap) {
        body.add(&d, 0, SizerFlag::Bottom, theme::current().px(8));
    }
}
fn dropdown(sw: &ScrolledWindow, body: &BoxSizer, options: &[String], selected: usize, report: impl Fn(Value) + 'static) {
    let t = theme::current();
    let c = Choice::builder(sw).build();
    for name in options {
        c.append(name);
    }
    c.set_selection(crate::num::to_u32(selected));
    c.set_min_size(Size::new(t.px(260), -1));
    theme::adopt(c, Role::Input);
    c.on_selection_changed(move |e| {
        if let Some(i) = e.get_selection() {
            report(Value::Choice(crate::num::to_usize(i)));
        }
    });
    body.add(&c, 0, SizerFlag::Bottom, t.px(4));
}

fn add_control(sw: &ScrolledWindow, body: &BoxSizer, control: &Control, report: impl Fn(Value) + Clone + 'static, on_change: &Report) {
    let t = theme::current();
    match control {
        Control::Number { value, min, max } => {
            let b = SpinCtrl::builder(sw).with_range(crate::num::to_i32(*min), crate::num::to_i32(*max)).with_initial_value(crate::num::to_i32(*value)).build();
            b.set_min_size(Size::new(t.px(110), -1));
            theme::adopt(b, Role::Input);
            b.on_value_changed(move |e| report(Value::Int(i64::from(e.get_value()))));
            body.add(&b, 0, SizerFlag::Bottom, t.px(4));
        }
        Control::Text(text) => {
            let b = TextBox::new(sw, text, false);
            b.set_min_size(Size::new(t.px(320), -1));
            b.on_text_updated(move |_| report(Value::Text(b.get_value())));
            body.add(&b, 0, SizerFlag::Bottom, t.px(4));
        }
        Control::Choice { options, selected } => {
            let radios = BoxSizer::builder(Orientation::Horizontal).build();
            for (i, name) in options.iter().enumerate() {
                let style = if i == 0 { RadioButtonStyle::GroupStart } else { RadioButtonStyle::Default };
                let r = RadioButton::adopt(wxdragon::prelude::RadioButton::builder(sw).with_label(name).with_style(style).build());
                r.set_value(i == *selected);
                let report = report.clone();
                r.on_selected(move |_| {
                    if r.get_value() {
                        report(Value::Choice(i));
                    }
                });
                radios.add(&r, 0, SizerFlag::Right, t.px(16));
            }
            body.add_sizer(&radios, 0, SizerFlag::Bottom, t.px(4));
        }
        Control::Dropdown { options, selected } => dropdown(sw, body, options, *selected, report),
        Control::Button(label) => {
            let b = Button::new(sw, label);
            b.on_click(move |_| report(Value::Pressed));
            body.add(&b, 0, SizerFlag::Bottom, t.px(4));
        }
        Control::Custom(c) => {
            let host = ThemedPanel::new(sw);
            host.set_min_size(Size::new(c.size.0, c.size.1));
            (c.build)(&host, on_change);
            body.add(&host, 0, SizerFlag::Bottom, t.px(4));
        }
        Control::Heading | Control::Toggle(_) => {}
    }
}
