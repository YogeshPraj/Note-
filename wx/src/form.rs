//! A declarative dialog: controls at fixed positions, like the Win32 shell's `form`.
//! Dialog definitions are copied from there, so both builds lay out identically.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wxdragon::event::WxEvtHandler;
use wxdragon::prelude::*;

#[derive(Clone, Copy, PartialEq)]
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
    /// A report-style list (fill it with Form::list_fill).
    List,
    /// Like List, but several rows can be selected.
    MultiList,
}

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
    Label(StaticText),
    Edit(TextCtrl),
    Check(CheckBox),
    Radio(RadioButton),
    Button(Button),
    Swatch(Panel),
    List(ListCtrl),
}

/// Live access to the controls while the form is open.
#[derive(Clone)]
pub struct Form {
    dialog: Dialog,
    map: Rc<HashMap<usize, W>>,
}

impl Form {
    pub fn text(&self, id: usize) -> String {
        match self.map.get(&id) {
            Some(W::Edit(t)) => t.get_value(),
            Some(W::Label(l)) => l.get_label(),
            Some(W::Button(b)) => b.get_label(),
            _ => String::new(),
        }
    }
    pub fn set_text(&self, id: usize, s: &str) {
        match self.map.get(&id) {
            Some(W::Edit(t)) => t.set_value(s),
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
            Some(W::Swatch(_)) | None => {}
            Some(W::List(l)) => {
                l.enable(on);
            }
        }
    }
    pub fn focus(&self, id: usize) {
        if let Some(W::Edit(t)) = self.map.get(&id) {
            t.set_focus();
        }
    }
    /// Replace the columns (once) and rows of a list.
    pub fn list_fill(&self, id: usize, columns: &[(&str, i32)], rows: &[Vec<String>]) {
        let Some(W::List(l)) = self.map.get(&id) else { return };
        if l.get_column_count() == 0 {
            for (i, (name, w)) in columns.iter().enumerate() {
                l.insert_column(i as i64, name, ListColumnFormat::Left, *w);
            }
        }
        l.delete_all_items();
        for (r, row) in rows.iter().enumerate() {
            l.insert_item(r as i64, row.first().map_or("", String::as_str), None);
            for (c, cell) in row.iter().enumerate().skip(1) {
                l.set_item_text_by_column(r as i64, c as i32, cell);
            }
        }
    }
    pub fn list_set_cell(&self, id: usize, row: usize, col: usize, text: &str) {
        if let Some(W::List(l)) = self.map.get(&id) {
            l.set_item_text_by_column(row as i64, col as i32, text);
        }
    }
    pub fn list_selected_all(&self, id: usize) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(W::List(l)) = self.map.get(&id) {
            let mut i = -1i64;
            loop {
                let n = l.get_next_item(i, ListNextItemFlag::All, ListItemState::Selected);
                if n < 0 {
                    break;
                }
                out.push(n as usize);
                i = n as i64;
            }
        }
        out
    }
    pub fn list_selected(&self, id: usize) -> Option<usize> {
        match self.map.get(&id) {
            Some(W::List(l)) => usize::try_from(l.get_first_selected_item()).ok(),
            _ => None,
        }
    }
    pub fn list_select(&self, id: usize, row: usize) {
        if let Some(W::List(l)) = self.map.get(&id) {
            let both = ListItemState::Selected | ListItemState::Focused;
            l.set_item_state(row as i64, both, both);
            l.ensure_visible(row as i64);
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
pub struct Values {
    text: HashMap<usize, String>,
    checked: HashMap<usize, bool>,
}

impl Values {
    pub fn text(&self, id: usize) -> &str {
        self.text.get(&id).map(String::as_str).unwrap_or("")
    }
    pub fn checked(&self, id: usize) -> bool {
        self.checked.get(&id).copied().unwrap_or(false)
    }
}

thread_local! {
    static EDIT_HOOK: std::cell::Cell<Option<fn(&Form, usize)>> = const { std::cell::Cell::new(None) };
}

/// Called with the id of an edit box whenever its text changes, for the next form only.
pub fn on_edit(hook: fn(&Form, usize)) {
    EDIT_HOOK.with(|h| h.set(Some(hook)));
}

pub struct Spec<'a> {
    pub title: &'a str,
    pub size: (i32, i32),
    pub ctls: Vec<Ctl>,
    pub ok: &'a str,
    /// Positions of the OK and Cancel buttons; by default they sit at the bottom right.
    pub buttons: Option<((i32, i32), (i32, i32))>,
    /// Called after a button, check box or radio button is clicked; gets the control's id.
    pub on_change: Option<fn(&Form, usize)>,
    /// Called when OK is pressed; returning true keeps the form open.
    pub on_apply: Option<Rc<dyn Fn(&Values, &Form) -> bool>>,
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

pub fn run(owner: &dyn WxWidget, spec: Spec, init: impl FnOnce(&Form)) -> Option<Values> {
    let d = Dialog::builder(owner, spec.title).build();
    d.set_client_size(Size::new(spec.size.0, spec.size.1));
    let pos = |r: (i32, i32, i32, i32)| (Point::new(r.0, r.1), Size::new(r.2, r.3));
    let mut map: HashMap<usize, W> = HashMap::new();
    let mut kinds = Vec::new();
    let mut first_edit = None;
    let mut click_ids: Vec<(usize, Button)> = Vec::new();
    let mut swatches: Vec<(Panel, usize)> = Vec::new();
    let mut lists: Vec<(usize, ListCtrl)> = Vec::new();
    for c in &spec.ctls {
        let (p, s) = pos(c.rect);
        match c.kind {
            Kind::Group => {
                // A section heading with an etched line under it.
                let (x, y, w, _) = c.rect;
                let _line = StaticLine::builder(&d).with_pos(Point::new(x, y + 18)).with_size(Size::new(w, 2)).build();
                let l = StaticText::builder(&d).with_label(&c.text).with_pos(Point::new(x, y)).with_size(Size::new(w, 18)).build();
                map.insert(c.id, W::Label(l));
            }
            Kind::Label => {
                let l = StaticText::builder(&d).with_label(&c.text).with_pos(p).with_size(s).build();
                map.insert(c.id, W::Label(l));
            }
            Kind::Edit | Kind::Password | Kind::MultiEdit(_) => {
                let mut b = TextCtrl::builder(&d).with_value(&c.text).with_pos(p).with_size(s);
                b = match c.kind {
                    Kind::Password => b.with_style(TextCtrlStyle::Password),
                    Kind::MultiEdit(true) => b.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly),
                    Kind::MultiEdit(false) => b.with_style(TextCtrlStyle::MultiLine),
                    _ => b,
                };
                let t = b.build();
                first_edit.get_or_insert(c.id);
                map.insert(c.id, W::Edit(t));
                kinds.push((c.id, c.kind));
            }
            Kind::Check => {
                let ch = CheckBox::builder(&d).with_label(&c.text).with_pos(p).with_size(s).build();
                map.insert(c.id, W::Check(ch));
                kinds.push((c.id, c.kind));
            }
            Kind::Radio(group) => {
                let mut b = RadioButton::builder(&d).with_label(&c.text).with_pos(p).with_size(s);
                if group {
                    b = b.with_style(RadioButtonStyle::GroupStart);
                }
                let r = b.build();
                map.insert(c.id, W::Radio(r));
                kinds.push((c.id, c.kind));
            }
            Kind::Swatch => {
                let sw = Panel::builder(&d).with_pos(p).with_size(s).with_style(PanelStyle::BorderSimple).build();
                if let Ok(edit) = c.text.parse::<usize>() {
                    swatches.push((sw, edit));
                }
                map.insert(c.id, W::Swatch(sw));
            }
            Kind::List | Kind::MultiList => {
                let style = if c.kind == Kind::List { ListCtrlStyle::Report | ListCtrlStyle::SingleSel } else { ListCtrlStyle::Report };
                let l = ListCtrl::builder(&d).with_pos(p).with_size(s).with_style(style).build();
                lists.push((c.id, l));
                map.insert(c.id, W::List(l));
            }
            Kind::Button | Kind::DefButton => {
                let b = Button::builder(&d).with_label(&c.text).with_pos(p).with_size(s).build();
                if c.kind == Kind::DefButton {
                    b.set_default();
                }
                click_ids.push((c.id, b));
                map.insert(c.id, W::Button(b));
            }
        }
    }
    let (bw, bh, by) = (80, 26, spec.size.1 - 36);
    let ((okx, oky), (cx, cy)) = spec.buttons.unwrap_or(((spec.size.0 - 2 * bw - 24, by), (spec.size.0 - bw - 12, by)));
    let mut ok_btn = None;
    if !spec.ok.is_empty() {
        let ok = Button::builder(&d).with_label(spec.ok).with_pos(Point::new(okx, oky)).with_size(Size::new(bw, bh)).build();
        ok.set_default();
        let cancel = Button::builder(&d).with_label("Cancel").with_pos(Point::new(cx, cy)).with_size(Size::new(bw, bh)).build();
        cancel.on_click(move |_| d.end_modal(ID_CANCEL));
        ok_btn = Some(ok);
    }
    d.set_escape_id(ID_CANCEL);

    let form = Form { dialog: d, map: Rc::new(map) };
    if let Some(on) = spec.on_change {
        for (id, b) in &click_ids {
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
    }
    if let Some(on) = spec.on_change {
        for (id, l) in &lists {
            let (f, id) = (form.clone(), *id);
            l.on_item_selected(move |_| on(&f, id));
        }
    }
    if let Some(hook) = EDIT_HOOK.with(|h| h.replace(None)) {
        for (id, w) in form.map.iter() {
            if let W::Edit(t) = w {
                let (f, id) = (form.clone(), *id);
                t.on_text_updated(move |_| hook(&f, id));
            }
        }
    }
    let result: Rc<RefCell<Option<Values>>> = Rc::new(RefCell::new(None));
    if let Some(ok) = ok_btn {
        let (f, k, r) = (form.clone(), kinds.clone(), result.clone());
        let apply = spec.on_apply.clone();
        ok.on_click(move |_| {
            let v = collect(&k, &f);
            if let Some(a) = &apply {
                if a(&v, &f) {
                    return;
                }
            }
            *r.borrow_mut() = Some(v);
            f.close(true);
        });
    }
    for (sw, edit) in swatches {
        let paint = move |hex: &str| {
            if let Some(c) = notepp_core::settings::parse_hex(hex) {
                sw.set_background_color(Colour::rgb(c.0, c.1, c.2));
                sw.refresh(true, None);
            }
        };
        if let Some(W::Edit(t)) = form.map.get(&edit) {
            paint(&t.get_value());
            let t2 = *t;
            t.on_text_updated(move |_| paint(&t2.get_value()));
        }
    }
    init(&form);
    if let Some(id) = first_edit {
        form.focus(id);
    }
    d.centre();
    d.show_modal();
    let out = result.borrow_mut().take();
    out
}
