#!/usr/bin/env bash
# Start the packaged macOS or Linux build and check that it really works: its window opens and its whiteboard page (WebKit) is
# ready. Run from the repository root after scripts/package-unix.sh. Exits non-zero, with what was seen, when it does not.
#
#   scripts/start-check.sh linux-x64      (needs a display: the build server runs it under xvfb-run)
#   scripts/start-check.sh macos-arm64
set -euo pipefail

target="${1:?usage: start-check.sh linux-x64|macos-arm64}"
root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n1)"
# How long a slow build server may take to show the window and load a web page.
limit="${START_CHECK_SECONDS:-90}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
case "$target" in
  linux-*) tar -C "$work" -xzf "$root/dist/release/NotePP-$target.tar.gz"; exe="$work/NotePP/notepp" ;;
  macos-*) ditto -x -k "$root/dist/release/NotePP-$target.zip" "$work"; exe="$work/Note++.app/Contents/MacOS/notepp" ;;
  *) echo "unknown target $target" >&2; exit 1 ;;
esac
[ -x "$exe" ] || { echo "no program in the package: $exe" >&2; exit 1; }

# A profile of its own, and a whiteboard to open, so the web page has to load.
mkdir -p "$work/profile"
board="$work/check.excalidraw"
printf '%s' '{"type":"excalidraw","version":2,"source":"check","elements":[],"appState":{},"files":{}}' > "$board"
report="$work/started.txt"

APPDATA="$work/profile" NOTEPP_START_CHECK="$report" "$exe" --new-instance "$board" > "$work/out.txt" 2>&1 &
app=$!

seen() { grep -qx "$1 $version" "$report" 2>/dev/null; }
waited=0
while [ "$waited" -lt "$limit" ]; do
  if ! kill -0 "$app" 2>/dev/null; then
    echo "Note++ ended during the start check" >&2
    break
  fi
  if seen window && seen whiteboard; then
    break
  fi
  sleep 1
  waited=$((waited + 1))
done

# Still running a few seconds after it said it was ready: a crash right after showing the window counts as a failure.
sleep 3
alive=no
kill -0 "$app" 2>/dev/null && alive=yes
kill "$app" 2>/dev/null || true
wait "$app" 2>/dev/null || true

if seen window && seen whiteboard && [ "$alive" = yes ]; then
  echo "Start check passed: window and whiteboard page ($target, ${waited}s)"
  exit 0
fi
echo "Start check FAILED for $target after ${waited}s" >&2
echo "--- what Note++ reported:" >&2
cat "$report" 2>/dev/null >&2 || echo "(nothing)" >&2
echo "--- still running at the end: $alive" >&2
echo "--- its output:" >&2
tail -n 40 "$work/out.txt" >&2 || true
exit 1
