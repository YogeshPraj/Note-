//! Auto-update: look for a newer release, download it in the background, and swap it in on request.

use std::path::PathBuf;
use std::sync::Mutex;

use notepp_core::commands as c;
use notepp_core::update::{self, Release};
use notepp_core::workspace::Ui as _;
use wxdragon::prelude::*;

use crate::app::App;
use crate::command::CommandHandler;

/// What the update threads hand to the window.
#[derive(Default)]
struct Shared {
    check: Option<(bool, Result<Option<Release>, notepp_core::error::Error>)>,
    pct: Option<u32>,
    done: Option<Result<(String, PathBuf), notepp_core::error::Error>>,
}

static UPD: Mutex<Shared> = Mutex::new(Shared { check: None, pct: None, done: None });

/// Half-second timer ticks until the first background check after start-up (10 s: the window is up and the editor quiet),
/// and between the checks after it (6 hours).
const FIRST_CHECK_TICKS: u32 = 20;
const RECHECK_TICKS: u32 = 6 * 60 * 60 * 2;

thread_local! {
    /// Ticks left until the next background check; 0 = none is planned.
    static CHECK_IN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// The user asked for this check or download, so a failure is worth a dialog; a background one fails quietly.
    static USER_ASKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Only an installed release updates itself in place: it replaces the folder its `notepp.exe` runs from. A development
/// build, a test instance (own profile) and the other systems never do.
fn self_updating() -> bool {
    cfg!(windows)
        && !cfg!(debug_assertions)
        && !notepp_core::session::standalone()
        && std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.eq_ignore_ascii_case("notepp.exe"))).unwrap_or(false)
}

/// Called while starting, before any window exists: an update downloaded earlier is installed now. The new version
/// replaces this one's files and starts with the same files to open; this process ends at once.
pub(crate) fn apply_staged_at_start(files: &[PathBuf]) {
    if !self_updating() || !notepp_core::settings::Settings::load().auto_update {
        return;
    }
    let Some((_, dir)) = update::claim_for_start(update::VERSION) else { return };
    if update::launch_apply(&dir, files).is_ok() {
        std::process::exit(0);
    }
}

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Available(Release),
    Downloading(Release, u32),
    Ready(String, PathBuf),
}

impl App {
    /// An update downloaded by an earlier run is offered again straight away.
    pub fn update_startup(&mut self) {
        if let Some(v) = update::take_applied(update::VERSION) {
            self.flash = Some(format!("Note++ was updated to v{v}"));
        }
        if let Some((v, d)) = update::staged(update::VERSION) {
            self.upd = State::Ready(v, d);
            self.update_pill();
        }
        if self_updating() {
            CHECK_IN.with(|c| c.set(FIRST_CHECK_TICKS));
        }
    }

    /// Timer: look for an update in the background now and then (after start-up and every few hours) when automatic
    /// updates are on. A found update downloads at once (see `update_poll`) and installs the next time the program starts.
    fn update_auto_check(&mut self) {
        let left = CHECK_IN.with(std::cell::Cell::get);
        if left == 0 {
            return;
        }
        CHECK_IN.with(|c| c.set(left - 1));
        if left > 1 {
            return;
        }
        CHECK_IN.with(|c| c.set(RECHECK_TICKS));
        if self.ws.settings.auto_update && matches!(self.upd, State::Idle | State::Available(_)) {
            self.update_check(false);
        }
    }

    /// Show the state of the update in the toolbar pill.
    fn update_pill(&self) {
        let pill = match &self.upd {
            State::Available(r) => Some((format!("New update available (v{})", r.version), crate::widgets::theme::current().colors.info, c::ID_UPDATE_PILL)),
            State::Downloading(_, p) => Some((format!("Downloading {p}%"), crate::widgets::theme::current().colors.warning, c::ID_UPDATE_PILL)),
            State::Ready(v, _) => {
                let text = if self.ws.settings.auto_update {
                    format!("v{v} installs on next start (click to install now)")
                } else {
                    format!("Click to update to v{v}")
                };
                Some((text, crate::widgets::theme::current().colors.success, c::ID_UPDATE_PILL))
            }
            State::Idle | State::Checking => None,
        };
        self.chrome.set_pill(pill);
    }

