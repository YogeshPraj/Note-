#!/usr/bin/env bash
# Package the macOS or Linux release build (run from the repository root, after
# `node scripts/setup-assets.js` and `cargo build -p notepp-wx --release` in wx/).
#
#   scripts/package-unix.sh linux-x64     -> dist/release/NotePP-linux-x64.tar.gz
#   scripts/package-unix.sh macos-arm64   -> dist/release/NotePP-macos-arm64.zip  (Note++.app, unsigned)
#
# The app looks for its whiteboard/ folder next to the executable.
set -euo pipefail

target="${1:?usage: package-unix.sh linux-x64|macos-arm64}"
root="$(cd "$(dirname "$0")/.." && pwd)"
exe="$root/wx/target/release/notepp-wx"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n1)"
out="$root/dist/release"
mkdir -p "$out"

[ -x "$exe" ] || { echo "missing $exe" >&2; exit 1; }
[ -f "$root/assets/whiteboard/whiteboard.bundle.js" ] || { echo "run: node scripts/setup-assets.js" >&2; exit 1; }

stage="$root/dist/stage"
rm -rf "$stage"
mkdir -p "$stage"

case "$target" in
  linux-*)
    app="$stage/NotePP"
    mkdir -p "$app"
    cp "$exe" "$app/notepp"
    cp -R "$root/assets/whiteboard" "$app/whiteboard"
    tar -C "$stage" -czf "$out/NotePP-$target.tar.gz" NotePP
    file="NotePP-$target.tar.gz"
    ;;
  macos-*)
    app="$stage/Note++.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$exe" "$app/Contents/MacOS/notepp"
    cp -R "$root/assets/whiteboard" "$app/Contents/MacOS/whiteboard"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Note++</string>
  <key>CFBundleDisplayName</key><string>Note++</string>
  <key>CFBundleIdentifier</key><string>io.github.yogeshpraj.notepp</string>
  <key>CFBundleExecutable</key><string>notepp</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
    ditto -c -k --keepParent "$app" "$out/NotePP-$target.zip"
    file="NotePP-$target.zip"
    ;;
  *) echo "unknown target $target" >&2; exit 1 ;;
esac

( cd "$out" && if command -v sha256sum >/dev/null; then sha256sum "$file"; else shasum -a 256 "$file"; fi > "$file.sha256" )
echo "Ready: $out/$file"
