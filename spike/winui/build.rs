// Embed Note++'s own manifest (Windows 10 support, DPI awareness, common controls): layered child windows need the
// Windows 8 or later declaration. A build script reports to cargo on stdout and aborts by panicking.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = root.join("assets/app.manifest").canonicalize().expect("assets/app.manifest");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("spike.rc");
    let path = manifest.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
    std::fs::write(&rc, format!("1 24 \"{path}\"\n")).unwrap();
    let obj = out.join("spike_res.o");
    let ok = std::process::Command::new("windres").arg(&rc).args(["-O", "coff", "-o"]).arg(&obj).status().is_ok_and(|s| s.success());
    if !ok {
        panic!("windres failed");
    }
    println!("cargo:rustc-link-arg-bins={}", obj.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", manifest.display());
}
