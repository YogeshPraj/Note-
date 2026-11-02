//! The kit's components: one per file. Each is a small, themed, self-contained control with no knowledge of the
//! editor; the shell composes them. See `examples/gallery.rs` for all of them on screen.

mod themed;

// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod button;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod check_box;
pub mod data_list;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod dialog;
pub mod dock;
pub mod form;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod label;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod list_box;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod list_view;
pub mod menu_bar;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod panel;
pub mod popup_menu;
pub mod quick_pick;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod radio_button;
pub mod settings_window;
pub mod status_bar;
pub mod tab_strip;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod text_box;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod toggle_button;
pub mod tool_bar;
// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod tree_view;
pub mod web_pane;
