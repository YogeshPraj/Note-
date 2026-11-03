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
    /// The dialog a form runs in; none for a form embedded in a page.
    dialog: Option<Dialog>,
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

    /// Is the form's dialog still up (false once it is closing)? An embedded form always is.
    pub fn is_shown(&self) -> bool {
        self.dialog.is_none_or(|d| d.is_shown())
    }

    pub fn close(&self, ok: bool) {
        if let Some(d) = self.dialog {
            d.end_modal(if ok { ID_OK } else { ID_CANCEL });
        }
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

mod build;
use build::*;

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
    d.close_on_escape();
    let form =
        Form { dialog: Some(d), map: Rc::new(built.map.clone()), ok: Rc::new(std::cell::Cell::new(ok_btn)), cancel: Rc::new(std::cell::Cell::new(cancel_btn)) };
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

/// Build the form's controls into `parent` (a page of the settings window) instead of a dialog of their own. There is no
/// OK button: the hooks in `spec` act on every change, and the caller applies the result as it happens.
pub fn embed(parent: &impl WxWidget, spec: Spec, init: impl FnOnce(&Form)) -> Form {
    let Spec { ctls, on_change, .. } = spec;
    let built = build_controls(parent, &ctls);
    let form = Form { dialog: None, map: Rc::new(built.map.clone()), ok: Rc::new(std::cell::Cell::new(None)), cancel: Rc::new(std::cell::Cell::new(None)) };
    wire_events(&form, on_change, &built);
    wire_swatches(&form, built.swatches);
    init(&form);
    form
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
