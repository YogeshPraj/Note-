//! Passwords: adding a vault item, with a generated password.

use super::*;

// ---- add an item ----------------------------------------------------------------------------

pub(super) const A_TITLE: usize = 101;
pub(super) const A_USER: usize = 103;
pub(super) const A_PW: usize = 105;
pub(super) const A_SHOW: usize = 106;
pub(super) const A_GEN: usize = 107;
pub(super) const A_LEN: usize = 109;
pub(super) const A_SYMBOLS: usize = 110;
pub(super) const A_URL: usize = 112;
pub(super) const A_NOTES: usize = 114;
pub(super) const A_STATUS: usize = 115;

pub(super) fn add_click(f: &Form, id: usize) {
    if busy() {
        return;
    }
    match id {
        A_SHOW => f.set_masked(A_PW, !f.checked(A_SHOW)),
        A_GEN => {
            let opts = GenOpts { length: f.text(A_LEN).trim().parse().unwrap_or(20), symbols: f.checked(A_SYMBOLS), ..Default::default() };
            match work(f, A_STATUS, "Generating...", move |_| bw::generate(&opts)) {
                Ok(p) => {
                    f.set_text(A_PW, &p);
                    f.set_checked(A_SHOW, true);
                    f.set_masked(A_PW, false);
                    f.set_text(A_STATUS, "A new password was generated.");
                }
                Err(e) => f.set_text(A_STATUS, &e.to_string()),
            }
        }
        _ => {}
    }
}

/// Add a login to the vault. True when it was added.
pub(super) fn add_screen(host: &Frame) -> bool {
    let done = Arc::new(Mutex::new(false));
    let flag = done.clone();
    let apply = move |v: &Values, f: &Form| {
        let item = NewItem {
            title: v.text(A_TITLE).trim().to_string(),
            username: v.text(A_USER).to_string(),
            password: v.text(A_PW).to_string(),
            url: v.text(A_URL).trim().to_string(),
            notes: v.text(A_NOTES).to_string(),
        };
        if item.title.is_empty() {
            f.set_text(A_STATUS, "A title is required.");
            return true;
        }
        match work(f, A_STATUS, "Adding to your vault...", move |_| bw::create_item(&item)) {
            Ok(()) => {
                if let Ok(mut g) = flag.lock() {
                    *g = true;
                }
                false
            }
            Err(e) => {
                f.set_text(A_STATUS, &e.to_string());
                true
            }
        }
    };
    let spec = Spec {
        title: "Add item to Bitwarden",
        size: (520, 500),
        ctls: vec![
            ctl(100, Kind::Label, "Title:", 18, 20, 100, 18),
            ctl(A_TITLE, Kind::Edit, "", 124, 16, 378, 24),
            ctl(102, Kind::Label, "Username:", 18, 54, 100, 18),
            ctl(A_USER, Kind::Edit, "", 124, 50, 378, 24),
            ctl(104, Kind::Label, "Password:", 18, 88, 100, 18),
            ctl(A_PW, Kind::Password, "", 124, 84, 378, 24),
            ctl(A_SHOW, Kind::Check, "Show", 124, 114, 70, 20),
            ctl(A_GEN, Kind::Button, "Generate", 200, 110, 90, 26),
            ctl(108, Kind::Label, "Length:", 300, 116, 52, 18),
            ctl(A_LEN, Kind::Edit, "20", 354, 112, 46, 22),
            ctl(A_SYMBOLS, Kind::Check, "Symbols", 410, 114, 92, 20),
            ctl(111, Kind::Label, "Website:", 18, 152, 100, 18),
            ctl(A_URL, Kind::Edit, "", 124, 148, 378, 24),
            ctl(113, Kind::Label, "Notes:", 18, 186, 100, 18),
            ctl(A_NOTES, Kind::MultiEdit(false), "", 124, 184, 378, 200),
            ctl(A_STATUS, Kind::Label, "", 18, 396, 484, 40),
        ],
        ok: "Add",
        buttons: None,
        on_change: Some(add_click),
        on_apply: Some(Rc::new(apply)),
    };
    let _ = form::run(host, spec, |f| f.focus(A_TITLE));
    done.lock().is_ok_and(|g| *g)
}
