//! A declarative dialog: controls at fixed positions, like the Win32 shell's `form`.
//! Dialog definitions are copied from there, so both builds lay out identically.
//!
//! ```ignore
//! let spec = Spec { title: "Go To".into(), ..Spec::default() };
//! if let Some(values) = run(&frame, spec, |form| form.focus(1)) { /* read `values` */ }
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wxdragon::event::WxEvtHandler;
use wxdragon::prelude::*;

mod layout;

use crate::controls::{Button, CheckBox, Dialog, Label, ListView, RadioButton, TextBox};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Label,
    Edit,
    Password,
    /// `true` makes it read-only.
    MultiEdit(bool),
    Check,
    /// `true` starts a new group of radio buttons.
    Radio(bool),
    Group,
    Button,
    /// A push button that Enter presses.
    DefButton,
    /// A small box showing the "#RRGGBB" typed in the edit box whose id is the control text.
    Swatch,
    /// A report-style list (fill it with `Form::list_fill`).
    List,
    /// Like List, but several rows can be selected.
    MultiList,
}

#[derive(Debug)]
pub struct Ctl {
    pub id: usize,
    pub kind: Kind,
    pub text: String,
    pub rect: (i32, i32, i32, i32),
}

pub fn ctl(id: usize, kind: Kind, text: &str, x: i32, y: i32, w: i32, h: i32) -> Ctl {
    Ctl { id, kind, text: text.into(), rect: (x, y, w, h) }
}

#[derive(Clone)]
enum W {
    Label(Label),
    Edit(TextBox),
    Check(CheckBox),
    Radio(RadioButton),
    Button(Button),
    Swatch,
    List(ListView),
    /// A masked box with a plain twin at the same place; "masked" says which one shows.
    Pw(TextBox, TextBox, Rc<std::cell::Cell<bool>>),
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form").finish_non_exhaustive()
    }
}

/// Live access to the controls while the form is open.
#[derive(Clone)]
pub struct Form {
    dialog: Dialog,
    map: Rc<HashMap<usize, W>>,
    ok: Rc<std::cell::Cell<Option<Button>>>,
    cancel: Rc<std::cell::Cell<Option<Button>>>,
}

impl Form {
    pub fn text(&self, id: usize) -> String {
        match self.map.get(&id) {
            Some(W::Edit(t)) => t.get_value(),
            Some(W::Pw(m, p, on)) => {
                if on.get() {
                    m.get_value()
                } else {
                    p.get_value()
                }
            }
            Some(W::Label(l)) => l.get_label(),
            Some(W::Button(b)) => b.get_label(),
            _ => String::new(),
        }
    }

    /// Show the password as dots (true) or as typed (false).
    pub fn set_masked(&self, id: usize, masked: bool) {
        if let Some(W::Pw(m, p, on)) = self.map.get(&id) {
            let text = self.text(id);
            on.set(masked);
            m.set_value(&text);
            p.set_value(&text);
            m.show(masked);
            p.show(!masked);
        }
    }
    pub fn set_text(&self, id: usize, s: &str) {
        match self.map.get(&id) {
            Some(W::Edit(t)) => t.set_value(s),
            Some(W::Pw(m, p, _)) => {
                m.set_value(s);
                p.set_value(s);
            }
            Some(W::Label(l)) => l.set_label(s),
            Some(W::Button(b)) => b.set_label(s),
            _ => {}
        }
    }
    pub fn checked(&self, id: usize) -> bool {
        match self.map.get(&id) {
            Some(W::Check(c)) => c.is_checked(),
            Some(W::Radio(r)) => r.get_value(),
            _ => false,
        }
    }
    pub fn set_checked(&self, id: usize, on: bool) {
        match self.map.get(&id) {
            Some(W::Check(c)) => c.set_value(on),
            Some(W::Radio(r)) => r.set_value(on),
            _ => {}
        }
    }
    pub fn enable(&self, id: usize, on: bool) {
        if id == 1 || id == 2 {
            if let Some(b) = if id == 1 { self.ok.get() } else { self.cancel.get() } {
                b.enable(on);
            }
            return;
        }
        match self.map.get(&id) {
            Some(W::Edit(t)) => {
                t.enable(on);
            }
            Some(W::Check(c)) => {
                c.enable(on);
            }
            Some(W::Radio(r)) => {
                r.enable(on);
            }
            Some(W::Button(b)) => {
                b.enable(on);
            }
            Some(W::Label(l)) => {
                l.enable(on);
            }
            Some(W::Pw(m, p, _)) => {
                m.enable(on);
                p.enable(on);
            }
            Some(W::Swatch) | None => {}
            Some(W::List(l)) => {
                l.enable(on);
            }
        }
    }
    pub fn focus(&self, id: usize) {
        match self.map.get(&id) {
            Some(W::Edit(t)) => t.set_focus(),
            Some(W::Pw(m, p, on)) => {
                if on.get() {
                    m.set_focus();
                } else {
                    p.set_focus();
                }
            }
            _ => {}
        }
    }
    /// Set the columns (once) and replace the rows of a list.
    pub fn list_fill(&self, id: usize, columns: &[(&str, i32)], rows: &[Vec<String>]) {
        let Some(W::List(l)) = self.map.get(&id) else { return };
        if l.get_column_count() == 0 {
            l.set_columns(columns);
        }
        l.fill(rows);
    }
    pub fn list_set_cell(&self, id: usize, row: usize, col: usize, text: &str) {
        if let Some(W::List(l)) = self.map.get(&id) {
            l.set_item_text_by_column(crate::num::to_i64(row), crate::num::to_i32(col), text);
        }
    }
    pub fn list_selected_all(&self, id: usize) -> Vec<usize> {
        match self.map.get(&id) {
            Some(W::List(l)) => l.selected_all(),
            _ => Vec::new(),
        }
    }
    pub fn list_selected(&self, id: usize) -> Option<usize> {
        match self.map.get(&id) {
            Some(W::List(l)) => l.selected(),
            _ => None,
        }
    }
    pub fn list_select(&self, id: usize, row: usize) {
        if let Some(W::List(l)) = self.map.get(&id) {
            let both = ListItemState::Selected | ListItemState::Focused;
            l.set_item_state(crate::num::to_i64(row), both, both);
            l.ensure_visible(crate::num::to_i64(row));
        }
    }

