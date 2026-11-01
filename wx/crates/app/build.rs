// Embed the icon, the visual-styles manifest and version information (what Task Manager and
// "Open with" show). On non-Windows targets this does nothing.
// A build script reports to cargo on stdout and aborts the build by panicking.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]
use std::path::PathBuf;

fn root_version(root: &std::path::Path) -> String {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    text.lines().find_map(|l| l.strip_prefix("version = \"").and_then(|r| r.split('"').next().map(String::from))).unwrap_or_else(|| "0.0.0".into())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/app -> crates -> wx -> repository root.
    let root = manifest_dir.ancestors().nth(3).expect("repository root").to_path_buf();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let version = root_version(&root);
    let parts: Vec<&str> = version.split('.').collect();
    let (major, minor, patch) = (parts.first().unwrap_or(&"0"), parts.get(1).unwrap_or(&"0"), parts.get(2).unwrap_or(&"0"));
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
        include = root.join("app.rc").to_string_lossy().replace('\\', "/"),
    );
    let wrapper = out_dir.join("app_gen.rc");
    std::fs::write(&wrapper, rc).expect("write resource file");
    let obj = out_dir.join("app_res.o");
    // app.rc names its files relative to the repository root.
    let status = std::process::Command::new("windres").current_dir(&root).arg(&wrapper).args(["-O", "coff", "-o"]).arg(&obj).status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", obj.display()),
        _ => panic!("windres failed (it ships with the WinLibs MinGW package)"),
    }
    println!("cargo:rerun-if-changed=../../../app.rc");
    println!("cargo:rerun-if-changed=../../../assets/app.ico");
    println!("cargo:rerun-if-changed=../../../assets/app.manifest");
}
