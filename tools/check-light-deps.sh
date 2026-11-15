#!/bin/sh
# A view or an editor used in a plain egui app stays light: each crate
# listed in tools/light-deps.txt, built without its default features,
# depends neither on the application layer (egui-pop-api) nor on eframe,
# and its tree holds no more crates than the number recorded there.
# Usage: tools/check-light-deps.sh
set -u
cd "$(dirname "$0")/.." || exit 1
failed=0
while read -r crate max; do
  case "$crate" in '' | '#'*) continue ;; esac
  tree=$(cargo tree --locked -p "$crate" --no-default-features -e normal --prefix none </dev/null) || {
    echo "::error::cargo tree failed for $crate"
    failed=1
    continue
  }
  heavy=$(printf '%s\n' "$tree" | awk '$1 == "egui-pop-api" || $1 == "eframe" { print $1 }' | sort -u)
  if [ -n "$heavy" ]; then
    echo "::error::$crate without its default features depends on:" $heavy
    failed=1
  fi
  count=$(printf '%s\n' "$tree" | awk '{ print $1, $2 }' | sort -u | wc -l)
  if [ "$count" -gt "$max" ]; then
    echo "::error::$crate without its default features has $count crates in its tree, more than the $max recorded in tools/light-deps.txt"
    failed=1
  elif [ "$count" -lt "$max" ]; then
    echo "$crate: $count crates, under the $max recorded: lower it in tools/light-deps.txt"
  else
    echo "$crate: $count crates (at most $max)"
  fi
done < tools/light-deps.txt
exit $failed
