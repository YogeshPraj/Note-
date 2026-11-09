// The exe's manifest: Note++'s own (`assets/app.manifest`: Windows 10 support, which layered child windows need,
// common controls, DLLs pinned to System32), made per-monitor DPI aware for WinUI, embedded with windres.
//
// With the `self-contained` feature, windows-reactor-setup first stages the Windows App Runtime beside the exe and
// writes its own manifest (the runtime's classes and the marker reactor looks for); the two are merged. The setup
// crate supports only the MSVC and gnullvm targets: on this project's gnu target it does all its work and then
// panics choosing a linker flag, so that last step is caught and the manifest is embedded here instead.
// A build script reports to cargo on stdout and aborts by panicking.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]
use std::path::{Path, PathBuf};

fn setup_manifest(out: &Path) -> Option<String> {
    #[cfg(feature = "self-contained")]
    {
        std::panic::set_hook(Box::new(|_| {}));
        let staged = std::panic::catch_unwind(windows_reactor_setup::as_self_contained);
        let _ = std::panic::take_hook();
        if staged.is_ok() {
            println!("cargo:warning=windows-reactor-setup now supports this target; embed its manifest the usual way");
        }
        Some(std::fs::read_to_string(out.join("app.manifest")).expect("windows-reactor-setup wrote no manifest"))
    }
    #[cfg(not(feature = "self-contained"))]
    {
        let _ = out;
        None
    }
}

/// Ours, per-monitor DPI aware, with the setup manifest's body (and namespaces) added when there is one.
fn merged(ours: &str, setup: Option<&str>) -> String {
    let dpi = r#"<ws1:dpiAware xmlns:ws1="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</ws1:dpiAware>"#;
    let per_monitor = r#"<ws3:dpiAwareness xmlns:ws3="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</ws3:dpiAwareness>"#;
    assert!(ours.contains(dpi), "assets/app.manifest changed its dpiAware line");
    let mut text = ours.replace(dpi, &format!("{dpi}\n      {per_monitor}"));
    if let Some(setup) = setup {
        let body_start = setup.find("<assembly").and_then(|a| setup[a..].find('>').map(|b| a + b + 1)).expect("setup manifest root");
        let body_end = setup.rfind("</assembly>").expect("setup manifest end");
        let root = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">"#;
        assert!(text.contains(root), "assets/app.manifest changed its root");
        text = text.replace(root, r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" xmlns:winrtv1="urn:schemas-microsoft-com:winrt.v1" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3" manifestVersion="1.0">"#);
        // The setup body brings its own <description> (reactor's marker); a manifest may have only one.
        text = text.replace("  <description>Note++</description>\n", "").replace("  <description>Note++</description>\r\n", "");
        let end = text.rfind("</assembly>").expect("our manifest end");
        text.insert_str(end, &setup[body_start..body_end]);
    }
    text
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ours = root.join("assets/app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", ours.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let setup = setup_manifest(&out);
    let manifest = out.join("spike.manifest");
    std::fs::write(&manifest, merged(&std::fs::read_to_string(&ours).unwrap(), setup.as_deref())).unwrap();
    let rc = out.join("spike.rc");
    std::fs::write(&rc, format!("1 24 \"{}\"\n", manifest.to_string_lossy().replace('\\', "/"))).unwrap();
    let obj = out.join("spike_res.o");
    let ok = std::process::Command::new("windres").arg(&rc).args(["-O", "coff", "-o"]).arg(&obj).status().is_ok_and(|s| s.success());
    if !ok {
        panic!("windres failed");
    }
    println!("cargo:rustc-link-arg-bins={}", obj.display());
}
