"""Hover after a move by the gallery's own title bar, through wbox.

The title bar hands the window to the compositor for a move
(`ViewportCommand::StartDrag`); labwc keeps the release of the press that
started it, so egui would go on holding the primary button and nothing
would hover until the next click. kittest cannot show it (no compositor):
this scenario moves the real window in a nested labwc and checks that the
close button still hovers.

Run it with the Python that has the wbox library (the `wbox-mcp`
install), after a release build of the gallery:

    cargo build --release -p egui-pop-gallery
    python tools/wbox/titlebar.py [--out DIR]

The gallery fills the output (1280x800) with its own frame
(`--own-frame`): labwc puts it at the left edge, 28 px down, so its title
bar's middle row is at y = 43. Two runs, each in
its own compositor:

1. **still**: the window is not moved. The title bar's right end is
   captured with the pointer on the caption, then on the close button:
   the two captures must differ (the button hovers).
2. **moved**: the caption is dragged 100 px to the left first, then the
   same captures, 100 px further left: they must differ too.

Full screenshots of each run go to the output directory. One JSON line per
run on stdout, {"name", "ok"} or {"name", "error"}; the exit code is 1
when one failed.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

from wbox.config import load_config, resolve_dir
from wbox.server import _build_app_cmd, build_compositor

HERE = Path(__file__).resolve().parent
CONFIG = HERE / "config.yaml"
WIDTH, HEIGHT = 1280, 800
# The title bar's middle row, and how far the move goes.
BAR_Y = 43
SHIFT = 100


def wait_ready(jsonl: Path, timeout: float) -> bool:
    """Waits for the gallery's start-up line."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            for line in jsonl.read_text().splitlines():
                if json.loads(line).get("event") == "ready":
                    return True
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    return False


def capture(comp, out: Path, name: str, region: str | None = None) -> bytes:
    """A capture of the output (or a region of it), kept in `out`."""
    taken = comp.screenshot(name=name, region=region)
    if "error" in taken:
        raise RuntimeError(f"screenshot {name}: {taken['error']}")
    out.mkdir(parents=True, exist_ok=True)
    dest = out / name
    shutil.move(taken["path"], dest)
    return dest.read_bytes()


def run(cfg: dict, name: str, moved: bool, out: Path) -> dict:
    out_dir = Path(cfg["_config_dir"]).resolve() / "out"
    jsonl = out_dir / "gallery.jsonl"
    if jsonl.exists():
        jsonl.unlink()
    comp = build_compositor(cfg)
    comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
    if hasattr(comp, "set_log_dir"):
        comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
    env = {
        "GALLERY_OWN_FRAME": "1",
        "GALLERY_SIZE": f"{WIDTH}x{HEIGHT}",
        "GALLERY_REDUCED_MOTION": "1",
        "GALLERY_PAGE": "overview",
    }
    try:
        started = comp.launch(_build_app_cmd(cfg), env)
        if "error" in started:
            return {"error": f"launch: {started['error']}"}
        if not wait_ready(jsonl, 30):
            return {"error": "the gallery never printed its ready line"}
        time.sleep(2.0)
        left = SHIFT if moved else 0
        caption = (WIDTH // 2, BAR_Y)
        if moved:
            # One press, travelled by steps, released: the compositor takes
            # the move after egui's drag threshold.
            dragged = comp.drag(caption[0], BAR_Y, caption[0] - SHIFT, BAR_Y, steps=10, seconds=0.5)
            if isinstance(dragged, dict) and "error" in dragged:
                return {"error": f"drag: {dragged['error']}"}
            time.sleep(1.0)
        # The bar's right end, where the window buttons are.
        right = WIDTH - left
        region = f"{right - 160},{BAR_Y - 14} 160x28"
        comp.mouse_move(caption[0] - left, BAR_Y)
        time.sleep(0.8)
        idle = capture(comp, out, f"{name}-idle.png", region)
        comp.mouse_move(right - 23, BAR_Y)
        time.sleep(0.8)
        hovered = capture(comp, out, f"{name}-hover.png", region)
        capture(comp, out, f"{name}.png")
        if idle == hovered:
            return {"error": "the close button does not hover"}
        return {"ok": True}
    finally:
        comp.stop()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", default=str(HERE / "screenshots"))
    args = parser.parse_args()
    cfg = load_config(CONFIG)
    # The app command is relative to the config's directory.
    os.chdir(CONFIG.parent)
    out = Path(args.out).resolve()
    failed = 0
    for name, moved in [("still", False), ("moved", True)]:
        try:
            result = run(cfg, name, moved, out)
        except Exception as exc:  # report and go on with the next run
            result = {"error": repr(exc)}
        failed += "error" in result
        print(json.dumps({"name": name, **result}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
