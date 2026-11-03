//! Passwords (Bitwarden): the dialogs around `core::bitwarden`.
//!
//! Flow: install the official CLI -> sign in / unlock -> pick an item -> Insert (at the cursor,
//! clipboard untouched) or Copy (cleared after 30 seconds). Forms cannot be nested, so each screen
//! is its own form and `run` strings them together. Work that waits on `bw` or the network runs on
//! a thread while the form keeps repainting.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::rc::Rc;

use notepp_core::bitwarden::{self as bw, AuthState, GenOpts, Item, NewItem};
use wxdragon::prelude::*;
use zeroize::Zeroizing;

use crate::widgets::form::{self, Form, Kind, Spec, Values, ctl};

/// What the user chose.
pub enum Outcome {
    /// Put this at the cursor (the clipboard is not used).
    Insert(Zeroizing<String>),
    /// Put this on the clipboard (the caller clears it again after a while).
    Copy(Zeroizing<String>),
    Closed,
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Close,
    Insert,
    Copy,
    Add,
    Relock,
}

thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
    static ACT: Cell<Act> = const { Cell::new(Act::Close) };
    static SECRET: RefCell<Option<Zeroizing<String>>> = const { RefCell::new(None) };
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    static SHOWN: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

type Progress = Arc<Mutex<(u32, String)>>;

/// Run `job` on a thread, keeping the form alive (repainting, showing progress in `status`) until
/// it finishes. OK is off meanwhile; Cancel stays on and closes the dialog, which stops the job.
fn work<T: Send + 'static>(f: &Form, status: usize, first: &str, job: impl FnOnce(&Progress) -> T + Send + 'static) -> T {
    BUSY.with(|b| b.set(true));
    f.enable(1, false);
    f.set_text(status, first);
    bw::begin_job();
    let prog: Progress = Arc::new(Mutex::new((0, first.to_string())));
    let p2 = prog.clone();
    let handle = std::thread::spawn(move || job(&p2));
    let mut shown = (u32::MAX, String::new());
    let mut stopping = false;
    loop {
        // Let the dialog repaint and answer the mouse while the worker runs.
        #[allow(unsafe_code)]
        unsafe {
            wxdragon_sys::wxd_Note_SafeYield();
        };
        // Cancel (or Esc) closed the dialog: stop the job; the loop ends as soon as the worker gives up.
        if !stopping && !f.is_shown() {
            stopping = true;
            bw::cancel();
        }
        let now = prog.lock().map(|p| p.clone()).unwrap_or_default();
        if now != shown {
            f.set_text(status, &if now.0 > 0 && now.0 < 100 { format!("{} - {}%", now.1, now.0) } else { now.1.clone() });
            shown = now;
        }
        if handle.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    f.enable(1, true);
    BUSY.with(|b| b.set(false));
    handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn busy() -> bool {
    BUSY.with(std::cell::Cell::get)
}

fn show_error(host: &Frame, text: &str) {
    MessageDialog::builder(host, text, "Passwords").with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError).build().show_modal();
}

mod install;
use install::*;

// ---- sign in / unlock -----------------------------------------------------------------------

const U_INFO: usize = 70;
const U_EMAIL: usize = 72;
const U_PW: usize = 74;
const U_STATUS: usize = 75;
const U_TOKEN: usize = 77;

/// Sign in or unlock. True when the vault is unlocked afterwards.
fn unlock_screen(host: &Frame, auth: &bw::Auth) -> bool {
    let signed_out = auth.state == AuthState::Unauthenticated;
    let info = match (&auth.state, &auth.email) {
        (AuthState::Unauthenticated, _) => "You are not signed in to Bitwarden. Enter your email and master password.".to_string(),
        (_, Some(e)) => format!("Signed in as {e}. The vault is locked - enter your master password."),
        _ => "The vault is locked - enter your master password.".to_string(),
    };
    let email = auth.email.clone().unwrap_or_default();
    let apply = move |v: &Values, f: &Form| {
        let (token, pw, mail) = (v.text(U_TOKEN).trim().to_string(), Zeroizing::new(v.text(U_PW).to_string()), v.text(U_EMAIL).trim().to_string());
        if token.is_empty() && pw.is_empty() {
            f.set_text(U_STATUS, "Enter your master password.");
            return true;
        }
        let r = work(f, U_STATUS, if signed_out { "Signing in..." } else { "Unlocking..." }, move |_| {
            if !token.is_empty() {
                bw::use_session_token(&token)
            } else if signed_out {
                bw::login(&mail, &pw)
            } else {
                bw::unlock(&pw)
            }
        });
        match r {
            Ok(()) => false,
            Err(e) => {
                f.set_text(U_STATUS, &e.to_string());
                f.set_text(U_PW, "");
                f.focus(U_PW);
                true
            }
        }
    };
    let spec = Spec {
        title: if signed_out { "Sign in to Bitwarden" } else { "Unlock Bitwarden" },
        size: (500, 340),
        ctls: vec![
            ctl(U_INFO, Kind::Label, &info, 18, 12, 464, 38),
            ctl(71, Kind::Label, "Email:", 18, 62, 140, 18),
            ctl(U_EMAIL, Kind::Edit, &email, 164, 58, 318, 24),
            ctl(73, Kind::Label, "Master password:", 18, 96, 140, 18),
            ctl(U_PW, Kind::Password, "", 164, 92, 318, 24),
            ctl(U_STATUS, Kind::Label, "", 18, 126, 464, 36),
            ctl(76, Kind::Label, "Or paste a session token (from `bw unlock --raw` in a terminal); the master password is then not needed.", 18, 170, 464, 40),
            ctl(U_TOKEN, Kind::Password, "", 18, 214, 464, 24),
        ],
        ok: if signed_out { "Sign in" } else { "Unlock" },
        buttons: None,
        on_change: None,
        on_apply: Some(Rc::new(apply)),
    };
    let _ = form::run(host, spec, |f| {
        f.enable(U_EMAIL, signed_out);
        f.focus(if signed_out && email.is_empty() { U_EMAIL } else { U_PW });
    });
    bw::is_unlocked()
}

// ---- the vault ------------------------------------------------------------------------------

const V_SEARCH: usize = 80;
const V_LIST: usize = 81;
const V_PASSWORD: usize = 83;
const V_USERNAME: usize = 84;
const V_TOTP: usize = 85;
const V_NOTES: usize = 86;
const V_INSERT: usize = 90;
const V_COPY: usize = 91;
const V_ADD: usize = 92;
const V_SYNC: usize = 93;
const V_LOCK: usize = 94;
const V_STATUS: usize = 95;

fn fill_list(f: &Form) {
    let query = f.text(V_SEARCH).to_lowercase();
    let mut shown = Vec::new();
    let rows: Vec<Vec<String>> = ITEMS.with(|items| {
        items
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, it)| query.is_empty() || [&it.title, &it.username, &it.url].iter().any(|s| s.to_lowercase().contains(&query)))
            .map(|(i, it)| {
                shown.push(i);
                vec![it.title.clone(), it.username.clone(), it.url.clone()]
            })
            .collect()
    });
    f.list_fill(V_LIST, &[("Title", 200), ("Username", 140), ("URL", 230)], &rows);
    if !rows.is_empty() {
        f.list_select(V_LIST, 0);
    }
    f.set_text(V_STATUS, &format!("{} of {} items", rows.len(), ITEMS.with(|i| i.borrow().len())));
    SHOWN.with(|s| *s.borrow_mut() = shown);
}

