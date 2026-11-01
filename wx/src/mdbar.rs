//! The Markdown formatting toolbar: a row of buttons shown above the editor while a Markdown file is active.
//! Each button runs the command with its id.

use notepp_core::commands as c;
use wxdragon::prelude::*;

use crate::app::Shared;

pub const HEIGHT: i32 = 38;

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Plain,
    Bold,
    Italic,
    Strike,
}

/// What the bar shows, left to right: command, label, tooltip, width, font style. `None` is a gap between groups.
type Slot = Option<(usize, &'static str, &'static str, i32, Style)>;

fn layout() -> Vec<Slot> {
    let b = |id, label, tip, width, style| Some((id, label, tip, width, style));
    vec![
        b(c::ID_MD_BOLD, "B", "Bold", 30, Style::Bold),
        b(c::ID_MD_ITALIC, "I", "Italic", 30, Style::Italic),
        b(c::ID_MD_STRIKE, "S", "Strikethrough", 30, Style::Strike),
        b(c::ID_MD_CODE, "</>", "Inline code", 38, Style::Plain),
        None,
        b(c::ID_MD_H1, "H1", "Heading 1", 34, Style::Plain),
        b(c::ID_MD_H2, "H2", "Heading 2", 34, Style::Plain),
        b(c::ID_MD_H3, "H3", "Heading 3", 34, Style::Plain),
        None,
        b(c::ID_MD_QUOTE, "\u{275D}", "Quote", 30, Style::Plain),
        b(c::ID_MD_BULLET, "\u{2022}\u{2261}", "Bulleted list", 40, Style::Plain),
        b(c::ID_MD_NUMBERED, "1.", "Numbered list", 32, Style::Plain),
        b(c::ID_MD_TASK, "[x]", "Task list", 32, Style::Plain),
        None,
        b(c::ID_MD_LINK, "Link", "Insert link", 46, Style::Plain),
        b(c::ID_MD_IMAGE, "Image", "Insert image", 54, Style::Plain),
        b(c::ID_MD_TABLE, "Table", "Insert table", 50, Style::Plain),
        b(c::ID_MD_CODEBLOCK, "{ }", "Code block", 38, Style::Plain),
        b(c::ID_MD_RULE, "\u{2015}", "Horizontal rule", 30, Style::Plain),
    ]
}

/// The (initially hidden) empty bar inside `frame`; its buttons are added by [`wire`].
pub fn create(frame: &Frame) -> Panel {
    let bar = Panel::builder(frame).build();
    bar.set_min_size(Size::new(-1, HEIGHT));
    bar.show(false);
    bar
}

pub fn wire(app: &Shared) {
    let bar = app.borrow().mdbar;
    let (mut x, y, gap) = (8, 5, 8);
    for slot in layout() {
        match slot {
            None => x += gap,
            Some((id, label, tip, w, style)) => {
                let b = Button::builder(&bar).with_label(label).with_pos(Point::new(x, y)).with_size(Size::new(w, 28)).build();
                let size = 9;
                let font = match style {
                    Style::Plain => None,
                    Style::Bold => Font::builder().with_point_size(size).with_weight(FontWeight::Bold).build(),
                    Style::Italic => Font::builder().with_point_size(size).with_style(FontStyle::Italic).build(),
                    Style::Strike => Font::builder().with_point_size(size).with_strikethrough(true).build(),
                };
                if let Some(f) = font {
                    b.set_font(&f);
                }
                b.set_tooltip(tip);
                let a = app.clone();
                b.on_click(move |_| crate::app::run_command(&a, id));
                x += w + 2;
            }
        }
    }
}
