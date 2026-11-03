#!/usr/bin/env bash
# Fails if a layer depends on something it must not.
#   notepp-core (root workspace)  : no UI toolkit, no ui-kit, no app
#   ui-kit      (wx workspace)    : no notepp-core, no app
# Run from anywhere; needs the Rust toolchain on PATH (see wx/README.md for the wx build environment).
set -euo pipefail
cd "$(dirname "$0")/.."

UI_PATTERN='wxdragon|wxdragon-sys|egui|eframe|iced|slint|gtk|winit|wgpu|tauri|dioxus'
fail=0

deps() { cargo tree "$@" -e normal --prefix none 2>/dev/null | awk '{print $1}' | sort -u; }

core_deps=$(deps -p notepp-core)
if grep -Eq "^($UI_PATTERN)$" <<<"$core_deps"; then
  echo "ARCH VIOLATION: notepp-core depends on a UI framework" >&2; fail=1
fi
for forbidden in ui-kit notepp-wx; do
  if grep -Eq "^$forbidden$" <<<"$core_deps"; then
    echo "ARCH VIOLATION: notepp-core depends on $forbidden" >&2; fail=1
  fi
done

kit_deps=$(cd wx && deps -p ui-kit)
for forbidden in notepp-core notepp-wx; do
  if grep -Eq "^$forbidden$" <<<"$kit_deps"; then
    echo "ARCH VIOLATION: ui-kit depends on $forbidden" >&2; fail=1
  fi
done

# Source-level guard: the kit must not name the application or the core even if a manifest is edited later.
if grep -rEn 'notepp_core|crate::(app|dispatch|handlers)' wx/crates/ui-kit/src >/dev/null 2>&1; then
  echo "ARCH VIOLATION: ui-kit source refers to notepp_core or the app" >&2; fail=1
fi

# Native message boxes and pickers are built only by ui_kit::ask, so how the program asks is decided in one place.
if grep -rEn '(Message|File|Dir|SingleChoice|MultiChoice|Colour|Font|TextEntry)Dialog::builder' wx/crates --include='*.rs' | grep -v 'components/ask.rs' >/dev/null; then
  echo "ARCH VIOLATION: a native dialog is built outside ui_kit::ask" >&2; fail=1
fi

# Files stay at most 400 lines (CLAUDE.md, Code rules): split by responsibility instead.
long=$(find crates wx/crates -name '*.rs' -not -path '*/target/*' -exec wc -l {} + | awk '$2 != "total" && $1 > 400 {print "  " $2 " (" $1 ")"}')
if [ -n "$long" ]; then
  echo "ARCH VIOLATION: files over 400 lines:" >&2; echo "$long" >&2; fail=1
fi

[ "$fail" -eq 0 ] && echo "architecture OK"
exit $fail
