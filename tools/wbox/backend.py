"""The compositor a wbox scenario runs in.

`WBOX_COMPOSITOR` (labwc, mutter, ...) overrides the config's
`compositor`; the wbox library builds it either way. `mutter` is wbox's
headless GNOME compositor (wbox-mcp 0.14 with its `mutter` extra).
"""

from __future__ import annotations

import os

from wbox.server import build_compositor as wbox_build


def compositor_name(cfg: dict) -> str:
    return os.environ.get("WBOX_COMPOSITOR") or cfg.get("compositor", "labwc")


def build_compositor(cfg: dict):
    return wbox_build(dict(cfg, compositor=compositor_name(cfg)))
