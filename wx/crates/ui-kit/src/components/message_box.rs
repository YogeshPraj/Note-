//! A message or question in a themed dialog: an icon, the text (wrapped), and buttons on the right with the primary
//! one rightmost. Enter presses the default button, Esc the cancelling one. It follows light and dark themes, which
//! the system message box does not. Reached through [`crate::ask`].

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::ask::Icon;
use crate::controls::{Button, Dialog, Label};
use crate::geom::{Rect, Rgb};
use crate::paint::{colour, text_at};
use crate::theme;

/// Width the text wraps at, before scaling.
const WRAP: i32 = 400;
/// Size of the round icon, before scaling.
const GLYPH: i32 = 32;
/// Narrowest button, before scaling.
const BUTTON_W: i32 = 80;

/// One button: the code `show` returns for it and its label.
#[derive(Debug, Clone, Copy)]
pub struct Choice {
    pub code: i32,
    pub label: &'static str,
}

/// Show `text` under `title` with `choices` laid out left to right (put the primary one last). Returns the code of
/// the button pressed; `default` is pressed by Enter and `cancel` by Esc or closing the window.
pub fn show(parent: &impl WxWidget, title: &str, text: &str, icon: Option<Icon>, choices: &[Choice], default: i32, cancel: i32) -> i32 {
    let t = theme::current();
    let d = Dialog::new(parent, title);
    let lines = lines(&d, text, t.px(WRAP));
    let buttons: Vec<Button> = choices
        .iter()
        .map(|c| {
            let b = Button::new(&d, c.label);
            let code = c.code;
            b.on_click(move |_| d.end_modal(code));
            if code == default {
                b.set_default();
                b.set_focus();
            }
            b
        })
        .collect();
    place(&d, &lines, icon.map(|i| glyph(&d, i)).as_ref(), &buttons);
    d.set_escape_id(cancel);
    d.on_char_hook(move |e| match &e {
        WindowEventData::Keyboard(k) if k.get_key_code() == Some(KEY_ESCAPE) => d.end_modal(cancel),
        _ => e.skip(true),
    });
    d.on_close(move |_| d.end_modal(cancel));
    d.centre();
    let code = d.show_modal();
    d.destroy();
    code
}

/// The text wrapped at `width`, one label per line: under the dark theme a native static control paints only its
/// first line. `&` is shown as itself, not as a mnemonic.
fn lines(d: &Dialog, text: &str, width: i32) -> Vec<Label> {
    let probe = Label::new(d, &text.replace('&', "&&"));
    probe.wrap(width);
    let wrapped = probe.get_label();
    probe.destroy();
    wrapped.split('\n').map(|l| Label::new(d, if l.is_empty() { " " } else { l })).collect()
}

/// Lay the box out from the controls' own sizes: icon and text in a row, buttons right-aligned under them. Placed
/// explicitly like the kit's other dialogs (with a sizer the text did not paint in the dark theme).
fn place(d: &Dialog, lines: &[Label], glyph: Option<&Panel>, buttons: &[Button]) {
    let t = theme::current();
    let (pad, gap, edge) = (t.px(t.spacing.xxl), t.px(t.spacing.xl), t.px(GLYPH));
    let line_sizes: Vec<Size> = lines.iter().map(WxWidget::get_best_size).collect();
    let text = Size::new(line_sizes.iter().map(|s| s.width).max().unwrap_or(0), line_sizes.iter().map(|s| s.height).sum());
    let icon_w = glyph.map_or(0, |_| edge + gap);
    let row_h = text.height.max(glyph.map_or(0, |_| edge));
    if let Some(g) = glyph {
        g.set_size_with_pos(pad, pad + (row_h - edge) / 2, edge, edge);
    }
    let mut line_y = pad + (row_h - text.height) / 2;
    for (l, s) in lines.iter().zip(&line_sizes) {
        l.set_size_with_pos(pad + icon_w, line_y, text.width, s.height);
        line_y += s.height;
    }
    let sizes: Vec<Size> = buttons.iter().map(WxWidget::get_best_size).map(|s| Size::new(s.width.max(t.px(BUTTON_W)), s.height)).collect();
    let row_w = sizes.iter().map(|s| s.width).sum::<i32>() + t.px(t.spacing.md) * (crate::num::to_i32(sizes.len()) - 1).max(0);
    let button_h = sizes.iter().map(|s| s.height).max().unwrap_or(0);
    let width = (pad + icon_w + text.width + pad).max(row_w + 2 * gap);
    let y = pad + row_h + pad;
    let mut x = width - gap - row_w;
    for (b, s) in buttons.iter().zip(&sizes) {
        b.set_size_with_pos(x, y, s.width, s.height);
        x += s.width + t.px(t.spacing.md);
    }
    d.set_client_size(Size::new(width, y + button_h + gap));
}

const KEY_ESCAPE: i32 = 27;

/// The round icon: a filled circle in the icon's colour with its sign on it.
fn glyph(d: &Dialog, icon: Icon) -> Panel {
    let p = Panel::builder(d).build();
    p.set_background_style(BackgroundStyle::Paint);
    let edge = theme::current().px(GLYPH);
    p.set_min_size(Size::new(edge, edge));
    p.on_paint(move |_| {
        let t = theme::current();
        let dc = AutoBufferedPaintDC::new(&p);
        let area = p.get_client_size();
        crate::paint::fill(&dc, Rect::new(0, 0, area.width, area.height), t.surface());
        let (fill, sign) = look(&t, icon);
        dc.set_pen(colour(fill), 1, PenStyle::Solid);
        dc.set_brush(colour(fill), BrushStyle::Solid);
        dc.draw_circle(area.width / 2, area.height / 2, area.width.min(area.height) / 2 - 1);
        if let Some(f) = t.title_font() {
            dc.set_font(&f);
        }
        text_at(&dc, sign, Rect::new(0, 0, area.width, area.height), on(fill, &t), true);
    });
    p
}

/// The theme's text colour that reads best on `fill` (light text on blue and red, dark text on amber).
fn on(fill: Rgb, t: &theme::Theme) -> Rgb {
    let luma = |(r, g, b): Rgb| 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b);
    let (a, b) = (t.colors.accent_text, t.colors.ink);
    if luma(fill).abs_diff(luma(a)) >= luma(fill).abs_diff(luma(b)) { a } else { b }
}

fn look(t: &theme::Theme, icon: Icon) -> (Rgb, &'static str) {
    match icon {
        Icon::Info => (t.colors.info, "i"),
        Icon::Question => (t.colors.accent, "?"),
        Icon::Warning => (t.colors.warning, "!"),
        Icon::Error => (t.colors.danger, "\u{00D7}"),
    }
}
