"""The compositor a wbox scenario runs in: wbox's own, or mutter.

`WBOX_COMPOSITOR` (labwc, mutter, ...) overrides the config's
`compositor`. `mutter` is this directory's `mutter.py`; anything else goes
to the wbox library's `build_compositor`.
"""

from __future__ import annotations

import os


def build_compositor(cfg: dict):
    name = os.environ.get("WBOX_COMPOSITOR") or cfg.get("compositor", "labwc")
    if name == "mutter":
        from mutter import MutterCompositor

        return MutterCompositor(cfg)
    from wbox.server import build_compositor as wbox_build

    return wbox_build(dict(cfg, compositor=name))
