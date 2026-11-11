//! Holding a scroll arrow keeps scrolling: a first pause, then a step at a steady pace until the button is let go.

use super::*;

/// The wait before the arrow repeats, then the time between steps (milliseconds).
pub(super) const AFTER: i32 = 400;
const EVERY: i32 = 90;

/// The timer that steps the scroll while an arrow is held. Starting it (with `AFTER`) and stopping it is up to the mouse handlers.
pub(super) fn timer(panel: Panel, st: &Rc<RefCell<State>>) -> Rc<Timer<Panel>> {
    let timer = Rc::new(Timer::new(&panel));
    let (st, t) = (st.clone(), timer.clone());
    timer.on_tick(move |_| {
        let go = {
            let mut s = st.borrow_mut();
            let can = |b: Btn, s: &State| (b == Btn::Left && s.layout.left.is_some()) || (b == Btn::Right && s.layout.right.is_some());
            match s.btn_down {
                Some(b) if can(b, &s) => {
                    s.scroll.step(b);
                    true
                }
                _ => false,
            }
        };
        if go {
            t.set_interval(EVERY);
        } else {
            t.stop();
        }
        panel.refresh(false, None);
    });
    timer
}
