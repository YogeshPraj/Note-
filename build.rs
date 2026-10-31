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

/// Compile `app.rc` (icon and application manifest: visual styles, no elevation, long paths)
/// plus a version resource built from Cargo.toml (what Task Manager and "Open with" show),
/// with the MinGW resource compiler, and link it into the exe.
fn embed_manifest(manifest_dir: &Path) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let out = out_dir.join("app_res.o");
    let v = |k: &str| std::env::var(k).unwrap_or_else(|_| "0".into());
    let (major, minor, patch) = (v("CARGO_PKG_VERSION_MAJOR"), v("CARGO_PKG_VERSION_MINOR"), v("CARGO_PKG_VERSION_PATCH"));
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let wrapper = out_dir.join("app_gen.rc");
    let rc = format!(
        "#include \"{include}\"\n\
         1 VERSIONINFO\n FILEVERSION {major},{minor},{patch},0\n PRODUCTVERSION {major},{minor},{patch},0\n FILEOS 0x4\n FILETYPE 0x1\n\
         BEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n BLOCK \"040904B0\"\n BEGIN\n\
         VALUE \"CompanyName\", \"Yogesh Prajapati\"\n\
         VALUE \"FileDescription\", \"Note++\"\n\
         VALUE \"FileVersion\", \"{version}\"\n\
         VALUE \"InternalName\", \"notepp\"\n\
         VALUE \"OriginalFilename\", \"notepp.exe\"\n\
         VALUE \"ProductName\", \"Note++\"\n\
         VALUE \"ProductVersion\", \"{version}\"\n\
         END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n VALUE \"Translation\", 0x0409, 0x04B0\n END\n END\n",
        include = manifest_dir.join("app.rc").to_string_lossy().replace('\\', "/"),
    );
    std::fs::write(&wrapper, rc).expect("write the generated resource file");
    let status = std::process::Command::new("windres").current_dir(manifest_dir).arg(&wrapper).args(["-O", "coff", "-o"]).arg(&out).status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        _ => panic!("\n\nwindres failed (it ships with the WinLibs MinGW package). The app manifest is required.\n"),
    }
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
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

    embed_manifest(&manifest);

    println!("cargo:rerun-if-changed=lexilla/bin/liblexilla.a");
    println!("cargo:rerun-if-changed=scintilla/bin/Scintilla.dll");
    println!("cargo:rerun-if-changed=assets/mermaid.min.js");
    println!("cargo:rerun-if-changed=build.rs");
}
