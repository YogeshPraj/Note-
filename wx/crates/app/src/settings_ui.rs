//! The Style Configurator and the font picker, plus applying new settings.

use notepp_core::sci::Theme;
use notepp_core::settings::{self, Settings};
use wxdragon::prelude::*;

/// Take new settings: the workspace restyles its views, and they are saved. Distraction Free mode keeps its margins
/// hidden; a save that fails is said on the status bar (the change still holds until the program closes).
pub fn apply_settings(app: &mut crate::app::App, s: Settings) {
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    app.ws.set_settings(s);
    app.ws.set_margins_visible(!app.distraction);
    if let Err(e) = app.ws.settings.save() {
        app.flash = Some(format!("Settings could not be saved: {e}"));
    }
}

pub(crate) fn pick_font(owner: &Frame, family: &str, size: i32) -> Option<(String, i32)> {
    let fd = FontDialog::builder(owner).build();
    let _ = (family, size);
    if fd.show_modal() != ID_OK {
        return None;
    }
    let f = fd.get_font()?;
    Some((f.get_face_name(), f.get_point_size()))
}

/// Told the colours as they are now.
pub type StyleApply = std::rc::Rc<dyn Fn(StyleEdit)>;

pub struct StyleEdit {
    pub reset: bool,
    pub entered: Vec<(&'static str, String)>,
}

/// The size of the colour editor's area in the settings window, for a theme with `roles` colours.
pub fn colours_size(roles: usize) -> (i32, i32) {
    (720, crate::platform::wx::to_i32(roles.div_ceil(2)) * 26 + 44)
}

const FIRST: usize = 100; // edit i is FIRST + i
const PICK: usize = 200; // button i is PICK + i
const RESET: usize = 300;

thread_local! {
    static OWNER: std::cell::RefCell<Option<Frame>> = const { std::cell::RefCell::new(None) };
    static APPLY: std::cell::RefCell<Option<StyleApply>> = const { std::cell::RefCell::new(None) };
    static RESET_ALL: std::cell::RefCell<Option<std::rc::Rc<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}

/// Tell the owner the colours as they are now (only once every box holds a valid colour, so half-typed text never applies).
fn send_colours(f: &crate::widgets::form::Form) {
    let entered: Vec<(&'static str, String)> = settings::ROLES.iter().enumerate().map(|(i, (_, key))| (*key, f.text(FIRST + i))).collect();
    if entered.iter().any(|(_, h)| settings::parse_hex(h).is_none()) {
        return;
    }
    if let Some(apply) = APPLY.with(|a| a.borrow().clone()) {
        apply(StyleEdit { reset: false, entered });
    }
}

fn colour_clicked(f: &crate::widgets::form::Form, id: usize) {
    if id == RESET {
        if let Some(reset) = RESET_ALL.with(|r| r.borrow().clone()) {
            reset();
        }
    } else if (PICK..PICK + settings::ROLES.len()).contains(&id) {
        let edit = FIRST + (id - PICK);
        let start = settings::parse_hex(&f.text(edit)).unwrap_or((0, 0, 0));
        let Some(owner) = OWNER.with(|o| *o.borrow()) else { return };
        let cd = ColourDialog::builder(&owner).with_initial_colour(crate::widgets::paint::colour(start)).build();
        if cd.show_modal() == ID_OK
            && let Some(c) = cd.get_colour()
        {
            f.set_text(edit, &settings::to_hex((c.r, c.g, c.b)));
        }
    }
}

/// The colours of the theme in use (#RRGGBB), built into a settings page. `apply` gets the colours whenever they change;
/// `reset` restores every default.
pub fn embed_colours(parent: &impl WxWidget, owner: &Frame, theme: &Theme, apply: std::rc::Rc<dyn Fn(StyleEdit)>, reset: std::rc::Rc<dyn Fn()>) {
    use crate::widgets::form::{Kind, Spec, ctl, embed, on_edit};
    OWNER.with(|o| *o.borrow_mut() = Some(*owner));
    APPLY.with(|a| *a.borrow_mut() = Some(apply));
    RESET_ALL.with(|r| *r.borrow_mut() = Some(reset));
    on_edit(|f, id| {
        if (FIRST..FIRST + settings::ROLES.len()).contains(&id) {
            send_colours(f);
        }
    });
    let mut ctls = Vec::new();
    for (i, (label, key)) in settings::ROLES.iter().enumerate() {
        // Two columns, filled down the left first.
        let half = settings::ROLES.len().div_ceil(2);
        let (y, x) = (crate::platform::wx::to_i32(i % half) * 26, if i < half { 0 } else { 360 });
        let hex = settings::color_of(theme, key).map(settings::to_hex).unwrap_or_default();
        ctls.push(ctl(400 + i, Kind::Label, label, x, y + 3, 170, 18));
        ctls.push(ctl(FIRST + i, Kind::Edit, &hex, x + 174, y, 84, 22));
        ctls.push(ctl(500 + i, Kind::Swatch, &(FIRST + i).to_string(), x + 266, y, 28, 22));
        ctls.push(ctl(PICK + i, Kind::Button, "...", x + 300, y - 1, 30, 24));
    }
    let rows = crate::platform::wx::to_i32(settings::ROLES.len().div_ceil(2));
    ctls.push(ctl(RESET, Kind::Button, "Reset every colour to the default", 0, rows * 26 + 8, 220, 26));
    let spec =
        Spec { title: "Colours", size: colours_size(settings::ROLES.len()), ctls, ok: "", buttons: None, on_change: Some(colour_clicked), on_apply: None };
    embed(parent, spec, |_| {});
}
