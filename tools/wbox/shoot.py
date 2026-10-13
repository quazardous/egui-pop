"""Capture the gallery through wbox, one compositor run per shot.

Run by `cargo xtask screenshots`, with the Python that has the wbox
library (the `wbox-mcp` install). It reads a plan (JSON) from the file
given as its argument:

    {"config": "tools/wbox/config.yaml",
     "settle": 1.5,
     "shots": [{"name": "palette-pop-dark",
                "env": {"GALLERY_THEME": "pop-dark", ...},
                "steps": [{"click": [x, y]}, {"key": "ctrl+shift+p"},
                          {"move": [x, y]}, {"wait": 0.5}],
                "screen": "1280x800",
                "out": "/abs/path/palette-pop-dark.png"}]}

A shot's optional "screen" replaces the config's output size for its run.
For each shot it starts the compositor with the gallery inside, waits for
the gallery's "ready" line, plays the steps, captures the whole output
with grim (no cursor) and stops the compositor. It prints one JSON line
per shot on stdout: {"name": ..., "ok": true} or {"name": ..., "error": ...}.
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

from wbox.config import load_config, resolve_dir
from wbox.server import _build_app_cmd, build_compositor


def wait_ready(out_dir: Path, timeout: float) -> bool:
    """Waits for the gallery's start-up line in out/gallery.jsonl."""
    jsonl = out_dir / "gallery.jsonl"
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


def shoot(cfg: dict, shot: dict, settle: float) -> dict:
    out_dir = Path(cfg["_config_dir"]).resolve() / "out"
    jsonl = out_dir / "gallery.jsonl"
    if jsonl.exists():
        jsonl.unlink()
    if "screen" in shot:
        cfg = {**cfg, "screen": shot["screen"]}
    comp = build_compositor(cfg)
    comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
    if hasattr(comp, "set_log_dir"):
        comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
    try:
        started = comp.launch(_build_app_cmd(cfg), dict(shot.get("env", {})))
        if "error" in started:
            return {"error": f"launch: {started['error']}"}
        if not wait_ready(out_dir, 30):
            return {"error": "the gallery never printed its ready line"}
        time.sleep(settle)
        for step in shot.get("steps", []):
            if "click" in step:
                result = comp.click(*step["click"])
            elif "move" in step:
                result = comp.mouse_move(*step["move"])
            elif "key" in step:
                result = comp.key(step["key"])
            elif "wait" in step:
                time.sleep(step["wait"])
                continue
            else:
                return {"error": f"unknown step {step}"}
            if isinstance(result, dict) and "error" in result:
                return {"error": f"{step}: {result['error']}"}
            time.sleep(0.25)
        time.sleep(settle)
        taken = comp.screenshot(name=Path(shot["out"]).name)
        if "error" in taken:
            return {"error": f"screenshot: {taken['error']}"}
        Path(shot["out"]).parent.mkdir(parents=True, exist_ok=True)
        os.replace(taken["path"], shot["out"])
        return {"ok": True}
    finally:
        comp.stop()


def main() -> int:
    plan = json.loads(Path(sys.argv[1]).read_text())
    cfg_path = Path(plan["config"]).resolve()
    cfg = load_config(cfg_path)
    # The app command is relative to the config's directory.
    os.chdir(cfg_path.parent)
    failed = 0
    for shot in plan["shots"]:
        try:
            result = shoot(cfg, shot, plan.get("settle", 1.5))
        except Exception as exc:  # report and go on with the next shot
            result = {"error": repr(exc)}
        failed += "error" in result
        print(json.dumps({"name": shot["name"], **result}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
