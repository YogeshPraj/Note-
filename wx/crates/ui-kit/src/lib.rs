//! Note++'s UI kit: themed controls and custom-painted bars for wxWidgets, with no editor or business logic.
//!
//! * [`theme`]: the single source of truth for colours, spacing, radii and fonts, and the hub components follow.
//! * [`components`]: one component per file (menu bar, tool bar, tab strip, status bar, dialogs, themed controls...).
//! * [`paint`], [`geom`], [`pointer`]: what the custom-painted components share.
//! * [`native`]: the operating-system calls a window needs (dark title bar, native menus).
//!
//! `examples/gallery.rs` shows every component in the light and the dark theme.

pub mod components;
pub mod defer;
pub mod geom;
pub mod num;
// Raw operating-system calls: the one place in the kit that needs unsafe code.
#[allow(unsafe_code)]
pub mod native;
pub mod paint;
pub mod pointer;
pub mod theme;

pub use components::{data_list, dock, form, menu_bar, popup_menu, quick_pick, settings_window, status_bar, tab_strip, tool_bar, web_pane};

/// The themed stock controls, under one name.
pub mod controls {
    pub use crate::components::button::Button;
    pub use crate::components::check_box::CheckBox;
    pub use crate::components::dialog::Dialog;
    pub use crate::components::label::Label;
    pub use crate::components::list_box::ListBox;
    pub use crate::components::list_view::ListView;
    pub use crate::components::panel::Panel;
    pub use crate::components::radio_button::RadioButton;
    pub use crate::components::text_box::TextBox;
    pub use crate::components::toggle_button::ToggleButton;
    pub use crate::components::tree_view::TreeView;
}
