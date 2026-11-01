//! The macro behind the themed controls. Internal: each control has its own file.

/// Define a `Copy` handle `$name` around the stock widget `$inner`. It dereferences to the stock widget, so every
/// existing call and event hook keeps working, and it is adopted by the theme on creation: coloured and natively
/// styled now, and again whenever the theme changes, until the widget is destroyed.
macro_rules! themed {
    ($(#[$doc:meta])* $name:ident, $inner:ty, $role:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy)]
        pub struct $name($inner);

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        impl $name {
            /// Take over a stock widget: it follows the theme from now on.
            pub fn adopt(inner: $inner) -> Self {
                Self::adopt_as(inner, $role)
            }

            /// Like `adopt`, with a different role than the control's usual one.
            pub fn adopt_as(inner: $inner, role: $crate::theme::Role) -> Self {
                $crate::theme::adopt(inner, role);
                Self(inner)
            }

            /// The stock widget underneath.
            pub fn inner(&self) -> $inner {
                self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &$inner {
                &self.0
            }
        }

        impl ::wxdragon::prelude::WxWidget for $name {
            fn handle_ptr(&self) -> *mut ::wxdragon_sys::wxd_Window_t {
                ::wxdragon::prelude::WxWidget::handle_ptr(&self.0)
            }
            fn is_valid(&self) -> bool {
                ::wxdragon::prelude::WxWidget::is_valid(&self.0)
            }
        }

        impl ::wxdragon::prelude::WxEvtHandler for $name {
            unsafe fn get_event_handler_ptr(&self) -> *mut ::wxdragon_sys::wxd_EvtHandler_t {
                unsafe { ::wxdragon::prelude::WxEvtHandler::get_event_handler_ptr(&self.0) }
            }
        }
    };
}

pub(crate) use themed;
