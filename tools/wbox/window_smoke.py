"""The window smoke run (tools/window-smoke.py) on Linux, through wbox:
the gallery in a headless labwc, once as a Wayland client and once as
an X11 one (Xwayland), its detached window focused and typed into by
the compositor, with real keys.

Run by the bench box's `egui-pop-window-smoke` job (a pre-release check,
docs/releasing.md), with the Python that has the wbox library, after a
release build of the gallery:

    cargo build --release -p egui-pop-gallery
    python tools/wbox/window_smoke.py [--out DIR] [--only wayland|x11]

Each session runs the gallery's scripted run (`--focus-detached`, no
`--type`: the keys come from the compositor), then:

1. waits for the detached notes' window;
2. Wayland: sets the output's scale to 2 while the gallery runs (each
   window must follow it); X11: the gallery starts with
   `WINIT_X11_SCALE_FACTOR=1.5`;
3. focuses the notes' window through the compositor, waits for the
   gallery to say that window has the focus, and types the text there.

The checks are tools/window-smoke.py's, one line each
(`wayland check focus: PASS: ...`), and one JSON line per session; the
exit code is 1 when a check failed. A screenshot of each session goes to
the output directory.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

from wbox.config import load_config, resolve_dir
from wbox.server import build_compositor

HERE = Path(__file__).resolve().parent
CONFIG = HERE / "config.yaml"
NOTES_TITLE = "Notes — egui-pop gallery"
FRAMES = 1800
TEXT = "smoke 42"
SCREEN = "1920x1200"
SIZE = "800x500"
RESIZE_TO = "700x450"


def load_smoke():
    """tools/window-smoke.py, whose name is no module name."""
    spec = importlib.util.spec_from_file_location(
        "window_smoke", HERE.parent / "window-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SESSIONS = {
    "wayland": {"x11": False, "env": {}, "output_scale": 2.0},
    "x11": {"x11": True, "env": {"WINIT_X11_SCALE_FACTOR": "1.5"}, "scale": 1.5},
}


def lines(jsonl: Path, smoke) -> list[dict]:
    try:
        return smoke.parse_lines(jsonl.read_text())
    except OSError:
        return []


def wait_event(jsonl: Path, smoke, event: str, timeout: float) -> dict | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in lines(jsonl, smoke):
            if line.get("event") == event:
                return line
        time.sleep(0.1)
    return None


def wait_window(comp, title: str, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        listed = comp.list_windows()
        if any(w.get("title") == title for w in listed.get("windows", [])):
            return True
        time.sleep(0.2)
    return False


def gallery_bin() -> Path:
    target = os.environ.get("CARGO_TARGET_DIR") or str(HERE.parent.parent / "target")
    return Path(target) / "release" / "egui-pop-gallery"


def session(cfg: dict, name: str, spec: dict, smoke, out: Path) -> list[tuple[str, bool, str]]:
    """One session's checks; a step that fails is a failed check `run`."""
    out_dir = out / name
    out_dir.mkdir(parents=True, exist_ok=True)
    jsonl = out_dir / "gallery.jsonl"
    jsonl.unlink(missing_ok=True)
    comp = build_compositor(cfg)
    comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
    if hasattr(comp, "set_log_dir"):
        comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
    args = ["--no-persistence", *smoke.gallery_args(FRAMES, None, SIZE, RESIZE_TO)]
    # X11: no Wayland display to find, winit takes Xwayland's.
    unset = "unset WAYLAND_DISPLAY; " if spec["x11"] else ""
    cmd = ["bash", "-c",
           unset + 'exec "$0" "$@" > "$GALLERY_OUT/gallery.jsonl" 2> "$GALLERY_OUT/gallery.log"',
           str(gallery_bin()), *args]
    env = {"GALLERY_OUT": str(out_dir), **spec["env"]}
    scales = {}

    def fail(step: str) -> list[tuple[str, bool, str]]:
        return [("run", False, step)]

    try:
        started = comp.launch(cmd, env)
        if "error" in started:
            return fail(f"launch: {started['error']}")
        if wait_event(jsonl, smoke, "ready", 60) is None:
            return fail("the gallery never printed its ready line")
        if not wait_window(comp, NOTES_TITLE, 30):
            return fail(f"no window titled {NOTES_TITLE!r}: {comp.list_windows()}")
        if spec.get("output_scale"):
            factor = spec["output_scale"]
            wl = comp._wl_env()
            output = comp._wlr_output_name(wl)
            set_scale = subprocess.run(
                ["wlr-randr", "--output", output, "--scale", str(factor)],
                env=wl, capture_output=True, text=True, timeout=10)
            if set_scale.returncode != 0:
                return fail(f"wlr-randr --scale {factor}: {set_scale.stderr.strip()}")
            scales = {"main": factor, smoke.PANEL: factor}
        elif spec.get("scale"):
            scales = {"main": spec["scale"], smoke.PANEL: spec["scale"]}
        time.sleep(1.0)
        focused = comp.focus_window(title=NOTES_TITLE)
        if "error" in focused:
            return fail(f"focus: {focused['error']}")
        # The gallery says when the window has the focus (and gives its
        # field the keys then); its own focus request comes a quarter of
        # the way through.
        if wait_event(jsonl, smoke, "focus", 120) is not None:
            time.sleep(1.0)
            typed = comp.type_text(TEXT)
            if "error" in typed:
                return fail(f"type: {typed['error']}")
        time.sleep(1.0)
        shot = comp.screenshot(name=f"window-smoke-{name}.png")
        if "path" in shot:
            shutil.move(shot["path"], out / Path(shot["path"]).name)
        if wait_event(jsonl, smoke, "quit", 240) is None:
            return fail("the gallery never quit by itself")
        proc = getattr(comp.state, "app_proc", None)
        returncode = None
        if proc is not None:
            try:
                returncode = proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                returncode = "still running 30 s after quit"
        return smoke.check(lines(jsonl, smoke), frames=FRAMES, text=TEXT,
                           scales=scales, returncode=returncode)
    finally:
        comp.stop()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=HERE / "screenshots",
                        help="the gallery's output and screenshots (default tools/wbox/screenshots)")
    parser.add_argument("--only", choices=sorted(SESSIONS), help="one session")
    args = parser.parse_args()
    out = args.out.resolve()
    smoke = load_smoke()
    cfg = load_config(CONFIG)
    cfg["screen"] = SCREEN
    os.chdir(CONFIG.parent)
    failed = 0
    for name, spec in SESSIONS.items():
        if args.only and name != args.only:
            continue
        print(f"== {name}", flush=True)
        try:
            results = session(cfg, name, spec, smoke, out)
        except Exception as exc:  # report and go on with the next session
            results = [("run", False, repr(exc))]
        ok = smoke.report(results, prefix=f"{name} ")
        failed += not ok
        print(json.dumps({"session": name, "ok": ok,
                          "checks": {n: passed for n, passed, _ in results}}), flush=True)
        log = out / name / "gallery.log"
        if not ok and log.exists():
            print(f"-- {name} gallery.log (tail)")
            print("\n".join(log.read_text(errors="replace").splitlines()[-40:]))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
