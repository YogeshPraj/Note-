// Link the Lexilla static library built by scripts/setup.ps1, and put
// Scintilla.dll next to the executable so `cargo run` / tests can load it.
use std::path::{Path, PathBuf};

fn require(path: &Path, what: &str) {
    if !path.exists() {
        panic!(
            "\n\nMissing {what}: {}\nRun the one-time setup first:\n    powershell -ExecutionPolicy Bypass -File scripts\\setup.ps1\n",
            path.display()
        );
    }
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lexilla = manifest.join("lexilla/bin");
    let scintilla_dll = manifest.join("scintilla/bin/Scintilla.dll");
    let mermaid = manifest.join("assets/mermaid.min.js");

    require(&lexilla.join("liblexilla.a"), "Lexilla library");
    require(&scintilla_dll, "Scintilla.dll");
    require(&mermaid, "Mermaid bundle (embedded by the preview)");

    // The whiteboard page is loaded from disk at run time, not embedded: warn, don't fail.
    if !manifest.join("assets/whiteboard/whiteboard.bundle.js").exists() {
        println!("cargo:warning=whiteboard assets missing - run scripts/setup.ps1 or whiteboards will not load");
    }

    println!("cargo:rustc-link-search=native={}", lexilla.display());
    println!("cargo:rustc-link-lib=static=lexilla");
    // Lexilla is C++, so the GNU toolchain needs libstdc++. Link it statically so no
    // loose libstdc++-6.dll has to be found on PATH (a DLL-planting risk).
    println!("cargo:rustc-link-lib=static=stdc++");

    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out; the exe lives in target/<profile>.
    if let Some(profile_dir) = PathBuf::from(std::env::var("OUT_DIR").unwrap()).ancestors().nth(3) {
        let _ = std::fs::copy(&scintilla_dll, profile_dir.join("Scintilla.dll"));
        // The GNU build imports WebView2Loader.dll; keep it beside the exe so it is found
        // there first instead of through PATH or the current directory.
        let wanted = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") { "arm64" } else { "x64" };
        if let Ok(entries) = std::fs::read_dir(profile_dir.join("build")) {
            for e in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("webview2-com-sys-")) {
                let dll = e.path().join("out").join(wanted).join("WebView2Loader.dll");
                if dll.exists() {
                    let _ = std::fs::copy(&dll, profile_dir.join("WebView2Loader.dll"));
                    break;
                }
            }
        }
    }

    println!("cargo:rerun-if-changed=lexilla/bin/liblexilla.a");
    println!("cargo:rerun-if-changed=scintilla/bin/Scintilla.dll");
    println!("cargo:rerun-if-changed=assets/mermaid.min.js");
    println!("cargo:rerun-if-changed=build.rs");
}
