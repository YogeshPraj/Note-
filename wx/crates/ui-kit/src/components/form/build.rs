//! Building a spec's controls and buttons, and wiring their events.

use super::*;

/// `v` pixels at 100%, on this display.
pub(super) fn px(v: i32) -> i32 {
    crate::theme::current().px(v)
}

/// A spec rectangle (at 100%) on this display.
fn scaled((x, y, w, h): (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    (px(x), px(y), px(w), px(h))
}

/// Everything created for a spec's controls, for wiring them afterwards.
pub(super) struct Built {
    pub(super) map: HashMap<usize, W>,
    pub(super) kinds: Vec<(usize, Kind)>,
    pub(super) first_edit: Option<usize>,
    pub(super) click_ids: Vec<(usize, Button)>,
    pub(super) swatches: Vec<(Panel, usize)>,
    pub(super) lists: Vec<(usize, ListView)>,
}

impl Built {
    #[allow(clippy::too_many_lines, reason = "one arm per kind of control")]
    fn add(&mut self, d: &impl WxWidget, c: &Ctl) {
        // Specs are written at 100%; they are placed scaled to the display.
        let (x, y, w, h) = scaled(c.rect);
        let (p, s) = (Point::new(x, y), Size::new(w, h));
        match c.kind {
            Kind::Group => {
                // A section heading with an etched line under it.
                let (under, line) = (px(18), px(2));
                let _line = StaticLine::builder(d).with_pos(Point::new(x, y + under)).with_size(Size::new(w, line)).build();
                let l = Label::adopt(StaticText::builder(d).with_label(&c.text).with_pos(Point::new(x, y)).with_size(Size::new(w, under)).build());
                self.map.insert(c.id, W::Label(l));
            }
            Kind::Label => {
                let l = Label::adopt(StaticText::builder(d).with_label(&c.text).with_pos(p).with_size(s).build());
                self.map.insert(c.id, W::Label(l));
            }
            Kind::Password => {
                let m = TextBox::adopt(TextCtrl::builder(d).with_value(&c.text).with_pos(p).with_size(s).with_style(TextCtrlStyle::Password).build());
                let pl = TextBox::adopt(TextCtrl::builder(d).with_value(&c.text).with_pos(p).with_size(s).build());
                pl.show(false);
                self.first_edit.get_or_insert(c.id);
                self.map.insert(c.id, W::Pw(m, pl, Rc::new(std::cell::Cell::new(true))));
                self.kinds.push((c.id, c.kind));
            }
            Kind::Edit | Kind::MultiEdit(_) => {
                let mut b = TextCtrl::builder(d).with_value(&c.text).with_pos(p).with_size(s);
                b = match c.kind {
                    Kind::MultiEdit(true) => b.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly),
                    Kind::MultiEdit(false) => b.with_style(TextCtrlStyle::MultiLine),
                    _ => b,
                };
                let raw = b.build();
                if c.kind == Kind::Edit {
                    crate::theme::drop_border(&raw);
                }
                let t = TextBox::adopt(raw);
                self.first_edit.get_or_insert(c.id);
                self.map.insert(c.id, W::Edit(t));
                self.kinds.push((c.id, c.kind));
            }
            Kind::Check => {
                let ch = CheckBox::adopt(wxdragon::prelude::CheckBox::builder(d).with_label(&c.text).with_pos(p).with_size(s).build());
                self.map.insert(c.id, W::Check(ch));
                self.kinds.push((c.id, c.kind));
            }
            Kind::Radio(group) => {
                let mut b = wxdragon::prelude::RadioButton::builder(d).with_label(&c.text).with_pos(p).with_size(s);
                if group {
                    b = b.with_style(RadioButtonStyle::GroupStart);
                }
                let r = RadioButton::adopt(b.build());
                self.map.insert(c.id, W::Radio(r));
                self.kinds.push((c.id, c.kind));
            }
            Kind::Swatch => {
                let sw = Panel::builder(d).with_pos(p).with_size(s).with_style(PanelStyle::BorderSimple).build();
                if let Ok(edit) = c.text.parse::<usize>() {
                    self.swatches.push((sw, edit));
                }
                self.map.insert(c.id, W::Swatch);
            }
            Kind::List | Kind::MultiList => {
                let style = if c.kind == Kind::List { ListCtrlStyle::Report | ListCtrlStyle::SingleSel } else { ListCtrlStyle::Report };
                let raw = ListCtrl::builder(d).with_pos(p).with_size(s).with_style(style).build();
                crate::theme::drop_border(&raw);
                let l = ListView::adopt(raw);
                self.lists.push((c.id, l));
                self.map.insert(c.id, W::List(l));
            }
            Kind::Button | Kind::DefButton => {
                let b = Button::adopt(wxdragon::prelude::Button::builder(d).with_label(&c.text).with_pos(p).with_size(s).build());
                if c.kind == Kind::DefButton {
                    b.set_default();
                }
                self.click_ids.push((c.id, b));
                self.map.insert(c.id, W::Button(b));
            }
        }
    }
}

