#!/usr/bin/env python3
"""The semver job's exclusions, read from crates.io.

`cargo semver-checks` compares each published crate with its latest release
on crates.io and fails on a crate that has none. CI's semver job
(.github/workflows/ci.yml) leaves such crates out: this script prints an
`--exclude <crate>` for each workspace crate that is to be published and
has no release on crates.io yet, so a new crate needs no hand-kept list
and drops out of the list by itself once released. What it found goes to
standard error; it fails when crates.io's index cannot be read.

Usage: python3 tools/semver-excludes.py   (from the repository's root)
"""

import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

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


def main():
    crates = publishable()
    unreleased = [c for c in crates if not released(c)]
    print(
        f"semver-excludes: {len(crates)} crates to publish, "
        f"{len(unreleased)} without a release on crates.io, left out: "
        f"{', '.join(unreleased) or 'none'}",
        file=sys.stderr,
    )
    print(" ".join(f"--exclude {c}" for c in unreleased))


if __name__ == "__main__":
    main()
