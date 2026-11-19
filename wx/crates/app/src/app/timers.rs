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
                    app.mcp_retry();
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
    // The close button sends the window to the tray (the session is saved first: if the program is then ended from
    // outside, nothing is lost). Ending it for real saves the session too; unsaved text comes back next time.
    let a = app.clone();
    frame.on_close(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            let kept = app.save_session();
            let lost = app.ws.unrecoverable(kept);
            let request = if let WindowEventData::General(ev) = &e { Some(*ev) } else { None };
            if let Some(ev) = request
                && crate::tray::wanted(ev.can_veto())
            {
                drop(app);
                if crate::tray::hide(&a, &frame) {
                    ev.veto();
                    return;
                }
            }
            // Ending for real: say so when unsaved text would not come back (the session file cannot hold it).
            if !lost.is_empty() {
                let veto = if let WindowEventData::General(ev) = &e { ev.can_veto().then_some(*ev) } else { None };
                if let Some(ev) = veto {
                    let names = lost.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
                    let msg = format!("These have unsaved changes that cannot be kept for next time: {names}.\n\nExit and lose them?");
                    if !crate::widgets::ask::yes_no(&frame, "Note++", &msg, false) {
                        crate::tray::cancel_quit();
                        ev.veto();
                        return;
                    }
                }
            }
            crate::tray::remove_icon();
            // The session is written and nothing runs after the main loop, so the program ends here. Taking the windows
            // and their web views down one by one crashed now and then (twice in about 30 test runs, cause not found).
            std::process::exit(0);
        }
        e.skip(true);
    });
}
