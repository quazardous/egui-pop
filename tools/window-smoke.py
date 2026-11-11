#!/usr/bin/env python3
"""A real window, scripted: the gallery opens, detaches a panel into a
window of its own, resizes, and closes by itself.

    python3 tools/window-smoke.py target/debug/egui-pop-gallery[.exe]

Run by the platforms workflow on Windows and macOS (docs/testing.md,
"Other platforms"). The gallery prints JSON lines on stdout (`--help`):
this checks it said `ready` with the panel detached, `resize`, then
`quit`, and exited with 0, within the time limit. Exits non-zero
otherwise, with the gallery's output.
"""

import json
import subprocess
import sys

FRAMES = 240
TIMEOUT = 240  # seconds: a software renderer is slow, a hung window is not.


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    cmd = [
        sys.argv[1],
        "--quit-after", str(FRAMES),
        "--size", "1100x720",
        "--resize-to", "800x600",
        "--detach", "gallery.notes",
        "--page", "overview",
    ]
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
    events = {}
    for line in run.stdout.splitlines():
        try:
            line = json.loads(line)
        except ValueError:
            continue
        # The first line of each event counts: a frame drawn while the
        # window closes must not stand for the run.
        if isinstance(line, dict) and "event" in line:
            events.setdefault(line["event"], line)
    errors = []
    if run.returncode != 0:
        errors.append(f"exit code {run.returncode}")
    ready = events.get("ready")
    if not ready:
        errors.append("no `ready` line")
    elif not (ready.get("detached") or {}).get("panel"):
        errors.append(f"the panel was not detached: {ready.get('detached')}")
    if "resize" not in events:
        errors.append("no `resize` line")
    quit_ = events.get("quit")
    if not quit_:
        errors.append("no `quit` line")
    elif quit_.get("frames") != FRAMES:
        errors.append(f"quit after {quit_.get('frames')} frames, not {FRAMES}")
    if errors:
        print("window smoke failed: " + "; ".join(errors), file=sys.stderr)
        return 1
    # The size the window had when it closed: a compositor may refuse the
    # resize, so it is reported, not checked.
    print(f"window smoke passed: {FRAMES} frames, closed at {quit_.get('size')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
