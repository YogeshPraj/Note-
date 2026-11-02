//! Note++ with a wxWidgets shell.
// A release build is a GUI program: no console window. Debug builds keep the console for tracing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chrome;
mod command;
mod dialogs;
mod encdialogs;
mod extra;
mod find;
mod findpanel;
mod handlers;
mod in_files;
mod incr;
mod mdbar;
mod menu_spec;
mod menus;
mod palette;
mod paste_special;
// Raw operating-system calls.
#[allow(unsafe_code)]
mod platform;
pub use platform::scintilla as sci;
mod panels;
mod popups;
mod preview;
mod pwdialogs;
mod services;
mod settings_ui;
mod settings_window;
mod side;
mod state;
mod streams;
mod ui;
mod updates;
mod whiteboard;
pub use ui_kit as widgets;

use wxdragon::prelude::*;

fn main() {
    // Our resources carry the visual-styles manifest; the check is for programs that forgot it.
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);
    sci::register_lexilla();
    let _ = widgets::form::PROBLEM_SINK.set(|line| app::trace(0, line));
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--new-instance") {
        notepp_core::session::set_standalone();
        args.retain(|a| a != "--new-instance");
    }
    let files: Vec<std::path::PathBuf> = args.into_iter().map(std::path::PathBuf::from).collect();
    let _ = wxdragon::main(move |_| app::start(&files));
}
