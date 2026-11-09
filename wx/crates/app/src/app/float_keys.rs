//! Keys in a floating window. It has no menu bar, so the shortcuts are looked up for it (`core::key_routing`); everything
//! else goes on to the editor, which also keeps keys such as Del and Tab its own.

use super::*;

pub(super) fn wire(a: &Shared, frame: Frame) {
    let a = a.clone();
    frame.on_char_hook(move |e| super::keys::on_key(&a, &e, notepp_core::key_routing::Place::Floating));
}