pub(super) fn build_controls(d: &impl WxWidget, ctls: &[Ctl]) -> Built {
    let mut built = Built { map: HashMap::new(), kinds: Vec::new(), first_edit: None, click_ids: Vec::new(), swatches: Vec::new(), lists: Vec::new() };
    for c in ctls {
        built.add(d, c);
    }
    built
}

/// The OK and Cancel buttons (none for a spec without an OK label); Cancel closes the dialog.
pub(super) fn add_buttons(d: &Dialog, ok_label: &str, ok_at: (i32, i32), cancel_at: (i32, i32)) -> (Option<Button>, Option<Button>) {
    if ok_label.is_empty() {
        return (None, None);
    }
    let size = Size::new(px(BTN_W), px(BTN_H));
    let (ok_at, cancel_at) = (Point::new(px(ok_at.0), px(ok_at.1)), Point::new(px(cancel_at.0), px(cancel_at.1)));
    let ok = Button::adopt(wxdragon::prelude::Button::builder(d).with_label(ok_label).with_pos(ok_at).with_size(size).build());
    ok.set_default();
    let cancel = Button::adopt(wxdragon::prelude::Button::builder(d).with_label("Cancel").with_pos(cancel_at).with_size(size).build());
    let dialog = *d;
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    (Some(ok), Some(cancel))
}

/// `on_change` for clicks, toggles and list selections; the edit hook for text changes.
pub(super) fn wire_events(form: &Form, on_change: Option<fn(&Form, usize)>, built: &Built) {
    if let Some(on) = on_change {
        for (id, b) in &built.click_ids {
            let (f, id) = (form.clone(), *id);
            b.on_click(move |_| on(&f, id));
        }
        for (id, w) in form.map.iter() {
            let (f, id) = (form.clone(), *id);
            match w {
                W::Check(c) => {
                    c.on_toggled(move |_| on(&f, id));
                }
                W::Radio(r) => {
                    r.bind_internal(EventType::COMMAND_RADIOBUTTON_SELECTED, move |_| on(&f, id));
                }
                _ => {}
            }
        }
        for (id, l) in &built.lists {
            let (f, id) = (form.clone(), *id);
            l.on_item_selected(move |_| on(&f, id));
        }
    }
    if let Some(hook) = EDIT_HOOK.with(|h| h.replace(None)) {
        for (id, w) in form.map.iter() {
            match w {
                W::Edit(t) => {
                    let (f, id) = (form.clone(), *id);
                    t.on_text_updated(move |_| hook(&f, id));
                }
                W::Pw(m, p, _) => {
                    for t in [m, p] {
                        let (f, id) = (form.clone(), *id);
                        t.on_text_updated(move |_| hook(&f, id));
                    }
                }
                _ => {}
            }
        }
    }
}

/// A swatch shows the colour typed in its edit box.
pub(super) fn wire_swatches(form: &Form, swatches: Vec<(Panel, usize)>) {
    for (sw, edit) in swatches {
        let paint = move |hex: &str| {
            if let Some(c) = parse_hex(hex) {
                sw.set_background_color(crate::paint::colour(c));
                sw.refresh(true, None);
            }
        };
        if let Some(W::Edit(t)) = form.map.get(&edit) {
            paint(&t.get_value());
            let t2 = *t;
            t.on_text_updated(move |_| paint(&t2.get_value()));
        }
    }
}

pub(super) fn report_problems(spec: &Spec) {
    if cfg!(debug_assertions) {
        for p in problems(spec) {
            let line = format!("dialog '{}': {p}", spec.title);
            eprintln!("{line}");
            if let Some(sink) = PROBLEM_SINK.get() {
                sink(&line);
            }
        }
    }
}
