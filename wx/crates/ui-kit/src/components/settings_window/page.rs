//! One page of the settings window: its title and rows, laid out from the theme tokens.

use std::rc::Rc;

use wxdragon::prelude::*;

use super::model::{Control, Page, Row, Value, parse_number};
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

/// How wide a description wraps, before scaling.
const WRAP: i32 = 560;

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
    col.add(&title, 0, SizerFlag::All, t.px(24));
    for row in &page.rows {
        add_row(&sw, &col, row, on_change);
    }
    col.add_spacer(t.px(24));
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

fn desc(sw: &ScrolledWindow, text: &str) -> Option<Label> {
    if text.is_empty() {
        return None;
    }
    let l = Label::new(sw, text);
    l.wrap(theme::current().px(WRAP));
    Some(l)
}

/// The accent-coloured bar at the left of a row.
fn accent_bar(sw: &ScrolledWindow) -> Panel {
    let bar = Panel::builder(sw).build();
    bar.set_min_size(Size::new(3, -1));
    theme::subscribe_while(move |t| {
        if !bar.is_valid() {
            return false;
        }
        bar.set_background_color(colour(t.colors.accent));
        bar.refresh(true, None);
        true
    });
    bar
}

fn add_row(sw: &ScrolledWindow, col: &BoxSizer, row: &Row, on_change: &Report) {
    let t = theme::current();
    if matches!(row.control, Control::Heading) {
        let h = Label::new(sw, &row.title);
        strong(&h);
        col.add(&h, 0, SizerFlag::Left | SizerFlag::Top | SizerFlag::Right, t.px(24));
        if let Some(d) = desc(sw, &row.desc) {
            col.add(&d, 0, SizerFlag::Left | SizerFlag::Top | SizerFlag::Right, t.px(24));
        }
        col.add_spacer(t.px(4));
        return;
    }
    let line = BoxSizer::builder(Orientation::Horizontal).build();
    line.add(&accent_bar(sw), 0, SizerFlag::Expand, 0);
    line.add_spacer(t.px(14));
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
        body.add(&c, 0, SizerFlag::Bottom, t.px(2));
        add_desc(sw, &body, &row.desc);
    } else {
        let title = Label::new(sw, &row.title);
        strong(&title);
        body.add(&title, 0, SizerFlag::Bottom, t.px(2));
        add_desc(sw, &body, &row.desc);
        add_control(sw, &body, &row.control, report, on_change);
    }
    line.add_sizer(&body, 1, SizerFlag::Expand, 0);
    col.add_sizer(&line, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, t.px(24));
}

fn add_desc(sw: &ScrolledWindow, body: &BoxSizer, text: &str) {
    if let Some(d) = desc(sw, text) {
        body.add(&d, 0, SizerFlag::Bottom, theme::current().px(6));
    }
}

fn add_control(sw: &ScrolledWindow, body: &BoxSizer, control: &Control, report: impl Fn(Value) + Clone + 'static, on_change: &Report) {
    let t = theme::current();
    match control {
        Control::Number { value, min, max } => {
            let (min, max) = (*min, *max);
            let b = TextBox::new(sw, &value.to_string(), false);
            b.set_min_size(Size::new(t.px(90), -1));
            b.on_text_updated(move |_| {
                if let Some(n) = parse_number(&b.get_value(), min, max) {
                    report(Value::Int(n));
                }
            });
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
