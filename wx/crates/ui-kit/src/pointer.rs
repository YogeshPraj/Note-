//! Hover and press tracking shared by the shell's button-like controls, with no toolkit in it.
//!
//! A control keeps one [`Pointer`] and asks it what to draw (`hot`, `down`) and whether a click completed
//! (`release`). Items are identified by index; the control does its own hit-testing.

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    hot: Option<usize>,
    down: Option<usize>,
}

impl Pointer {
    /// The item under the mouse.
    pub fn hot(&self) -> Option<usize> {
        self.hot
    }

    /// The item the mouse button is held on.
    pub fn down(&self) -> Option<usize> {
        self.down
    }

    /// The mouse moved over `hit`; true when the highlighted item changed (so a repaint is needed).
    pub fn hover(&mut self, hit: Option<usize>) -> bool {
        let changed = self.hot != hit;
        self.hot = hit;
        changed
    }

    /// The button went down over `hit`.
    pub fn press(&mut self, hit: Option<usize>) {
        self.down = hit;
        self.hot = hit;
    }

    /// The button went up over `hit`: the item that was clicked, when it is the one the press began on.
    pub fn release(&mut self, hit: Option<usize>) -> Option<usize> {
        let pressed = self.down.take();
        pressed.filter(|d| Some(*d) == hit)
    }

    /// The mouse left the control; true when something was highlighted or held.
    pub fn leave(&mut self) -> bool {
        let changed = self.hot.is_some() || self.down.is_some();
        self.hot = None;
        self.down = None;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_reports_only_real_changes() {
        let mut p = Pointer::default();
        assert!(p.hover(Some(2)));
        assert!(!p.hover(Some(2)));
        assert!(p.hover(None));
        assert_eq!(p.hot(), None);
    }

    #[test]
    fn a_click_completes_only_on_the_item_it_began_on() {
        let mut p = Pointer::default();
        p.press(Some(1));
        assert_eq!(p.down(), Some(1));
        assert_eq!(p.release(Some(1)), Some(1));
        assert_eq!(p.down(), None);
    }

    #[test]
    fn releasing_elsewhere_cancels_the_click() {
        let mut p = Pointer::default();
        p.press(Some(1));
        assert_eq!(p.release(Some(2)), None);
        p.press(Some(1));
        assert_eq!(p.release(None), None);
    }

    #[test]
    fn a_release_without_a_press_is_nothing() {
        assert_eq!(Pointer::default().release(Some(0)), None);
    }

    #[test]
    fn leaving_clears_everything_and_says_whether_it_mattered() {
        let mut p = Pointer::default();
        assert!(!p.leave());
        p.press(Some(3));
        assert!(p.leave());
        assert_eq!((p.hot(), p.down()), (None, None));
    }
}
