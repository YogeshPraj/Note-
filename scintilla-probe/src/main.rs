// Note++ native — Scintilla + Lexilla, structured for three platforms.
//
//   src/core/      portable: editor, tabs, sessions, encodings
//   src/platform/  one shell per OS; only win32 exists so far

#![windows_subsystem = "windows"]

mod core;
mod platform;

/// Log native crashes (access violations etc.) that a panic hook cannot see.
unsafe extern "system" fn crash_logger(info: *mut windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> i32 {
    let rec = unsafe { &*(*info).ExceptionRecord };
    // Only hard faults; first-chance C++ exceptions used internally are ignored.
    if matches!(rec.ExceptionCode.0 as u32, 0xC0000005 | 0xC0000094 | 0xC00000FD | 0xC0000409) {
        let dir = core::session::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let log = format!(
            "native exception {:#x} at {:p}\n{}\n",
            rec.ExceptionCode.0 as u32,
            rec.ExceptionAddress,
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::write(dir.join("crash.log"), log);
    }
    0 // EXCEPTION_CONTINUE_SEARCH
}

fn main() -> windows::core::Result<()> {
    platform::win32::harden_dll_search();
    unsafe {
        windows::Win32::System::Diagnostics::Debug::AddVectoredExceptionHandler(1, Some(crash_logger));
    }
    // A GUI app has no console, so record panics where the user (or we) can find them.
    std::panic::set_hook(Box::new(|info| {
        let dir = core::session::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        // The panic message can quote document text (e.g. a bad slice index), so keep only
        // where it happened and how long the message was.
        let at = info.location().map_or_else(|| "unknown location".to_string(), |l| format!("{}:{}", l.file(), l.line()));
        let len = info.payload().downcast_ref::<&str>().map(|s| s.len()).or_else(|| info.payload().downcast_ref::<String>().map(String::len)).unwrap_or(0);
        let log = format!("panic at {at} (message of {len} bytes withheld)\n{}\n", std::backtrace::Backtrace::force_capture());
        let _ = std::fs::write(dir.join("crash.log"), log);
    }));
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--new-instance") {
        core::session::set_standalone();
        args.retain(|a| a != "--new-instance");
    }
    let files = args.into_iter().map(std::path::PathBuf::from).collect();
    let (handles, menus) = platform::win32::create_window("Note++", 1200, 800)?;
    platform::win32::run(handles, menus, files);
    Ok(())
}
