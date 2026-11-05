//! The periodic jobs of the window.

use super::*;

/// One timer for every periodic job, and saving the session when the window closes.
pub(super) fn wire_timers(app: &Shared, frame: Frame) {
    // One timer drives every periodic job; the schedule says how often each runs.
    #[derive(Clone, Copy, PartialEq)]
    enum Job {
        /// Followed logs, HTTP responses, the update check, files dropped by a second launch.
        Stream,
        /// Clipboard History records what is copied, whichever program copies it; a copied password is cleared.
        Clipboard,
        /// Keep the session file current (a crash or power cut then loses at most a few seconds).
        Autosave,
    }
    let schedule = Schedule::new(500).every(Job::Stream, 500).every(Job::Clipboard, 1000).every(Job::Autosave, 10_000);
    let ticker = Timer::new(&frame);
    let a = app.clone();
    let beat = std::cell::Cell::new(0u64);
    let base = crate::platform::wx::to_i32(schedule.base_ms());
    ticker.on_tick(move |_| {
        beat.set(beat.get() + 1);
        let Ok(mut app) = a.try_borrow_mut() else { return };
        for job in schedule.due(beat.get()) {
            match job {
                Job::Stream => {
                    app.update_poll();
                    app.check_inbox();
                    app.monitor_tick();
                    app.stream_tick();
                    app.in_files_poll();
                    app.file_index.poll();
                    crate::tree_reader::poll(&app.panels.tree);
                }
                Job::Clipboard => {
                    app.clip_tick();
                    app.poll_clipboard();
                }
                Job::Autosave => app.autosave_session(),
            }
        }
    });
    ticker.start(base, false);
    std::mem::forget(ticker);
    // Closing the window saves the session; unsaved text comes back next time.
    let a = app.clone();
    frame.on_close(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.save_session();
            app.close_float_windows();
        }
        e.skip(true);
    });
}
