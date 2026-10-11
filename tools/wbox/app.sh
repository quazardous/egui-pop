#!/usr/bin/env bash
# The gallery launched inside the compositor. Options come from the
# environment so a driver can pick the theme and page per run:
#
#   GALLERY_THEME       pop-dark | pop-light | pop-high-contrast
#   GALLERY_DENSITY     comfortable | compact
#   GALLERY_ACCENT      gold | blue | green | amber | red (default: the theme's)
#   GALLERY_PAGE        overview | tokens | builtins | zones | settings | debug
#   GALLERY_QUIT_AFTER  frames before the gallery exits by itself (default: never)
#   GALLERY_OUT         where its stdout (the JSON start and quit lines) and
#                       stderr go (default: ./out)
#
# Build first: cargo build --release -p egui-pop-gallery
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="$here/../../target/release/egui-pop-gallery"
out="${GALLERY_OUT:-$here/out}"
mkdir -p "$out"
args=(--theme "${GALLERY_THEME:-pop-dark}"
      --density "${GALLERY_DENSITY:-comfortable}"
      --page "${GALLERY_PAGE:-overview}")
[ -n "${GALLERY_ACCENT:-}" ] && args+=(--accent "$GALLERY_ACCENT")
[ -n "${GALLERY_QUIT_AFTER:-}" ] && args+=(--quit-after "$GALLERY_QUIT_AFTER")
exec "$bin" "${args[@]}" > "$out/gallery.jsonl" 2> "$out/gallery.log"
