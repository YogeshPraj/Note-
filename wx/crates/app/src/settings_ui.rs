//! The Style Configurator and the font picker, plus applying new settings.

use notepp_core::sci::Theme;
use notepp_core::settings::{self, Settings};
use notepp_core::workspace::Workspace;
use wxdragon::prelude::*;

use crate::sci::WxSci;

/// Take new settings: the workspace restyles its views, and they are saved.
pub fn apply_settings(ws: &mut Workspace<WxSci>, s: Settings) {
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    ws.set_settings(s);
    let _ = ws.settings.save();
    ws.set_margins_visible(true);
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

pub struct StyleEdit {
    pub reset: bool,
    pub entered: Vec<(&'static str, String)>,
}

/// Settings > Style Configurator: the colours of the theme in use, as #RRGGBB. Laid out like the Win32 shell's.
pub fn style_configurator(frame: &Frame, theme: &Theme) -> Option<StyleEdit> {
    use crate::widgets::form::{Kind, Spec, ctl, run};
    const FIRST: usize = 100; // edit i is FIRST + i
    const PICK: usize = 200; // button i is PICK + i
    const RESET: usize = 300;
    thread_local! {
        static OWNER: std::cell::RefCell<Option<Frame>> = const { std::cell::RefCell::new(None) };
    }
    OWNER.with(|o| *o.borrow_mut() = Some(*frame));
    fn on_click(f: &crate::widgets::form::Form, id: usize) {
        if (PICK..PICK + settings::ROLES.len()).contains(&id) {
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
    let mut ctls =
        vec![ctl(10, Kind::Label, if theme.dark { "Colours of the dark theme (#RRGGBB):" } else { "Colours of the light theme (#RRGGBB):" }, 16, 12, 300, 18)];
    for (i, (label, key)) in settings::ROLES.iter().enumerate() {
        let y = 40 + crate::platform::wx::to_i32(i) * 26;
        let hex = settings::color_of(theme, key).map(settings::to_hex).unwrap_or_default();
        ctls.push(ctl(400 + i, Kind::Label, label, 16, y + 3, 170, 18));
        ctls.push(ctl(FIRST + i, Kind::Edit, &hex, 190, y, 84, 22));
        ctls.push(ctl(500 + i, Kind::Swatch, &(FIRST + i).to_string(), 282, y, 28, 22));
        ctls.push(ctl(PICK + i, Kind::Button, "...", 316, y - 1, 30, 24));
    }
    let n = crate::platform::wx::to_i32(settings::ROLES.len());
    ctls.push(ctl(RESET, Kind::Check, "Reset every colour to the default", 16, 40 + n * 26 + 6, 290, 20));
    let spec = Spec { title: "Style Configurator", size: (366, 40 + n * 26 + 78), ctls, ok: "OK", buttons: None, on_change: Some(on_click), on_apply: None };
    let v = run(frame, spec, |_| {})?;
    Some(StyleEdit {
        reset: v.checked(RESET),
        entered: settings::ROLES.iter().enumerate().map(|(i, (_, key))| (*key, v.text(FIRST + i).to_string())).collect(),
    })
}
