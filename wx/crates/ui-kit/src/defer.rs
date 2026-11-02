//! Running something just after the current event, for work that cannot happen inside the event that asks for it
//! (a window cannot be closed from its own handler; the focus a click hands to a control is set after the handler ran).

use std::cell::RefCell;
use std::collections::VecDeque;

use wxdragon::prelude::*;

thread_local! {
    static QUEUE: RefCell<VecDeque<Box<dyn FnOnce()>>> = const { RefCell::new(VecDeque::new()) };
    static TIMER: RefCell<Option<Timer<Frame>>> = const { RefCell::new(None) };
}

/// Run `f` on the UI thread shortly after the current event has been handled.
pub fn later(owner: &Frame, f: impl FnOnce() + 'static) {
    QUEUE.with(|q| q.borrow_mut().push_back(Box::new(f)));
    TIMER.with(|t| {
        let mut t = t.borrow_mut();
        let timer = t.get_or_insert_with(|| {
            let timer = Timer::new(owner);
            timer.on_tick(|_| run_queued());
            timer
        });
        timer.start(10, true);
    });
}

fn run_queued() {
    while let Some(f) = QUEUE.with(|q| q.borrow_mut().pop_front()) {
        f();
    }
}
