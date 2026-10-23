// Link the Lexilla static library we built with MinGW.
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lexilla/bin");
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=lexilla");
    // Lexilla is C++, so the GNU toolchain needs libstdc++ explicitly.
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rerun-if-changed=lexilla/bin/liblexilla.a");
}
