// One module per platform. Only one is compiled; the core is shared.
#[cfg(target_os = "windows")]
pub mod chrome;
#[cfg(target_os = "windows")]
pub mod dwm;
#[cfg(target_os = "windows")]
pub mod findpanel;
#[cfg(target_os = "windows")]
pub mod palette;
#[cfg(target_os = "windows")]
pub mod preview;
#[cfg(target_os = "windows")]
pub mod whiteboard;
#[cfg(target_os = "windows")]
pub mod win32;
// pub mod gtk;    <- Linux shell goes here
// pub mod cocoa;  <- macOS shell goes here
