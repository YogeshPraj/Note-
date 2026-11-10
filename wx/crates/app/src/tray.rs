//! The system tray: the window's close button sends Note++ there instead of ending it, and a double-click on its icon
//! (or a second launch) brings the window back. File > Exit, the tray menu's Exit and an installing update end it.

use std::cell::{Cell, RefCell};

use notepp_core::icon;
use wxdragon::prelude::*;

use crate::app::Shared;

const OPEN: i32 = 1;
const EXIT: i32 = 2;
/// The side of the picture the tray icon is made from (the system scales it to its own size).
const ICON_SIDE: u32 = 32;

thread_local! {
    static ICON: RefCell<Option<TaskBarIcon>> = const { RefCell::new(None) };
    static QUITTING: Cell<bool> = const { Cell::new(false) };
}

/// End the program: the close request that follows is not sent to the tray.
pub fn quit(frame: &Frame) {
    QUITTING.with(|q| q.set(true));
    frame.close(false);
}

/// Whether the window may go to the tray now: not when the program is being ended on purpose, and not when the system
/// forces the window closed (logging off).
pub fn wanted(can_veto: bool) -> bool {
    can_veto && !QUITTING.with(Cell::get)
}

/// Take the icon out of the tray (it would stay there, a dead icon, after the program ends).
pub fn remove_icon() {
    ICON.with(|i| {
        if let Some(icon) = i.borrow().as_ref() {
            icon.remove_icon();
        }
    });
}

fn picture() -> Option<Bitmap> {
    let (w, h, rgba) = icon::ico_picture(include_bytes!("../../../../assets/app.ico"), ICON_SIDE)?;
    Bitmap::from_rgba(&rgba, w, h)
}

/// Hide the window and show Note++'s icon in the tray. False (and the window stays) when there is no icon to show.
pub fn hide(app: &Shared, frame: &Frame) -> bool {
    let Some(bitmap) = picture() else { return false };
    let shown = ICON.with(|i| {
        let mut slot = i.borrow_mut();
        let icon = slot.get_or_insert_with(|| make_icon(app));
        icon.set_icon(&bitmap, "Note++")
    });
    if shown {
        frame.show(false);
    }
    shown
}

/// Bring the window back from the tray.
pub fn restore(frame: &Frame) {
    frame.show(true);
    if frame.is_iconized() {
        frame.iconize(false);
    }
    frame.raise();
    remove_icon();
}

fn make_icon(app: &Shared) -> TaskBarIcon {
    let icon = TaskBarIcon::builder().build();
    let frame = app.borrow().frame;
    icon.on_left_double_click(move |_| restore(&frame));
    icon.on_right_up(move |_| {
        let mut menu = Menu::builder().append_item(OPEN, "Open Note++", "").append_separator().append_item(EXIT, "Exit", "").build();
        ICON.with(|i| {
            if let Some(icon) = i.borrow().as_ref() {
                icon.popup_menu(&mut menu);
            }
        });
    });
    icon.on_menu(move |e| match e.get_id() {
        OPEN => restore(&frame),
        EXIT => quit(&frame),
        _ => {}
    });
    icon
}
