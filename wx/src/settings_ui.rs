//! Settings dialogs: Preferences and the Style Configurator, plus applying new settings.

use notepp_core::sci::Theme;
use notepp_core::settings::{self, Settings};
use notepp_core::workspace::Workspace;
use wxdragon::prelude::*;

use crate::sci::WxSci;

/// Take new settings: the workspace restyles its views, and they are saved.
pub fn apply_settings(ws: &mut Workspace<WxSci>, s: Settings) {
    notepp_core::bitwarden::set_auto_lock_minutes(s.passwords_auto_lock_min);
    ws.set_settings(s);
    let _ = ws.settings.save();
    ws.set_margins_visible(true);
}

fn row(parent: &Dialog, label: &str, control: &impl WxWidget) -> BoxSizer {
    let r = BoxSizer::builder(Orientation::Horizontal).build();
    let l = StaticText::builder(parent).with_label(label).build();
    r.add(&l, 1, SizerFlag::AlignCenterVertical | SizerFlag::All, 4);
    r.add(control, 1, SizerFlag::Expand | SizerFlag::All, 4);
    r
}

/// Settings > Preferences. Returns the edited settings on OK.
pub fn preferences(frame: &Frame, s: &Settings) -> Option<Settings> {
    let d = Dialog::builder(frame, "Preferences").with_size(480, 640).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let font = TextCtrl::builder(&d).build();
    font.set_value(&s.font);
    let size = SpinCtrl::builder(&d).with_range(6, 72).build();
    size.set_value(s.font_size);
    let tab = SpinCtrl::builder(&d).with_range(1, 16).build();
    tab.set_value(s.tab_width);
    let edge = SpinCtrl::builder(&d).with_range(0, 500).build();
    edge.set_value(s.edge_column);
    let lock = SpinCtrl::builder(&d).with_range(0, 1440).build();
    lock.set_value(s.passwords_auto_lock_min as i32);
    root.add_sizer(&row(&d, "Font:", &font), 0, SizerFlag::Expand, 0);
    root.add_sizer(&row(&d, "Font size:", &size), 0, SizerFlag::Expand, 0);
    root.add_sizer(&row(&d, "Tab width:", &tab), 0, SizerFlag::Expand, 0);
    root.add_sizer(&row(&d, "Right edge column (0 = off):", &edge), 0, SizerFlag::Expand, 0);
    root.add_sizer(&row(&d, "Lock Passwords after minutes idle (0 = never):", &lock), 0, SizerFlag::Expand, 0);

    let check = |text: &str, on: bool| {
        let c = CheckBox::builder(&d).with_label(text).build();
        c.set_value(on);
        root.add(&c, 0, SizerFlag::All, 4);
        c
    };
    let spaces = check("Insert spaces instead of tab characters", s.use_spaces);
    let numbers = check("Show line numbers", s.line_numbers);
    let fold = check("Show the fold margin", s.fold_margin);
    let bookmarks = check("Show the bookmark margin", s.bookmark_margin);
    let caret = check("Highlight the current line", s.caret_line);
    let smart = check("Highlight other occurrences of the selected word", s.smart_highlight);
    let md_bar = check("Show the Markdown formatting bar", s.md_toolbar);
    let remote = check("Let previews load images from the web (restart to apply)", s.preview_remote_images);
    let updates = check("Check for updates automatically", s.auto_update);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&d).with_label("OK").build();
    let cancel = Button::builder(&d).with_label("Cancel").build();
    buttons.add(&ok, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::AlignRight, 0);
    d.set_sizer(root, true);
    ok.on_click(move |_| d.end_modal(ID_OK));
    cancel.on_click(move |_| d.end_modal(ID_CANCEL));

    if d.show_modal() != ID_OK {
        return None;
    }
    let mut n = s.clone();
    n.font = font.get_value().trim().to_string();
    n.font_size = size.value();
    n.tab_width = tab.value();
    n.edge_column = edge.value();
    n.passwords_auto_lock_min = lock.value().max(0) as u32;
    n.use_spaces = spaces.is_checked();
    n.line_numbers = numbers.is_checked();
    n.fold_margin = fold.is_checked();
    n.bookmark_margin = bookmarks.is_checked();
    n.caret_line = caret.is_checked();
    n.smart_highlight = smart.is_checked();
    n.md_toolbar = md_bar.is_checked();
    n.preview_remote_images = remote.is_checked();
    n.auto_update = updates.is_checked();
    n.clamp();
    Some(n)
}

pub struct StyleEdit {
    pub reset: bool,
    pub entered: Vec<(&'static str, String)>,
}

/// Settings > Style Configurator: the colours of the theme in use, as #RRGGBB.
pub fn style_configurator(frame: &Frame, theme: &Theme) -> Option<StyleEdit> {
    let d = Dialog::builder(frame, if theme.dark { "Style Configurator - dark theme" } else { "Style Configurator - light theme" }).with_size(420, 640).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let mut boxes = Vec::new();
    for (label, key) in settings::ROLES {
        let hex = settings::color_of(theme, key).map(settings::to_hex).unwrap_or_default();
        let t = TextCtrl::builder(&d).build();
        t.set_value(&hex);
        let r = BoxSizer::builder(Orientation::Horizontal).build();
        let l = StaticText::builder(&d).with_label(label).build();
        r.add(&l, 1, SizerFlag::AlignCenterVertical | SizerFlag::All, 3);
        r.add(&t, 0, SizerFlag::All, 3);
        let pick = Button::builder(&d).with_label("...").build();
        r.add(&pick, 0, SizerFlag::All, 3);
        root.add_sizer(&r, 0, SizerFlag::Expand, 0);
        pick.on_click(move |_| {
            let start = settings::parse_hex(&t.get_value()).unwrap_or((0, 0, 0));
            let cd = ColourDialog::builder(&d).with_initial_colour(Colour::new(start.0, start.1, start.2, 255)).build();
            if cd.show_modal() == ID_OK {
                if let Some(c) = cd.get_colour() {
                    t.set_value(&settings::to_hex((c.r, c.g, c.b)));
                }
            }
        });
        boxes.push((*key, t));
    }
    let reset = CheckBox::builder(&d).with_label("Reset every colour to the default").build();
    root.add(&reset, 0, SizerFlag::All, 6);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&d).with_label("OK").build();
    let cancel = Button::builder(&d).with_label("Cancel").build();
    buttons.add(&ok, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::AlignRight, 0);
    d.set_sizer(root, true);
    ok.on_click(move |_| d.end_modal(ID_OK));
    cancel.on_click(move |_| d.end_modal(ID_CANCEL));
    if d.show_modal() != ID_OK {
        return None;
    }
    Some(StyleEdit { reset: reset.is_checked(), entered: boxes.into_iter().map(|(k, t)| (k, t.get_value())).collect() })
}
