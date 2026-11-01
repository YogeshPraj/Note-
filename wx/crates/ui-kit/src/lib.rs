//! Reusable building blocks for the shell's own controls.

// wxdragon's get_event_handler_ptr is an unsafe trait method; the wrappers can only forward it.
#[allow(unsafe_code)]
pub mod controls;
pub mod data_list;
pub mod dock;
pub mod form;
pub mod geom;
pub mod menu_bar;
// Raw operating-system calls: the one place in the kit that needs unsafe code.
#[allow(unsafe_code)]
pub mod native;
pub mod paint;
pub mod pointer;
pub mod quick_pick;
pub mod status_bar;
pub mod tab_strip;
pub mod theme;
pub mod tool_bar;
pub mod web_pane;

