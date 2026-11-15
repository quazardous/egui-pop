#!/bin/sh
# Every published crate builds on its own, as docs.rs and a user depending
# on it alone build it: `cargo check -p <crate>` resolves the features of
# that crate only, where a workspace build unifies them across every member
# (a crate can lean on a feature another member turns on, and build only
# beside it). Each crate is checked with its default features, and without
# them when it has any.
# Usage: tools/check-alone.sh [<crate>...]   (default: every published crate)
set -u
# One line per crate: its name and whether it has default features
# (`publish = false` reads as an empty list in cargo metadata).
crates=$(cargo metadata --no-deps --format-version 1 --locked | python3 -c '
import json, sys
only = sys.argv[1:]
for p in json.load(sys.stdin)["packages"]:
    if p["publish"] != [] and (not only or p["name"] in only):
        print(p["name"], "yes" if p["features"].get("default") else "no")
' "$@") || exit 1
failed=
check() {
  echo "::group::cargo check -p $*"
  if cargo check --locked -p "$@" </dev/null; then
    echo "::endgroup::"
  else
    echo "::endgroup::"
    echo "::error::$* does not build on its own"
    failed="$failed
  $*"
  fi
}
while read -r crate has_default; do
  check "$crate"
  if [ "$has_default" = yes ]; then
    check "$crate" --no-default-features
  fi
done <<EOF
$crates
EOF
if [ -n "$failed" ]; then
  echo "Crates that do not build on their own:$failed"
  exit 1
fi
echo "Every published crate builds on its own."
