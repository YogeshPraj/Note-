//! System-wide capture keys while Note++ is running (visible, minimized or in the tray): the shortcuts of Capture Screen
//! and Capture Region (Print Screen and Shift+Print Screen unless the user remapped them) are registered with Windows
//! and work whichever program has the keyboard.
//!
//! A thread owns the registrations and waits for the keys; the window's own timer picks the presses up, so the capture
//! runs on the interface thread. When the shortcuts change, the thread is told to register the new ones. If another
//! program already owns a key, that command simply stays without a system-wide key (the toolbar and menu still work).

use notepp_core::commands::{ID_CAPTURE_REGION, ID_CAPTURE_SCREEN};
use notepp_core::keymap::KeyMap;
use notepp_core::shortcuts::Chord;

use crate::app::Shared;

#[cfg(windows)]
mod win {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use notepp_core::shortcuts::Chord;

    /// Presses not yet handled: bit 0 for Capture Screen, bit 1 for Capture Region.
    pub static PRESSED: AtomicU32 = AtomicU32::new(0);
    /// The chords wanted now: Capture Screen's, then Capture Region's.
    static WANTED: Mutex<[Option<Chord>; 2]> = Mutex::new([None, None]);
    /// The listening thread's id; 0 until it can be sent messages.
    static THREAD: AtomicU32 = AtomicU32::new(0);
    static STARTED: Mutex<bool> = Mutex::new(false);

    const MOD_ALT: u32 = 0x1;
    const MOD_CONTROL: u32 = 0x2;
    const MOD_SHIFT: u32 = 0x4;
    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;
    /// Sent to the thread: register what is wanted now.
    const WM_APP: u32 = 0x8000;
    const PM_NOREMOVE: u32 = 0;

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        x: i32,
        y: i32,
        private: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn PeekMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
        fn PostThreadMessageW(thread: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    /// Register `WANTED`, replacing what was registered. Runs on the listening thread.
    #[allow(unsafe_code)]
    fn apply() {
        let wanted = WANTED.lock().map_or([None, None], |w| *w);
        for (i, chord) in wanted.iter().enumerate() {
            let id = i32::try_from(i).unwrap_or(0) + 1;
            // SAFETY: plain calls with values only; the registrations belong to this thread.
            unsafe {
                UnregisterHotKey(0, id);
                if let Some(c) = chord {
                    let mods = (if c.ctrl { MOD_CONTROL } else { 0 }) | (if c.alt { MOD_ALT } else { 0 }) | (if c.shift { MOD_SHIFT } else { 0 });
                    RegisterHotKey(0, id, mods | MOD_NOREPEAT, u32::from(c.vk));
                }
            }
        }
    }

    /// Change the chords wanted, starting the listening thread the first time.
    #[allow(unsafe_code)]
    pub fn set(chords: [Option<Chord>; 2]) {
        if let Ok(mut w) = WANTED.lock() {
            *w = chords;
        }
        let Ok(mut started) = STARTED.lock() else { return };
        if !std::mem::replace(&mut *started, true) {
            std::thread::spawn(|| {
                // SAFETY: plain calls; the message struct lives through them.
                unsafe {
                    let mut msg = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, x: 0, y: 0, private: 0 };
                    // Looking at the queue creates it, so messages can be posted to this thread.
                    PeekMessageW(&raw mut msg, 0, 0, 0, PM_NOREMOVE);
                    THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
                    apply();
                    while GetMessageW(&raw mut msg, 0, 0, 0) > 0 {
                        if msg.message == WM_HOTKEY && (1..=2).contains(&msg.wparam) {
                            PRESSED.fetch_or(1 << (msg.wparam - 1), Ordering::SeqCst);
                        } else if msg.message == WM_APP {
                            apply();
                        }
                    }
                }
            });
            return;
        }
        let thread = THREAD.load(Ordering::SeqCst);
        if thread != 0 {
            // SAFETY: a plain call with values only.
            unsafe {
                PostThreadMessageW(thread, WM_APP, 0, 0);
            }
        }
    }
}

/// Register the capture commands' shortcuts from `keymap` (call again whenever it changes).
pub(crate) fn sync(keymap: &KeyMap) {
    let chords: [Option<Chord>; 2] = [keymap.chord(ID_CAPTURE_SCREEN), keymap.chord(ID_CAPTURE_REGION)];
    #[cfg(windows)]
    win::set(chords);
    #[cfg(not(windows))]
    let _ = chords;
}

/// Watch for the keys.
pub(crate) fn start(app: &Shared, frame: wxdragon::prelude::Frame) {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        use wxdragon::prelude::*;
        if let Ok(a) = app.try_borrow() {
            sync(&a.keymap);
        }
        let ticker = Timer::new(&frame);
        let a = app.clone();
        ticker.on_tick(move |_| {
            let bits = win::PRESSED.swap(0, Ordering::SeqCst);
            if bits & 1 != 0 {
                crate::app::run_command(&a, ID_CAPTURE_SCREEN);
            }
            if bits & 2 != 0 {
                crate::app::run_command(&a, ID_CAPTURE_REGION);
            }
        });
        ticker.start(100, false);
        std::mem::forget(ticker);
    }
    #[cfg(not(windows))]
    let _ = (app, frame);
}
