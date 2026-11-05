//! The minimap's events.

use super::*;

/// The minimap is a second view of the document: clicking or dragging in it scrolls the editor.
pub(super) fn wire_mini(app: &Shared, mini: StyledTextCtrl) {
    use notepp_core::sci::Send;
    let jump = |app: &Shared, mini: StyledTextCtrl, e: &wxdragon::event::WindowEventData| {
        let p = match e {
            wxdragon::event::WindowEventData::MouseButton(m) => m.get_position(),
            wxdragon::event::WindowEventData::MouseMotion(m) => m.get_position(),
            _ => None,
        };
        let Some(p) = p else { return };
        let sci = WxSci::of(&mini);
        let pos = sci.send(sci::SCI_POSITIONFROMPOINT, crate::platform::wx::to_usize(p.x), p.y as isize).max(0);
        let line = sci.send(sci::SCI_LINEFROMPOSITION, crate::platform::wx::to_usize(pos), 0).max(0);
        if let Ok(mut a) = app.try_borrow_mut() {
            a.ws.scroll_to_line(crate::platform::wx::to_usize(line));
            a.refresh();
        }
    };
    let down = Rc::new(std::cell::Cell::new(false));
    let (a, d) = (app.clone(), down.clone());
    mini.on_mouse_left_down(move |e| {
        d.set(true);
        jump(&a, mini, &e);
    });
    let d = down.clone();
    mini.on_mouse_left_up(move |_| d.set(false));
    let a = app.clone();
    mini.on_mouse_motion(move |e| {
        if down.get() {
            jump(&a, mini, &e);
        }
    });
    let a = app.clone();
    mini.on_mouse_right_up(move |e| {
        if let wxdragon::event::WindowEventData::MouseButton(m) = &e
            && let Some(p) = m.get_position()
        {
            popups::minimap_menu(&a, mini.client_to_screen(p));
        }
    });
    let a = app.clone();
    mini.on_set_focus(move |_| {
        if let Ok(app) = a.try_borrow() {
            app.stc().set_focus();
        }
    });
}
