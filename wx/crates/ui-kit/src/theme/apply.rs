//! Applying the theme to widgets: roles, adopting a widget, painting, borders and native styling.

use super::*;

/// Keep `w` coloured for the theme (and styled natively) from now on; stops when the widget is destroyed.
pub fn adopt<W: WxWidget + Copy + 'static>(w: W, role: Role) {
    subscribe_while(move |_| {
        if !w.is_valid() {
            return false;
        }
        // Native theme first: it can reset colours, so ours go on last.
        style_native(&w);
        paint(&w, role);
        true
    });
}

/// What a widget is for, which decides its background.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    /// Panels, labels, buttons and check boxes.
    Surface,
    /// Text boxes and lists.
    Input,
    /// Panels that look like the editor itself: the workspace tree and the side lists.
    Editor,
}

pub fn background(role: Role) -> Colour {
    colour(current().background(role))
}

pub fn foreground(role: Role) -> Colour {
    colour(current().foreground(role))
}

/// Colour one widget for the current theme.
pub fn paint(w: &dyn WxWidget, role: Role) {
    w.set_background_color(background(role));
    w.set_foreground_color(foreground(role));
    w.refresh(true, None);
}

/// Make a container fill its own background with the surface colour on every repaint, instead of relying on the
/// toolkit's erase (which follows the system's dark setting rather than ours).
pub fn own_background(panel: Panel) {
    panel.set_background_style(BackgroundStyle::Paint);
    panel.on_paint(move |_| {
        let dc = AutoBufferedPaintDC::new(&panel);
        let size = panel.get_client_size();
        let c = background(Role::Surface);
        dc.set_pen(c, 1, PenStyle::Solid);
        dc.set_brush(c, BrushStyle::Solid);
        dc.draw_rectangle(0, 0, size.width, size.height);
    });
    // Repaint when the theme changes.
    subscribe(move |_| panel.refresh(true, None));
}

/// Paint a splitter's sash in the theme's colour: the toolkit draws it in a system colour, which shows as a white bar in
/// the dark theme. The panes cover everything else, so only the sash is left to paint. The toolkit also draws the sash
/// straight onto the window after every resize, so a resize of the splitter or of `pane` (which moves with the sash)
/// asks for a paint just after it.
pub fn own_sash(split: SplitterWindow, pane: Panel) {
    split.set_background_style(BackgroundStyle::Paint);
    let timer = Rc::new(Timer::new(&split));
    timer.on_tick(move |_| split.refresh(true, None));
    let later = timer.clone();
    split.on_size(move |e| {
        later.start(30, true);
        e.skip(true);
    });
    pane.on_size(move |e| {
        timer.start(30, true);
        e.skip(true);
    });
    split.on_paint(move |e| {
        e.skip(false);
        let dc = AutoBufferedPaintDC::new(&split);
        let size = split.get_client_size();
        let c = colour(current().colors.border);
        dc.set_pen(c, 1, PenStyle::Solid);
        dc.set_brush(c, BrushStyle::Solid);
        dc.draw_rectangle(0, 0, size.width, size.height);
    });
    subscribe(move |_| split.refresh(true, None));
}

/// Drop the toolkit's themed border: it is drawn in the system colours, which show as a white frame in the dark theme.
pub fn drop_border(w: &impl WxWidget) {
    const BORDERS: i64 = 0x1F00_0000; // wxBORDER_SIMPLE | SUNKEN | RAISED | STATIC | THEME
    const BORDER_NONE: i64 = 0x0020_0000;
    w.set_style_raw((w.get_style_raw() & !BORDERS) | BORDER_NONE);
    crate::native::current().remove_edge(w.get_handle() as isize);
}

/// Dark title bar and native control theme for a window and everything inside it.
pub fn style_native(w: &dyn WxWidget) {
    crate::native::current().style_window(w.get_handle() as isize, is_dark());
}
