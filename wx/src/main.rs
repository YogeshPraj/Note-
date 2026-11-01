//! Note++ with a wxWidgets shell.

mod app;
mod dispatch;
mod dialogs;
mod encdialogs;
mod extra;
mod find;
mod findpanel;
mod incr;
mod mdbar;
mod menus;
mod chrome;
mod command;
mod handlers;
mod form;
mod palette;
mod platform;
mod panels;
mod pwdialogs;
mod popups;
mod preview;
mod streams;
mod whiteboard;
mod sci;
mod state;
mod settings_ui;
mod side;
mod tables;
mod ui;
mod updates;
mod widgets;

use wxdragon::prelude::*;

fn main() {
    // Our resources carry the visual-styles manifest; the check is for programs that forgot it.
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);
    sci::register_lexilla();
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--new-instance") {
        notepp_core::session::set_standalone();
        args.retain(|a| a != "--new-instance");
    }
    let files: Vec<std::path::PathBuf> = args.into_iter().map(std::path::PathBuf::from).collect();
    let _ = wxdragon::main(move |_| app::start(files.clone()));
}
