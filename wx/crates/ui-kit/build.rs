// Embeds the visual-styles manifest and icon into the kit's examples (the gallery), so stock controls look native.
// The kit as a library carries no resources; the application embeds its own (see the app's build script).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // crates/ui-kit -> crates -> wx -> repository root.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).expect("repository root").to_path_buf();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("kit_res.o");
    let status = std::process::Command::new("windres").current_dir(&root).arg(root.join("app.rc")).args(["-O", "coff", "-o"]).arg(&out).status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-examples={}", out.display()),
        _ => println!("cargo:warning=windres failed; the gallery will use classic controls"),
    }
    println!("cargo:rerun-if-changed=../../../app.rc");
}