    /// Key presses in edit box `id`; return true to swallow the key.
    pub fn on_key(&self, id: usize, cb: impl Fn(&Form, i32, bool, bool, bool) -> bool + 'static) {
        if let Some(W::Edit(t)) = self.map.get(&id) {
            let f = self.clone();
            t.on_key_down(move |e| {
                if let wxdragon::event::WindowEventData::Keyboard(k) = &e {
                    let code = k.get_key_code().unwrap_or(0);
                    if cb(&f, code, k.control_down(), k.alt_down(), k.shift_down()) {
                        return;
                    }
                }
                e.skip(true);
            });
        }
    }

    pub fn close(&self, ok: bool) {
        self.dialog.end_modal(if ok { ID_OK } else { ID_CANCEL });
    }
}

/// What the user entered, read just before the form closed.
#[derive(Debug)]
pub struct Values {
    text: HashMap<usize, String>,
    checked: HashMap<usize, bool>,
}

impl Values {
    pub fn text(&self, id: usize) -> &str {
        self.text.get(&id).map_or("", String::as_str)
    }
    pub fn checked(&self, id: usize) -> bool {
        self.checked.get(&id).copied().unwrap_or(false)
    }
}

/// Called with the form and the id of the control that changed.
pub type EditHook = fn(&Form, usize);

thread_local! {
    static EDIT_HOOK: std::cell::Cell<Option<EditHook>> = const { std::cell::Cell::new(None) };
}

/// Called with the id of an edit box whenever its text changes, for the next form only.
pub fn on_edit(hook: EditHook) {
    EDIT_HOOK.with(|h| h.set(Some(hook)));
}

impl std::fmt::Debug for Spec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spec").finish_non_exhaustive()
    }
}

/// Positions of the OK and Cancel buttons.
pub type ButtonPositions = ((i32, i32), (i32, i32));

/// Called when OK is pressed with the entered values; returning true keeps the form open.
pub type ApplyHook = Rc<dyn Fn(&Values, &Form) -> bool>;

pub struct Spec<'a> {
    pub title: &'a str,
    pub size: (i32, i32),
    pub ctls: Vec<Ctl>,
    pub ok: &'a str,
    /// Positions of the OK and Cancel buttons; by default they sit at the bottom right.
    pub buttons: Option<ButtonPositions>,
    /// Called after a button, check box or radio button is clicked; gets the control's id.
    pub on_change: Option<fn(&Form, usize)>,
    /// Called when OK is pressed; returning true keeps the form open.
    pub on_apply: Option<ApplyHook>,
}

fn collect(kinds: &[(usize, Kind)], form: &Form) -> Values {
    let mut v = Values { text: HashMap::new(), checked: HashMap::new() };
    for (id, kind) in kinds {
        match kind {
            Kind::Edit | Kind::Password | Kind::MultiEdit(_) => {
                v.text.insert(*id, form.text(*id));
            }
            Kind::Check | Kind::Radio(_) => {
                v.checked.insert(*id, form.checked(*id));
            }
            _ => {}
        }
    }
    v
}

const BTN_W: i32 = 80;
const BTN_H: i32 = 26;

/// Where OK and Cancel go: the spec's own positions, else the bottom right.
fn button_positions(spec: &Spec) -> ButtonPositions {
    let by = spec.size.1 - 36;
    spec.buttons.unwrap_or(((spec.size.0 - 2 * BTN_W - 24, by), (spec.size.0 - BTN_W - 12, by)))
}

