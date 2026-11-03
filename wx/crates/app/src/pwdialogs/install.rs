//! Passwords: installing the Bitwarden CLI.

use super::*;

// ---- install --------------------------------------------------------------------------------

pub(super) const I_STATUS: usize = 61;
pub(super) const I_GO: usize = 62;

pub(super) fn install_click(f: &Form, id: usize) {
    if id != I_GO || busy() {
        return;
    }
    let result = work(f, I_STATUS, "Starting the download", |p| {
        bw::download(&mut |pct, what| {
            if let Ok(mut g) = p.lock() {
                *g = (pct, what.to_string());
            }
        })
    });
    match result {
        Ok(()) => f.close(true),
        Err(e) => f.set_text(I_STATUS, &e.to_string()),
    }
}

/// Offer to download the Bitwarden CLI. True when it is installed afterwards.
pub(super) fn install_screen(host: &Frame) -> bool {
    let supported = bw::install_info().supported;
    let text = if supported {
        format!(
            "Passwords uses the official Bitwarden command-line client (version {}). It is downloaded once (about 40 MB) from Bitwarden's GitHub release and checked against a fixed checksum before anything is run.\r\n\r\nNote++ stores no credentials and does no password cryptography itself. If you already have `bw` on your PATH it is used instead.",
            bw::BW_VERSION
        )
    } else {
        "Bitwarden does not publish a command-line client for this system.".to_string()
    };
    let spec = Spec {
        title: "Passwords (Bitwarden)",
        size: (500, 270),
        ctls: vec![
            ctl(60, Kind::Label, &text, 18, 14, 464, 130),
            ctl(I_STATUS, Kind::Label, "", 18, 150, 464, 36),
            ctl(I_GO, Kind::Button, "Download Bitwarden CLI", 18, 188, 220, 28),
        ],
        ok: "Close",
        buttons: None,
        on_change: Some(install_click),
        on_apply: None,
    };
    let _ = form::run(host, spec, |f| f.enable(I_GO, supported));
    bw::is_installed()
}
