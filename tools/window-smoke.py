#!/usr/bin/env python3
"""A real window, scripted: the gallery opens, detaches a panel into a
window of its own, focuses it, types into its field, resizes, and closes
by itself.

    python3 tools/window-smoke.py target/debug/egui-pop-gallery[.exe]
        [--frames N] [--text TEXT] [--scale WINDOW=FACTOR ...]

Run by the platforms workflow on Windows and macOS, and by the bench
box's `egui-pop-window-smoke` job on Linux, under Wayland and X11
(`tools/wbox/window_smoke.py`, which imports the checks from here;
docs/testing.md, "Other platforms"). The gallery prints JSON lines on
stdout (`--help`); this checks them, one line per check:

- detach: `ready` says the panel was detached into its window;
- focus: the detached window said it had the system's focus (`focus`),
  after the gallery asked for it (`--focus-detached`);
- keys: the text typed into that window (`--type`) is in its field at
  the end (`windows`, `field`);
- scale: each window has its own pixels per point, the system's times
  the zoom; with `--scale main=1.5`, exactly that factor;
- resize: `resize` was asked;
- quit: `quit` after every frame, exit code 0, within the time limit.

Exits non-zero when a check failed, with the gallery's output.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys

FRAMES = 360
TIMEOUT = 300  # seconds: a software renderer is slow, a hung window is not.
PANEL = "gallery.notes"
TEXT = "smoke 42"
SIZE = "1100x720"
RESIZE_TO = "800x600"


def gallery_args(frames: int, text: str | None, size: str = SIZE,
                 resize_to: str = RESIZE_TO) -> list[str]:
    """The gallery's options for the scripted run; `text` None leaves the
    typing to a driver (the box types real keys)."""
    args = [
        "--quit-after", str(frames),
        "--size", size,
        "--resize-to", resize_to,
        "--detach", PANEL,
        "--page", "workbench",
        "--focus-detached",
    ]
    if text is not None:
        args += ["--type", text]
    return args


def parse_lines(stdout: str) -> list[dict]:
    """The gallery's JSON lines, in order."""
    out = []
    for line in stdout.splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if isinstance(value, dict) and "event" in value:
            out.append(value)
    return out


def parse_scale(spec: str) -> tuple[str, float]:
    window, _, factor = spec.partition("=")
    if not window or not factor:
        raise argparse.ArgumentTypeError(f"`{spec}` is not WINDOW=FACTOR")
    try:
        return window, float(factor)
    except ValueError:
        raise argparse.ArgumentTypeError(f"`{factor}` is not a number") from None


def check(lines: list[dict], *, frames: int, text: str,
          scales: dict[str, float] | None = None,
          returncode: int | None = 0) -> list[tuple[str, bool, str]]:
    """Each check's name, result and what it saw. `returncode` None: the
    exit code is not known (a compositor launched the gallery)."""
    events: dict[str, dict] = {}
    for line in lines:
        # The first line of each event counts: a frame drawn while the
        # window closes must not stand for the run.
        events.setdefault(line["event"], line)
    results = []

    def add(name: str, ok: bool, detail: str) -> None:
        results.append((name, bool(ok), detail))

    ready = events.get("ready")
    detached = (ready or {}).get("detached") or {}
    add("detach", detached.get("panel") == PANEL,
        f"ready: {detached}" if ready else "no `ready` line")

    focus = events.get("focus")
    if focus:
        add("focus", focus.get("focused") is True and focus.get("window") == PANEL,
            f"{focus.get('window')} focused at frame {focus.get('frames')} "
            f"(asked at {(events.get('focus_asked') or {}).get('frames')}), "
            f"main window focused: {focus.get('main_focused')}")
    else:
        add("focus", False,
            "the detached window never said it had the focus"
            + ("" if "focus_asked" in events else " (never asked)"))

    windows = events.get("windows")
    field = (windows or {}).get("field")
    add("keys", field == text,
        f"field {field!r}, typed {text!r}" if windows else "no `windows` line")

    if not windows:
        add("scale", False, "no `windows` line")
    else:
        zoom = windows.get("zoom_factor") or 1.0
        seen = {w.get("window"): w for w in windows.get("windows", [])}
        problems = []
        for name in ("main", PANEL):
            w = seen.get(name)
            if not w:
                problems.append(f"no {name} window")
                continue
            ppp, native = w.get("pixels_per_point"), w.get("native_pixels_per_point")
            if native is None:
                problems.append(f"{name}: the system gave no scale")
            elif not ppp or abs(ppp - native * zoom) > 0.01:
                problems.append(f"{name}: {ppp} pixels per point, not {native} x {zoom}")
        for name, factor in (scales or {}).items():
            ppp = (seen.get(name) or {}).get("pixels_per_point")
            if ppp is None or abs(ppp - factor) > 0.01:
                problems.append(f"{name}: {ppp} pixels per point, {factor} expected")
        detail = ", ".join(
            f"{n} {w.get('pixels_per_point')} ({w.get('native_pixels_per_point')} x {zoom})"
            for n, w in seen.items())
        add("scale", not problems, "; ".join(problems) or detail)

    add("resize", "resize" in events,
        f"asked {events['resize'].get('size')}" if "resize" in events else "no `resize` line")

    quit_ = events.get("quit")
    problems = []
    if returncode not in (0, None):
        problems.append(f"exit code {returncode}")
    if not quit_:
        problems.append("no `quit` line")
    elif quit_.get("frames") != frames:
        problems.append(f"quit after {quit_.get('frames')} frames, not {frames}")
    # The size the window had when it closed: a window manager may refuse
    # the resize, so it is reported, not checked.
    add("quit", not problems,
        "; ".join(problems) or f"{frames} frames, closed at {quit_.get('size')}")
    return results


def report(results: list[tuple[str, bool, str]], prefix: str = "") -> bool:
    """Prints one line per check; whether all passed."""
    for name, ok, detail in results:
        print(f"{prefix}check {name}: {'PASS' if ok else 'FAIL'}: {detail}", flush=True)
    return all(ok for _, ok, _ in results)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("gallery", help="the gallery's binary")
    parser.add_argument("--frames", type=int, default=FRAMES)
    parser.add_argument("--text", default=TEXT, help="typed into the detached window")
    parser.add_argument("--scale", type=parse_scale, action="append", default=[],
                        metavar="WINDOW=FACTOR",
                        help="a window's expected pixels per point (main, gallery.notes)")
    args = parser.parse_args()
    cmd = [args.gallery, *gallery_args(args.frames, args.text)]
    print("$", " ".join(cmd), flush=True)
    try:
        run = subprocess.run(cmd, capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired as e:
        print(f"timed out after {TIMEOUT} s", file=sys.stderr)
        for stream in (e.stdout, e.stderr):
            if stream:
                print(stream if isinstance(stream, str) else stream.decode(errors="replace"))
        return 1
    print(run.stdout)
    print(run.stderr, file=sys.stderr)
    results = check(parse_lines(run.stdout), frames=args.frames, text=args.text,
                    scales=dict(args.scale), returncode=run.returncode)
    if not report(results):
        failed = [name for name, ok, _ in results if not ok]
        print("window smoke failed: " + ", ".join(failed), file=sys.stderr)
        return 1
    print("window smoke passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