fn search_changed(f: &Form, id: usize) {
    if id == V_SEARCH {
        fill_list(f);
    }
}

fn selected_item(f: &Form) -> Option<Item> {
    let row = f.list_selected(V_LIST)?;
    let index = SHOWN.with(|s| s.borrow().get(row).copied())?;
    ITEMS.with(|i| i.borrow().get(index).cloned())
}

fn vault_click(f: &Form, id: usize) {
    if busy() {
        return;
    }
    match id {
        V_INSERT | V_COPY => {
            let Some(item) = selected_item(f) else {
                f.set_text(V_STATUS, "Select an item first.");
                return;
            };
            let field = if f.checked(V_USERNAME) {
                "username"
            } else if f.checked(V_TOTP) {
                "totp"
            } else if f.checked(V_NOTES) {
                "notes"
            } else {
                "password"
            };
            let got = work(f, V_STATUS, "Fetching...", move |_| bw::get_field(&item.id, field));
            match got {
                Ok(v) => {
                    SECRET.with(|s| *s.borrow_mut() = Some(Zeroizing::new(v)));
                    ACT.with(|a| a.set(if id == V_INSERT { Act::Insert } else { Act::Copy }));
                    f.close(true);
                }
                Err(notepp_core::error::Error::Locked) => {
                    ACT.with(|a| a.set(Act::Relock));
                    f.close(true);
                }
                Err(e) => f.set_text(V_STATUS, &e.to_string()),
            }
        }
        V_ADD => {
            ACT.with(|a| a.set(Act::Add));
            f.close(true);
        }
        V_SYNC => {
            let r = work(f, V_STATUS, "Syncing with Bitwarden...", |_| bw::sync().and_then(|()| bw::list("")));
            match r {
                Ok(items) => {
                    ITEMS.with(|i| *i.borrow_mut() = items);
                    fill_list(f);
                }
                Err(notepp_core::error::Error::Locked) => {
                    ACT.with(|a| a.set(Act::Relock));
                    f.close(true);
                }
                Err(e) => f.set_text(V_STATUS, &e.to_string()),
            }
        }
        V_LOCK => {
            bw::lock();
            ACT.with(|a| a.set(Act::Close));
            f.close(true);
        }
        _ => {}
    }
}

