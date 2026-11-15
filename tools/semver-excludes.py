#!/usr/bin/env python3
"""The semver job's exclusions follow crates.io.

`cargo semver-checks` compares each published crate with its latest release
on crates.io and fails on a crate that has none. CI's semver job
(.github/workflows/ci.yml) therefore leaves such crates out with
`--exclude`. This script fails when a workspace crate that is to be
published has no release on crates.io yet and is missing from one of those
`--workspace` commands: a new crate adds its `--exclude` in the pull
request that adds the crate, not after `next` went red. It warns about an
excluded crate that has a release by now (a notice, not a failure).

Usage: python3 tools/semver-excludes.py   (from the repository's root)
"""

import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

WORKFLOW = ".github/workflows/ci.yml"


def publishable():
    """The workspace crates `cargo publish` would take."""
    meta = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"]
        )
    )
    # `publish = false` comes as an empty list of registries.
    return sorted(p["name"] for p in meta["packages"] if p.get("publish") != [])


def index_path(name):
    """A crate's file in crates.io's sparse index."""
    n = name.lower()
    if len(n) <= 2:
        return f"{len(n)}/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


def released(name):
    """Whether crates.io has a release of the crate."""
    url = f"https://index.crates.io/{index_path(name)}"
    request = urllib.request.Request(url, headers={"User-Agent": "semver-excludes"})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=20) as answer:
                return bool(answer.read().strip())
        except urllib.error.HTTPError as e:
            if e.code in (403, 404):
                return False
            error = e
        except OSError as e:
            error = e
        time.sleep(2 * (attempt + 1))
    sys.exit(f"semver-excludes: crates.io's index cannot be read ({url}): {error}")


def commands():
    """Each `cargo semver-checks ... --workspace` command, as its excludes."""
    text = open(WORKFLOW, encoding="utf-8").read()
    # A command may go on over lines ending with a backslash.
    text = re.sub(r"\\\n\s*", " ", text)
    found = []
    for line in text.splitlines():
        if "cargo semver-checks" in line and "--workspace" in line:
            found.append(set(re.findall(r"--exclude[ =]([A-Za-z0-9_-]+)", line)))
    return found


def main():
    excludes = commands()
    if not excludes:
        sys.exit(f"semver-excludes: no `cargo semver-checks --workspace` in {WORKFLOW}")
    crates = publishable()
    unreleased = [c for c in crates if not released(c)]
    problems = [
        f"{crate} has no release on crates.io: add `--exclude {crate}` to "
        f"every `cargo semver-checks check-release --workspace` of {WORKFLOW}"
        for crate in unreleased
        if any(crate not in e for e in excludes)
    ]
    for crate in sorted(set.union(*excludes)):
        if crate not in crates:
            print(f"::warning::{WORKFLOW} excludes {crate}, which is not a published workspace crate")
        elif crate not in unreleased and all(crate in e for e in excludes):
            print(f"::notice::{crate} has a release on crates.io: its `--exclude` in {WORKFLOW} can go")
    for p in problems:
        print(f"::error::{p}")
    if problems:
        sys.exit(1)
    print(
        f"semver-excludes: {len(crates)} crates to publish, "
        f"{len(unreleased)} without a release, all excluded: {', '.join(unreleased) or 'none'}"
    )


if __name__ == "__main__":
    main()
