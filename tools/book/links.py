#!/usr/bin/env python3
"""Fixes and checks the links of the docs site built by mdBook.

    python3 tools/book/links.py target/book [--ref main]

The book renders docs/ in place, so a page's relative links work on GitHub
and on the site alike, except those leading out of the book: to a file
outside docs/ (a crate's folder, the CHANGELOG, GOVERNANCE.md) or to a
page not in docs/SUMMARY.md (the upstream drafts). mdBook writes them as
they are, with `.md` turned into `.html`, and they would lead nowhere.

This script reads every page of the built book and rewrites each such
link to the file on GitHub (a folder to its tree, an image to its raw
file), then fails if a relative link still leads to nothing, in the book
or in the repository, or to an anchor its page in the book lacks. It needs Python 3 alone and edits the pages in
place.
"""

import argparse
import os
import posixpath
import re
import sys
from urllib.parse import unquote

REPO = "https://github.com/quazardous/egui-pop"
ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
DOCS = "docs"

ATTR = re.compile(r'\b(href|src)="([^"]*)"')
EXTERNAL = re.compile(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:|//|/)")
ID = re.compile(r'\bid="([^"]*)"')


def repo_path(page_dir, target):
    """The repository file or folder a link of the book points to, or None."""
    path = posixpath.normpath(posixpath.join(DOCS, page_dir, target))
    if path.startswith("../"):
        return None
    candidates = [path]
    if path.endswith(".html"):
        candidates.append(path[: -len(".html")] + ".md")
    for c in candidates:
        if os.path.exists(os.path.join(ROOT, c)):
            return c
    return None


def github_url(path, attr, ref):
    if attr == "src":
        return f"{REPO}/raw/{ref}/{path}"
    kind = "tree" if os.path.isdir(os.path.join(ROOT, path)) else "blob"
    return f"{REPO}/{kind}/{ref}/{path}"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("book", help="the built book (mdBook's build-dir)")
    parser.add_argument("--ref", default="main", help="the branch GitHub links point to")
    args = parser.parse_args()

    book = os.path.abspath(args.book)
    ids = {}

    def has_anchor(local, fragment):
        if local not in ids:
            with open(os.path.join(book, local), encoding="utf-8") as f:
                ids[local] = set(ID.findall(f.read()))
        return unquote(fragment) in ids[local]

    rewritten = 0
    broken = []
    for dirpath, _, files in os.walk(book):
        for name in files:
            if not name.endswith(".html"):
                continue
            file = os.path.join(dirpath, name)
            page_dir = posixpath.relpath(dirpath, book).replace(os.sep, "/")
            page_dir = "" if page_dir == "." else page_dir
            # print.html gathers every chapter; mdBook points its links at
            # anchors of the page itself, so it is checked as any page.
            with open(file, encoding="utf-8") as f:
                html = f.read()

            def fix(m):
                nonlocal rewritten
                attr, value = m.group(1), m.group(2)
                if not value or EXTERNAL.match(value) or "{{" in value:
                    return m.group(0)
                target, sep, fragment = value.partition("#")
                target = target.partition("?")[0]
                local = posixpath.normpath(posixpath.join(page_dir, unquote(target or name)))
                if not local.startswith("../") and os.path.exists(os.path.join(book, local)):
                    if fragment and local.endswith(".html") and not has_anchor(local, fragment):
                        broken.append(f"{posixpath.join(page_dir, name)}: {value} (no such anchor)")
                    return m.group(0)
                source = repo_path(page_dir, unquote(target))
                if source is None:
                    broken.append(f"{posixpath.join(page_dir, name)}: {value}")
                    return m.group(0)
                rewritten += 1
                return f'{attr}="{github_url(source, attr, args.ref)}{sep}{fragment}"'

            new = ATTR.sub(fix, html)
            if new != html:
                with open(file, "w", encoding="utf-8") as f:
                    f.write(new)

    print(f"{rewritten} links rewritten to {REPO}")
    if broken:
        print(f"{len(broken)} broken links:", file=sys.stderr)
        for b in sorted(set(broken)):
            print(f"  {b}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