    /// Ask GitHub for a newer version in the background. `manual`: the user asked, so say the result.
    pub fn update_check(&mut self, manual: bool) {
        // The in-place update swaps a Windows folder; elsewhere a new version is downloaded by hand.
        if !cfg!(windows) {
            if manual {
                crate::platform::current().open_default("https://github.com/YogeshPraj/Note-/releases/latest");
            }
            return;
        }
        if !matches!(self.upd, State::Idle | State::Available(_)) {
            if manual {
                self.flash = Some(
                    match self.upd {
                        State::Ready(..) => "An update is ready: click the green notice to install it",
                        _ => "An update check or download is already running",
                    }
                    .into(),
                );
            }
            return;
        }
        USER_ASKED.with(|u| u.set(manual));
        if manual {
            self.flash = Some("Checking for updates...".into());
        }
        self.upd = State::Checking;
        std::thread::spawn(move || {
            let r = update::check(update::VERSION);
            UPD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).check = Some((manual, r));
        });
    }

    fn update_download(&mut self, rel: Release) {
        self.upd = State::Downloading(rel.clone(), 0);
        self.update_pill();
        std::thread::spawn(move || {
            let r = update::download(&rel, &mut |p| {
                UPD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pct = Some(p);
            });
            UPD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).done = Some(r.map(|dir| (rel.version.clone(), dir)));
        });
    }

    /// Timer: take what the background steps have finished.
    pub fn update_poll(&mut self) {
        self.update_auto_check();
        let (check, pct, done) = {
            let mut u = UPD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            (u.check.take(), u.pct.take(), u.done.take())
        };
        if check.is_none() && pct.is_none() && done.is_none() {
            return;
        }
        if let Some(p) = pct
            && let State::Downloading(r, _) = &self.upd
        {
            self.upd = State::Downloading(r.clone(), p);
        }
        if let Some((manual, result)) = check {
            self.upd = State::Idle;
            match result {
                Ok(Some(rel)) => {
                    self.upd = State::Available(rel.clone());
                    // With automatic updates on (or when asked) the download starts at once.
                    if manual || self.ws.settings.auto_update {
                        self.update_download(rel);
                    }
                }
                Ok(None) if manual => {
                    let msg = format!("You have the latest version of Note++ (v{}).", update::VERSION);
                    MessageDialog::builder(&self.frame, &msg, "Note++")
                        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
                        .build()
                        .show_modal();
                }
                Err(e) if manual => self.ui.error(&format!("Cannot check for updates\n\n{e}")),
                Ok(None) | Err(_) => {}
            }
        }
        if let Some(result) = done {
            match result {
                Ok((ver, dir)) => {
                    self.flash = Some(if self.ws.settings.auto_update {
                        format!("Update v{ver} downloaded: it will be installed the next time you start Note++")
                    } else {
                        "Update downloaded - click the green notice to install".into()
                    });
                    self.upd = State::Ready(ver, dir);
                }
                Err(e) => {
                    self.upd = State::Idle;
                    if USER_ASKED.with(std::cell::Cell::get) {
                        self.ui.error(&format!("The update could not be downloaded\n\n{e}"));
                    } else {
                        self.flash = Some("An update could not be downloaded; Note++ will try again later".into());
                    }
                }
            }
        }
        self.update_pill();
        self.refresh();
    }

    /// Swap the new version in: a small script waits for this program to exit, copies the files and starts it again.
    fn update_install(&mut self, ver: &str, dir: &std::path::Path) {
        let msg = format!("Install Note++ v{ver} now?\n\nNote++ closes and starts again with the new version. Your tabs come back.");
        if !self.ui.ask_yes_no(&msg) {
            return;
        }
        match update::launch_apply(dir, &[]) {
            Ok(()) => {
                self.frame.close(false);
            }
            Err(e) => self.ui.error(&format!("Cannot install the update\n\n{e}")),
        }
    }
}

/// Update checks and the toolbar pill.
pub struct UpdateCommands;

impl CommandHandler<App> for UpdateCommands {
    fn name(&self) -> &'static str {
        "updates"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_UPDATE_CHECK => app.update_check(true),
            c::ID_UPDATE_AUTO => {
                let mut s = app.ws.settings.clone();
                s.auto_update = !s.auto_update;
                let on = s.auto_update;
                crate::settings_ui::apply_settings(app, s);
                app.flash =
                    Some(if on { "Updates download in the background and install the next time you start Note++" } else { "Automatic updates are off" }.into());
            }
            c::ID_UPDATE_PILL => match app.upd.clone() {
                State::Available(rel) => {
                    USER_ASKED.with(|u| u.set(true));
                    app.update_download(rel);
                }
                State::Ready(ver, dir) => app.update_install(&ver, &dir),
                State::Downloading(..) | State::Checking => app.flash = Some("The update is still downloading".into()),
                State::Idle => app.update_check(true),
            },
            _ => return false,
        }
        true
    }
}
