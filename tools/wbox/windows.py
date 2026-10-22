"""Overlays across real windows, through wbox: a detached panel's window
in a nested compositor, where a dialog blocks the other windows and the
command palette opens.

kittest draws a detached panel inside the main window, and the tests of
`crates/egui-pop-api/tests/windows.rs` run each window's pass through an
immediate viewport renderer of their own: this scenario checks the same
with real windows, a real window manager and real focus.

Run it with the Python that has the wbox library (the `wbox-mcp`
install), after a release build of the gallery:

    cargo build --release -p egui-pop-gallery
    python tools/wbox/windows.py [--frames N] [--out DIR]

Two runs, each in its own compositor, the gallery on the workbench page
with the notes detached (`--detach gallery.notes`):

1. **palette**: the notes' window is focused, Ctrl+Shift+P opens the
   palette; it must show in that window, which blocks the main one.
2. **dialog**: the notes' window is focused, Ctrl+Shift+Delete asks
   before clearing the notes; the dialog must show in that window. The
   main window is focused next and Ctrl+Shift+P pressed there: blocked,
   the palette must not open.

Each run ends when the gallery quits by itself (`--quit-after`, frames)
and prints the debug bridge's snapshot, whose `windows` collector says
where each overlay is. A screenshot of each run goes to the output
directory. One JSON line per run on stdout, {"name", "ok"} or
{"name", "error"}; the exit code is 1 when one failed.
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
MAIN_TITLE = "egui-pop gallery"
NOTES = "gallery.notes"
NOTES_TITLE = "Notes — egui-pop gallery"


def lines(jsonl: Path) -> list[dict]:
    """The gallery's JSON lines so far."""
    out = []
    try:
        text = jsonl.read_text()
    except OSError:
        return out
    for line in text.splitlines():
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def wait_event(jsonl: Path, event: str, timeout: float) -> dict | None:
    """Waits for a line of the gallery with this `event`."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in lines(jsonl):
            if line.get("event") == event:
                return line
        time.sleep(0.1)
    return None


def wait_window(comp, title: str, timeout: float) -> bool:
    """Waits for a window with this title."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        listed = comp.list_windows()
        if any(w.get("title") == title for w in listed.get("windows", [])):
            return True
        time.sleep(0.2)
    return False


def window(snapshot: dict, name: str) -> dict:
    """A window's entry in the `windows` collector."""
    for w in snapshot.get("windows", {}).get("windows", []):
        if w.get("window") == name:
            return w
    return {}


def check_palette(snapshot: dict) -> str | None:
    w = snapshot.get("windows", {})
    if w.get("palette") != NOTES:
        return f"the palette is in {w.get('palette')!r}, not the notes' window"
    if "Command palette" not in window(snapshot, NOTES).get("overlays", []):
        return f"the notes' window has no palette: {w}"
    if "Command palette" in window(snapshot, "main").get("overlays", []):
        return f"the palette is also in the main window: {w}"
    if not window(snapshot, NOTES).get("modal"):
        return f"the palette does not block the other windows: {w}"
    return None


def check_dialog(snapshot: dict) -> str | None:
    w = snapshot.get("windows", {})
    notes = window(snapshot, NOTES)
    if not notes.get("modal") or not notes.get("overlays"):
        return f"no dialog over the notes' window: {w}"
    if window(snapshot, "main").get("modal"):
        return f"the dialog is in the main window: {w}"
    if w.get("palette") is not None:
        return f"the palette opened in a blocked window: {w}"
    return None


RUNS = [
    {
        "name": "palette",
        "steps": [("focus", NOTES_TITLE), ("key", "ctrl+shift+p")],
        "check": check_palette,
    },
    {
        "name": "dialog",
        "steps": [
            ("focus", NOTES_TITLE),
            ("key", "ctrl+shift+delete"),
            ("shot", None),
            ("focus", MAIN_TITLE),
            ("key", "ctrl+shift+p"),
        ],
        "check": check_dialog,
    },
]


def run(cfg: dict, spec: dict, frames: int, out: Path) -> dict:
    out_dir = Path(cfg["_config_dir"]).resolve() / "out"
    jsonl = out_dir / "gallery.jsonl"
    if jsonl.exists():
        jsonl.unlink()
    comp = build_compositor(cfg)
    comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
    if hasattr(comp, "set_log_dir"):
        comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
    env = {
        "GALLERY_PAGE": "workbench",
        "GALLERY_DETACH": NOTES,
        "GALLERY_QUIT_AFTER": str(frames),
        "GALLERY_DUMP_DEBUG": "1",
    }
    shot = 0

    def screenshot() -> str | None:
        nonlocal shot
        shot += 1
        taken = comp.screenshot(name=f"windows-{spec['name']}-{shot}.png")
        if "error" in taken:
            return f"screenshot: {taken['error']}"
        out.mkdir(parents=True, exist_ok=True)
        # wbox may capture on another filesystem (a bench box job)
        shutil.move(taken["path"], out / Path(taken["path"]).name)
        return None

    try:
        started = comp.launch(_build_app_cmd(cfg), env)
        if "error" in started:
            return {"error": f"launch: {started['error']}"}
        ready = wait_event(jsonl, "ready", 30)
        if ready is None:
            return {"error": "the gallery never printed its ready line"}
        if (ready.get("detached") or {}).get("panel") != NOTES:
            return {"error": f"the notes were not detached: {ready.get('detached')}"}
        if not wait_window(comp, NOTES_TITLE, 15):
            return {"error": f"no window titled {NOTES_TITLE!r}: {comp.list_windows()}"}
        time.sleep(1.0)
        for kind, arg in spec["steps"]:
            if kind == "focus":
                result = comp.focus_window(title=arg)
            elif kind == "key":
                result = comp.key(arg)
            else:
                error = screenshot()
                if error:
                    return {"error": error}
                continue
            if isinstance(result, dict) and "error" in result:
                return {"error": f"{kind} {arg}: {result['error']}"}
            time.sleep(0.5)
        error = screenshot()
        if error:
            return {"error": error}
        debug = wait_event(jsonl, "debug", 120)
        if debug is None:
            return {"error": "the gallery never printed its debug snapshot"}
        failed = spec["check"](debug.get("snapshot", {}))
        return {"error": failed} if failed else {"ok": True}
    finally:
        comp.stop()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--frames",
        type=int,
        default=900,
        help="frames before the gallery quits and dumps its state (default 900)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=HERE / "screenshots",
        help="where the screenshots go (default tools/wbox/screenshots)",
    )
    args = parser.parse_args()
    out = args.out.resolve()
    cfg = load_config(CONFIG)
    # The app command is relative to the config's directory.
    os.chdir(CONFIG.parent)
    failed = 0
    for spec in RUNS:
        try:
            result = run(cfg, spec, args.frames, out)
        except Exception as exc:  # report and go on with the next run
            result = {"error": repr(exc)}
        failed += "error" in result
        print(json.dumps({"name": spec["name"], **result}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
