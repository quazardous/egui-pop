#!/usr/bin/env python3
"""Checks the links of the site's landing page in the assembled site.

    python3 tools/site/links.py <site> [--page index.html]

<site> is the folder deployed to GitHub Pages: the landing page at its
root, the gallery under gallery/, the docs under docs/, the API under
api/. Every relative `href` and `src` of the page must lead to a file of
the site (a folder to its index.html), and an anchor to an id of its
page. External links are left alone. It needs Python 3 alone.
"""

import argparse
import os
import re
import sys
from urllib.parse import unquote, urlsplit

ATTR = re.compile(r'\b(href|src)="([^"]*)"')
EXTERNAL = re.compile(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:|//)")
ID = re.compile(r'\bid="([^"]*)"')


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("site", help="the assembled site")
    parser.add_argument("--page", default="index.html", help="the page checked, in the site")
    args = parser.parse_args()

    site = os.path.abspath(args.site)
    page = os.path.join(site, args.page)
    with open(page, encoding="utf-8") as f:
        html = f.read()
    ids = set(ID.findall(html))
    base = os.path.dirname(page)

    broken = []
    checked = 0
    for attr, link in ATTR.findall(html):
        if not link or EXTERNAL.match(link):
            continue
        parts = urlsplit(link)
        if not parts.path:
            if parts.fragment and unquote(parts.fragment) not in ids:
                broken.append(f"{attr}={link}: no such anchor")
            checked += 1
            continue
        if parts.path.startswith("/"):
            broken.append(f"{attr}={link}: absolute; the site lives under the repository's name")
            continue
        target = os.path.normpath(os.path.join(base, unquote(parts.path)))
        if not target.startswith(site):
            broken.append(f"{attr}={link}: leaves the site")
            continue
        if os.path.isdir(target):
            target = os.path.join(target, "index.html")
        if not os.path.isfile(target):
            broken.append(f"{attr}={link}: nothing at {os.path.relpath(target, site)}")
        checked += 1

    if broken:
        print(f"{args.page}: {len(broken)} broken link(s):", file=sys.stderr)
        for b in broken:
            print(f"  {b}", file=sys.stderr)
        return 1
    print(f"{args.page}: {checked} relative links lead somewhere")
    return 0


if __name__ == "__main__":
    sys.exit(main())