fn vault_screen(host: &Frame, can_insert: bool) -> Act {
    ACT.with(|a| a.set(Act::Close));
    // Item titles and usernames only; secrets are fetched one at a time when you use them.
    match bw::list("") {
        Ok(items) => ITEMS.with(|i| *i.borrow_mut() = items),
        Err(notepp_core::error::Error::Locked) => return Act::Relock,
        Err(e) => {
            show_error(host, &format!("Cannot read the vault\n\n{e}"));
            return Act::Close;
        }
    }
    form::on_edit(search_changed);
    let spec = Spec {
        title: "Passwords (Bitwarden)",
        size: (640, 424),
        ctls: vec![
            ctl(79, Kind::Label, "Search:", 16, 16, 56, 18),
            ctl(V_SEARCH, Kind::Edit, "", 76, 12, 548, 24),
            ctl(V_LIST, Kind::List, "", 16, 46, 608, 236),
            ctl(87, Kind::Label, "Use:", 16, 296, 36, 18),
            ctl(V_PASSWORD, Kind::Radio(true), "Password", 56, 294, 90, 20),
            ctl(V_USERNAME, Kind::Radio(false), "Username", 152, 294, 90, 20),
            ctl(V_TOTP, Kind::Radio(false), "One-time code", 248, 294, 120, 20),
            ctl(V_NOTES, Kind::Radio(false), "Notes", 374, 294, 80, 20),
            ctl(V_INSERT, Kind::Button, "Insert at cursor", 16, 326, 130, 28),
            ctl(V_COPY, Kind::Button, "Copy (clears in 30 s)", 152, 326, 158, 28),
            ctl(V_ADD, Kind::Button, "Add item...", 316, 326, 100, 28),
            ctl(V_SYNC, Kind::Button, "Sync", 422, 326, 70, 28),
            ctl(V_LOCK, Kind::Button, "Lock", 498, 326, 70, 28),
            ctl(V_STATUS, Kind::Label, "", 16, 364, 608, 20),
        ],
        ok: "Close",
        buttons: None,
        on_change: Some(vault_click),
        on_apply: None,
    };
    let _ = form::run(host, spec, |f| {
        f.set_checked(V_PASSWORD, true);
        f.enable(V_INSERT, can_insert);
        fill_list(f);
        f.focus(V_SEARCH);
    });
    ACT.with(std::cell::Cell::get)
}

mod add;
use add::*;

// ---- the whole flow -------------------------------------------------------------------------

/// Passwords. `can_insert` is false on tabs that have no text editor to insert into.
pub fn run(host: &Frame, can_insert: bool) -> Outcome {
    let mut asked_env = false;
    loop {
        if !bw::is_installed() && !install_screen(host) {
            return Outcome::Closed;
        }
        if !bw::is_unlocked() {
            if !asked_env {
                asked_env = true;
                if bw::adopt_env_session() {
                    continue;
                }
            }
            let auth = bw::auth_status();
            if let AuthState::Unknown(e) = &auth.state {
                show_error(host, &format!("Cannot talk to the Bitwarden CLI\n\n{e}"));
                return Outcome::Closed;
            }
            if !unlock_screen(host, &auth) {
                return Outcome::Closed;
            }
            continue;
        }
        match vault_screen(host, can_insert) {
            Act::Insert => return SECRET.with(|s| s.borrow_mut().take()).map_or(Outcome::Closed, Outcome::Insert),
            Act::Copy => return SECRET.with(|s| s.borrow_mut().take()).map_or(Outcome::Closed, Outcome::Copy),
            Act::Add => {
                add_screen(host);
            }
            Act::Relock => {}
            Act::Close => return Outcome::Closed,
        }
    }
}