/// Layout mistakes in spec, as text (checked in debug builds when a dialog opens).
fn problems(spec: &Spec) -> Vec<String> {
    let placed: Vec<layout::Placed> = spec.ctls.iter().map(|c| layout::Placed { id: c.id, rect: c.rect }).collect();
    let buttons: Vec<(i32, i32, i32, i32)> = if spec.ok.is_empty() {
        Vec::new()
    } else {
        let ((ox, oy), (cx, cy)) = button_positions(spec);
        vec![(ox, oy, BTN_W, BTN_H), (cx, cy, BTN_W, BTN_H)]
    };
    layout::check(spec.size, &placed, &buttons).iter().map(std::string::ToString::to_string).collect()
}

/// Everything created for a spec's controls, for wiring them afterwards.
struct Built {
    map: HashMap<usize, W>,
    kinds: Vec<(usize, Kind)>,
    first_edit: Option<usize>,
    click_ids: Vec<(usize, Button)>,
    swatches: Vec<(Panel, usize)>,
    lists: Vec<(usize, ListView)>,
}

impl Built {
    #[allow(clippy::too_many_lines, reason = "one arm per kind of control")]
    fn add(&mut self, d: &Dialog, c: &Ctl) {
        let (p, s) = (Point::new(c.rect.0, c.rect.1), Size::new(c.rect.2, c.rect.3));
        match c.kind {
            Kind::Group => {
                // A section heading with an etched line under it.
                let (x, y, w, _) = c.rect;
                let _line = StaticLine::builder(d).with_pos(Point::new(x, y + 18)).with_size(Size::new(w, 2)).build();
                let l = Label::adopt(StaticText::builder(d).with_label(&c.text).with_pos(Point::new(x, y)).with_size(Size::new(w, 18)).build());
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
                let t = TextBox::adopt(b.build());
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
                let l = ListView::adopt(ListCtrl::builder(d).with_pos(p).with_size(s).with_style(style).build());
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

fn build_controls(d: &Dialog, ctls: &[Ctl]) -> Built {
    let mut built = Built { map: HashMap::new(), kinds: Vec::new(), first_edit: None, click_ids: Vec::new(), swatches: Vec::new(), lists: Vec::new() };
    for c in ctls {
        built.add(d, c);
    }
    built
}

/// The OK and Cancel buttons (none for a spec without an OK label); Cancel closes the dialog.
fn add_buttons(d: &Dialog, ok_label: &str, ok_at: (i32, i32), cancel_at: (i32, i32)) -> (Option<Button>, Option<Button>) {
    if ok_label.is_empty() {
        return (None, None);
    }
    let size = Size::new(BTN_W, BTN_H);
    let ok = Button::adopt(wxdragon::prelude::Button::builder(d).with_label(ok_label).with_pos(Point::new(ok_at.0, ok_at.1)).with_size(size).build());
    ok.set_default();
    let cancel =
        Button::adopt(wxdragon::prelude::Button::builder(d).with_label("Cancel").with_pos(Point::new(cancel_at.0, cancel_at.1)).with_size(size).build());
    let dialog = *d;
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    (Some(ok), Some(cancel))
}

/// `on_change` for clicks, toggles and list selections; the edit hook for text changes.
fn wire_events(form: &Form, on_change: Option<fn(&Form, usize)>, built: &Built) {
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
fn wire_swatches(form: &Form, swatches: Vec<(Panel, usize)>) {
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

fn report_problems(spec: &Spec) {
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

/// Show the form modally. Returns the values when it was accepted with OK, `None` when cancelled.
pub fn run(owner: &dyn WxWidget, spec: Spec, init: impl FnOnce(&Form)) -> Option<Values> {
    report_problems(&spec);
    let (ok_at, cancel_at) = button_positions(&spec);
    let Spec { title, size, ctls, ok, on_change, on_apply, .. } = spec;
    let d = Dialog::adopt(wxdragon::prelude::Dialog::builder(owner, title).build());
    d.set_client_size(Size::new(size.0, size.1));
    let built = build_controls(&d, &ctls);
    let (ok_btn, cancel_btn) = add_buttons(&d, ok, ok_at, cancel_at);
    d.set_escape_id(ID_CANCEL);
    let form =
        Form { dialog: d, map: Rc::new(built.map.clone()), ok: Rc::new(std::cell::Cell::new(ok_btn)), cancel: Rc::new(std::cell::Cell::new(cancel_btn)) };
    wire_events(&form, on_change, &built);
    let result: Rc<RefCell<Option<Values>>> = Rc::new(RefCell::new(None));
    if let Some(ok) = ok_btn {
        let (f, k, r) = (form.clone(), built.kinds.clone(), result.clone());
        ok.on_click(move |_| {
            let v = collect(&k, &f);
            if let Some(a) = &on_apply
                && a(&v, &f)
            {
                return;
            }
            *r.borrow_mut() = Some(v);
            f.close(true);
        });
    }
    wire_swatches(&form, built.swatches);
    init(&form);
    if let Some(id) = built.first_edit {
        form.focus(id);
    }
    d.centre();
    d.show_modal();
    result.borrow_mut().take()
}

/// Where layout problems found in debug builds are reported besides stderr (the application sets this once).
pub static PROBLEM_SINK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some((crate::num::to_u8(v >> 16), crate::num::to_u8((v >> 8) & 0xFF), crate::num::to_u8(v & 0xFF)))
}
