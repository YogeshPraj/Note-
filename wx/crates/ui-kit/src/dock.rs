//! A docked strip of the main window: a panel that paints its own themed background, takes a height, and asks the
//! window to re-flow when that changes. Height 0 hides it.
//!
//! The Find panel and the Markdown bar both sit in one.

use wxdragon::prelude::*;

use super::theme;

impl std::fmt::Debug for Dock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dock").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub struct Dock {
    host: Panel,
    frame: Frame,
}

impl Dock {
    /// An empty, hidden dock inside `frame`; add the controls to [`Dock::panel`].
    pub fn new(frame: &Frame) -> Dock {
        let host = Panel::builder(frame).build();
        theme::own_background(host);
        host.show(false);
        Dock { host, frame: *frame }
    }

    pub fn panel(&self) -> Panel {
        self.host
    }

    pub fn is_shown(&self) -> bool {
        self.host.is_shown()
    }

    /// Give the dock `height` pixels (0 hides it) and let the window re-flow.
    pub fn set_height(&self, height: i32) {
        self.host.show(height > 0);
        self.host.set_min_size(Size::new(-1, height));
        self.frame.layout();
    }
}
