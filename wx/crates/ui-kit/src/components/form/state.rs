//! A form's own state. Form callbacks are plain functions (`fn(&Form, usize)`), so whatever a dialog needs to keep
//! between them (its rows, what was picked) lives with the form, reached through the `&Form` they get, instead of in
//! globals.
//!
//! ```ignore
//! struct Picked(Option<usize>);
//! fn clicked(f: &Form, id: usize) { f.with_state(|p: &mut Picked| p.0 = Some(id)); }
//! let keep = form::run(&frame, spec, |f| f.set_state(Picked(None)));
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::Form;

/// The state of one form, shared by its clones.
#[derive(Clone, Default)]
pub(super) struct Slot(Rc<RefCell<Option<Box<dyn Any>>>>);

impl Form {
    /// Keep `state` with this form, replacing what it kept before.
    pub fn set_state<T: 'static>(&self, state: T) {
        *self.state.0.borrow_mut() = Some(Box::new(state));
    }

    /// Run `f` on the form's state when it is a `T`; None when the form keeps nothing of that type.
    pub fn with_state<T: 'static, R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut slot = self.state.0.borrow_mut();
        slot.as_mut().and_then(|b| b.downcast_mut::<T>()).map(f)
    }

    /// Take the form's state out (typically after the dialog closed), when it is a `T`.
    pub fn take_state<T: 'static>(&self) -> Option<T> {
        let taken = self.state.0.borrow_mut().take()?;
        match taken.downcast::<T>() {
            Ok(t) => Some(*t),
            Err(other) => {
                *self.state.0.borrow_mut() = Some(other);
                None
            }
        }
    }
}
