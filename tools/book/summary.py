#!/usr/bin/env python3
"""Checks that the book's table of contents lists every page of the index.

    python3 tools/book/summary.py

docs/README.md, the docs index, is what readers on GitHub follow;
docs/SUMMARY.md is the site's table of contents. A page the index links
but the summary leaves out is built by nobody: on the site its link is
sent to GitHub. This script fails, naming them, when a Markdown page of
docs/ that the index links is missing from the summary. It needs Python 3
alone and reads the sources, not the built book.
"""

import os
import posixpath
import re
import sys
from urllib.parse import unquote

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
DOCS = os.path.join(ROOT, "docs")

# An inline Markdown link's target: [text](target "title").
LINK = re.compile(r"\]\(\s*<?([^)\s>]*)>?(?:\s+\"[^\"]*\")?\s*\)")
EXTERNAL = re.compile(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:|//|/|#)")


def pages(file):
    """The Markdown pages of docs/ a file links, relative to docs/."""
    with open(os.path.join(DOCS, file), encoding="utf-8") as f:
        text = f.read()
    # Code spans and blocks hold no links.
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    text = re.sub(r"`[^`\n]*`", "", text)
    found = set()
    for target in LINK.findall(text):
        if not target or EXTERNAL.match(target):
            continue
        target = unquote(target.partition("#")[0])
        if not target.endswith(".md"):
            continue
        path = posixpath.normpath(posixpath.join(posixpath.dirname(file), target))
        if path.startswith("../"):
            continue
        found.add(path)
    return found


def main():
    index = pages("README.md")
    summary = pages("SUMMARY.md")
    missing = sorted(index - summary)
    print(f"{len(index)} pages linked from docs/README.md, {len(summary)} in docs/SUMMARY.md")
    if missing:
        print(f"{len(missing)} pages of docs/README.md missing from docs/SUMMARY.md:", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
