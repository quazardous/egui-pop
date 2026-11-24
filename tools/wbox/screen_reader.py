"""The screen reader pass on Linux, through wbox: what Orca says while
real keys walk five places of the gallery, compared with a checked-in
expectations file.

Run by the bench box's `egui-pop-screen-reader` job (a pre-release check,
docs/releasing.md), with the Python that has the wbox library, after a
release build of the gallery:

    cargo build --release -p egui-pop-gallery
    python tools/wbox/screen_reader.py [--out DIR] [--only WALK]
        [--record FILE] [--no-orca]

Each walk is one session, all private to the run: a D-Bus session bus,
the accessibility bus on it (at-spi2, enabled), the gallery in a headless
labwc as a Wayland client, Orca started on that compositor with its
debug log (every `SPEECH OUTPUT` line is what it would say; its speech
goes to speech-dispatcher's dummy module), and tools/wbox/atspi_focus.py
noting what the accessibility bus says has the focus (role, name,
states, value). The compositor types the keys; after each one the run
waits for Orca to say its line (or for the focus to move) and notes both.

Walks: the command palette, Settings, the tree, the data table and a
dialog (`WALKS` below). A step `until` presses its key until the focus
reaches that node (its first Tabs), and only the node reached counts.

Checks, one line per step (`tree.3 Down: PASS: ...`):

- the node with the focus has a name, and Orca never says "unlabelled":
  a node announced as a bare role is a bug to fix in egui-pop, never a
  line for the expectations;
- role, name, states and value are those of tools/wbox/screen_reader.json,
  and so is Orca's line (with `--no-orca`, the AT-SPI fallback, only the
  node is compared): a difference prints both.

`--record FILE` writes what this run heard as the expectations instead of
comparing (the first run, or after a reviewed change). The exit code is 1
when a check failed. A JSON line per walk sums it up; the run's logs (the
gallery's, Orca's debug log, the focus lines) and a screenshot at the end
of each walk go to the output directory.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from wbox.config import load_config, resolve_dir
from wbox.server import build_compositor

HERE = Path(__file__).resolve().parent
CONFIG = HERE / "config.yaml"
EXPECTATIONS = HERE / "screen_reader.json"
TITLE = "egui-pop gallery"
SCREEN = "1280x800"
SIZE = "1100x720"
SYSTEM_PYTHON = os.environ.get("SYSTEM_PYTHON", "/usr/bin/python3")
A11Y_CONF = "/usr/share/defaults/at-spi2/accessibility.conf"
ATSPI_LIBEXEC = Path(os.environ.get("ATSPI_LIBEXEC", "/usr/libexec"))

# Orca writes its debug log through a buffer; this runs it with the log
# written line by line, so each step reads what Orca said at once.
ORCA_LINE_BUFFERED = """
import builtins, runpy, shutil, sys
log = sys.argv[1]
_open = builtins.open
def _open_log(file, mode="r", *args, **kwargs):
    if file == log and "w" in mode:
        kwargs["buffering"] = 1
    return _open(file, mode, *args, **kwargs)
builtins.open = _open_log
sys.argv = ["orca", "--debug-file", log]
runpy.run_path(shutil.which("orca"), run_name="__main__")
"""

# Orca is done speaking once its log stays quiet this long after a line.
QUIET = 0.8
# How long a key may take to be announced (a software renderer is slow).
STEP_TIMEOUT = 6.0

WALKS: dict[str, dict] = {
    "palette": {"page": "overview", "steps": [
        {"key": "ctrl+shift+p"},
        {"key": "Down"},
        {"key": "Down"},
        {"key": "Up"},
        {"key": "Escape"},
    ]},
    "settings": {"page": "settings", "steps": [
        {"key": "Tab"},
        {"key": "Tab"},
        {"key": "Tab"},
        {"key": "Tab"},
        {"key": "shift+Tab"},
        {"key": "F6"},
        {"key": "Tab"},
    ]},
    "tree": {"page": "tree", "steps": [
        {"key": "Tab", "until": {"name": "Files"}, "max": 40},
        {"key": "Down"},
        {"key": "Down"},
        {"key": "Right"},
        {"key": "Left"},
        {"key": "Up"},
        {"key": "shift+Tab"},
    ]},
    "table": {"page": "table", "steps": [
        {"key": "Tab", "until": {"name": "Photos"}, "max": 40},
        {"key": "Down"},
        {"key": "Down"},
        {"key": "Up"},
        {"key": "Tab"},
    ]},
    "dialog": {"page": "dialogs", "steps": [
        {"key": "Tab", "until": {"name": "Confirm an export"}, "max": 40},
        {"key": "Return"},
        {"key": "Tab"},
        {"key": "shift+Tab"},
        {"key": "Escape"},
    ]},
}

# Speech that means a node without a name.
UNLABELLED = re.compile(r"\bunlabel+ed\b", re.IGNORECASE)


# ── the private session ──────────────────────────────────────────────


def start_bus(run: Path) -> list[subprocess.Popen]:
    """A D-Bus session bus and an accessibility bus for this run, started
    by hand (bus activation needs more than a container gives): the
    accessibility bus with at-spi2's own configuration, its registry, and
    at-spi-bus-launcher for the `org.a11y.Status` switch on the session
    bus. Their addresses go to os.environ, so the compositor, the gallery,
    Orca and the focus listener share them."""
    procs: list[subprocess.Popen] = []
    for name, args in (
        ("DBUS_SESSION_BUS_ADDRESS", ["--session"]),
        ("AT_SPI_BUS_ADDRESS", [f"--config-file={A11Y_CONF}"]),
    ):
        proc = subprocess.Popen(
            ["dbus-daemon", *args, "--nofork", "--nopidfile", "--print-address=1"],
            stdout=subprocess.PIPE, stderr=open(run / "dbus.log", "a"), text=True)
        procs.append(proc)
        address = proc.stdout.readline().strip()
        if not address:
            raise RuntimeError(f"dbus-daemon {' '.join(args)} printed no address")
        os.environ[name] = address
    for tool in ("at-spi2-registryd", "at-spi-bus-launcher"):
        procs.append(subprocess.Popen(
            [str(ATSPI_LIBEXEC / tool)], stdout=subprocess.DEVNULL,
            stderr=open(run / f"{tool}.log", "w")))
    return procs


def enable_accessibility() -> str | None:
    """The accessibility switch on, as a desktop turns it when a screen
    reader is wanted: AccessKit's adapter publishes the tree once it is."""
    deadline = time.monotonic() + 20
    for prop in ("IsEnabled", "ScreenReaderEnabled"):
        while True:
            done = subprocess.run(
                ["gdbus", "call", "--session", "--dest", "org.a11y.Bus",
                 "--object-path", "/org/a11y/bus", "--method",
                 "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", prop, "<true>"],
                capture_output=True, text=True, timeout=20)
            if done.returncode == 0:
                break
            if time.monotonic() > deadline:
                return f"{prop}: {done.stderr.strip()}"
            time.sleep(0.3)
    return None


def speechd_config(home: Path, run: Path) -> None:
    """speech-dispatcher speaking nowhere: its dummy module only, its log
    (the text it received) in the run's directory."""
    conf = home / "config" / "speech-dispatcher"
    conf.mkdir(parents=True, exist_ok=True)
    (run / "speechd").mkdir(exist_ok=True)
    (conf / "speechd.conf").write_text(
        'AddModule "dummy" "sd_dummy" ""\n'
        'DefaultModule dummy\n'
        'LogLevel 5\n'
        f'LogDir "{run / "speechd"}"\n')


def start_orca(comp, home: Path, log: Path) -> subprocess.Popen | None:
    if not shutil.which("orca"):
        return None
    env = comp._wl_env()
    env.update({
        "GDK_BACKEND": "wayland",
        "GSETTINGS_BACKEND": "memory",
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_DATA_HOME": str(home / "data"),
        "XDG_CACHE_HOME": str(home / "cache"),
        "LC_ALL": "C.UTF-8",
        "LANGUAGE": "en",
    })
    # Never --replace: it kills every Orca of the user, the desktop's too.
    return subprocess.Popen(
        [SYSTEM_PYTHON, "-I", "-c", ORCA_LINE_BUFFERED, str(log)],
        env=env, stdout=subprocess.DEVNULL,
        stderr=open(log.with_suffix(".stderr"), "w"))


def stop(proc: subprocess.Popen | None) -> None:
    if proc is None or proc.poll() is not None:
        return
    try:
        proc.terminate()
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()


# ── reading what was said ────────────────────────────────────────────


class Tail:
    """New lines of a file that grows, from where the last read stopped."""

    def __init__(self, path: Path):
        self.path = path
        self.pos = 0

    def skip(self) -> None:
        self.pos = self.path.stat().st_size if self.path.exists() else 0

    def read(self) -> list[str]:
        if not self.path.exists():
            return []
        with open(self.path, "rb") as f:
            f.seek(self.pos)
            data = f.read()
        # a line still being written waits for the next read
        end = data.rfind(b"\n") + 1
        self.pos += end
        return data[:end].decode(errors="replace").splitlines()


def speech_of(lines: list[str]) -> list[str]:
    """The text of Orca's `SPEECH OUTPUT: '<text>' <voice>` debug lines."""
    out = []
    for line in lines:
        _, found, rest = line.partition("SPEECH OUTPUT: '")
        if not found:
            continue
        for end in ("' {", "' None", "' ACSS"):
            i = rest.rfind(end)
            if i >= 0:
                rest = rest[:i]
                break
        else:
            rest = rest.rstrip()
            rest = rest[:-1] if rest.endswith("'") else rest
        if rest.strip():
            out.append(rest.strip())
    return out


def focus_of(lines: list[str]) -> list[dict]:
    out = []
    for line in lines:
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if value.get("event") == "focus":
            out.append({k: v for k, v in value.items() if k != "event"})
    return out


def listen(orca: Tail | None, focus: Tail, wait_speech: bool) -> tuple[list[str], dict | None]:
    """What follows a key: Orca's lines until it is quiet, and the last node
    that took the focus."""
    said: list[str] = []
    nodes: list[dict] = []
    deadline = time.monotonic() + STEP_TIMEOUT
    last = None
    while time.monotonic() < deadline:
        if orca is not None:
            new = speech_of(orca.read())
            if new:
                said += new
                last = time.monotonic()
        moved = focus_of(focus.read())
        if moved:
            nodes += moved
            last = last or time.monotonic()
        heard = said if wait_speech else nodes
        if heard and last is not None and time.monotonic() - last >= QUIET:
            break
        time.sleep(0.05)
    return said, (nodes[-1] if nodes else None)


def matches(node: dict | None, until: dict) -> bool:
    if node is None:
        return False
    if "roles" in until and node.get("role") not in until["roles"]:
        return False
    return "name" not in until or node.get("name") == until["name"]


# ── one walk ─────────────────────────────────────────────────────────


def gallery_bin() -> Path:
    target = os.environ.get("CARGO_TARGET_DIR") or str(HERE.parent.parent / "target")
    return Path(target) / "release" / "egui-pop-gallery"


def wait_for(predicate, timeout: float, every: float = 0.2) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(every)
    return False


def walk(cfg: dict, name: str, spec: dict, out: Path, use_orca: bool) -> tuple[str, list[dict]]:
    """One walk: its mode ("orca" or "atspi") and one record per step;
    a step that could not run is a record with an `error`."""
    run = out / name
    shutil.rmtree(run, ignore_errors=True)
    run.mkdir(parents=True)
    home = Path(tempfile.mkdtemp(prefix=f"sr-{name}-"))
    speechd_config(home, run)
    buses = start_bus(run)
    comp = None
    orca = listener = None
    records: list[dict] = []
    mode = "atspi"
    try:
        problem = enable_accessibility()
        if problem:
            return mode, [{"step": f"{name}.0", "error": f"accessibility bus: {problem}"}]
        comp = build_compositor(cfg)
        comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
        if hasattr(comp, "set_log_dir"):
            comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
        args = ["--no-persistence", "--reduced-motion", "--size", SIZE, "--page", spec["page"]]
        cmd = ["bash", "-c",
               'exec "$0" "$@" > "$GALLERY_OUT/gallery.jsonl" 2> "$GALLERY_OUT/gallery.log"',
               str(gallery_bin()), *args]
        started = comp.launch(cmd, {"GALLERY_OUT": str(run)})
        if "error" in started:
            return mode, [{"step": f"{name}.0", "error": f"launch: {started['error']}"}]

        def has_window() -> bool:
            return any(w.get("title") == TITLE for w in comp.list_windows().get("windows", []))

        if not wait_for(has_window, 60):
            return mode, [{"step": f"{name}.0", "error": f"no window titled {TITLE!r}"}]

        focus_log = run / "focus.jsonl"
        listener = subprocess.Popen(
            [SYSTEM_PYTHON, "-I", str(HERE / "atspi_focus.py"), str(focus_log)],
            stdout=subprocess.DEVNULL, stderr=open(run / "focus.stderr", "w"))
        if not wait_for(lambda: focus_log.exists() and "ready" in focus_log.read_text(), 30):
            return mode, [{"step": f"{name}.0", "error": "the focus listener never started"}]
        focus = Tail(focus_log)

        orca_tail = None
        if use_orca:
            orca_log = run / "orca-debug.out"
            orca = start_orca(comp, home, orca_log)
            if orca is not None:
                orca_tail = Tail(orca_log)
                # "Screen reader on." is its first line once it listens.
                if wait_for(lambda: bool(speech_of(orca_tail.read())), 60, 0.5):
                    mode = "orca"
                else:
                    print(f"{name}: Orca said nothing in 60 s; the AT-SPI tree alone",
                          flush=True)
                    stop(orca)
                    orca = orca_tail = None

        # The gallery's window takes the keys; what that says is not a step.
        comp.focus_window(title=TITLE)
        time.sleep(2.0)
        listen(orca_tail, focus, wait_speech=False)
        if orca_tail:
            orca_tail.read()

        # The node with the focus: a key that moves nothing leaves it there.
        current = None
        for i, step in enumerate(spec["steps"], 1):
            label = f"{name}.{i} {step['key']}"
            record: dict = {"step": label}
            presses = 0
            while True:
                pressed = comp.key(step["key"])
                presses += 1
                if "error" in pressed:
                    record["error"] = f"key: {pressed['error']}"
                    break
                said, node = listen(orca_tail, focus, wait_speech=mode == "orca")
                current = node or current
                node = current
                if "until" not in step or matches(node, step["until"]):
                    record["node"] = node
                    if mode == "orca":
                        record["speech"] = " | ".join(said)
                    break
                if presses >= step.get("max", 1):
                    record["error"] = (f"{presses} presses never reached "
                                       f"{step['until']} (last: {node})")
                    break
            records.append(record)
        shot = comp.screenshot(name=f"screen-reader-{name}.png")
        if "path" in shot:
            shutil.move(shot["path"], out / Path(shot["path"]).name)
        return mode, records
    finally:
        stop(orca)
        stop(listener)
        if comp is not None:
            comp.stop()
        for proc in reversed(buses):
            stop(proc)
        os.environ.pop("DBUS_SESSION_BUS_ADDRESS", None)
        os.environ.pop("AT_SPI_BUS_ADDRESS", None)
        shutil.rmtree(home, ignore_errors=True)


# ── the checks ───────────────────────────────────────────────────────


def expected_of(record: dict) -> dict:
    """A step's record as an expectation: the node, and Orca's line."""
    out = dict(record.get("node") or {})
    if "speech" in record:
        out["speech"] = record["speech"]
    return out


def check(record: dict, expected: dict | None, mode: str) -> tuple[bool, str]:
    if "error" in record:
        return False, record["error"]
    node = record.get("node")
    speech = record.get("speech", "")
    if node is None:
        return False, "nothing has the focus"
    if mode == "orca" and not speech:
        return False, "Orca said nothing"
    if not node.get("name"):
        return False, f"a {node.get('role')} without a name (fix it in egui-pop)"
    if UNLABELLED.search(speech):
        return False, f"Orca said unlabelled: {speech!r} (fix it in egui-pop)"
    got = expected_of(record)
    summary = f"{node.get('role')} {node.get('name')!r} {' '.join(node.get('states', []))}".rstrip()
    if speech:
        summary += f" | Orca: {speech!r}"
    if expected is None:
        return True, f"{summary} (recorded)"
    want = dict(expected)
    if mode != "orca":
        want.pop("speech", None)
        got.pop("speech", None)
    if got != want:
        diff = "\n".join(difflib.unified_diff(
            json.dumps(want, indent=2, sort_keys=True).splitlines(),
            json.dumps(got, indent=2, sort_keys=True).splitlines(),
            "expected", "heard", lineterm=""))
        return False, f"{summary}\n{diff}"
    return True, summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=HERE / "screenshots",
                        help="logs and screenshots (default tools/wbox/screenshots)")
    parser.add_argument("--only", choices=sorted(WALKS), help="one walk")
    parser.add_argument("--record", type=Path,
                        help="write what was heard as the expectations to this file")
    parser.add_argument("--no-orca", action="store_true",
                        help="read the AT-SPI tree alone, without Orca")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    expectations = json.loads(EXPECTATIONS.read_text()) if EXPECTATIONS.exists() else {}
    recorded: dict[str, dict] = {}
    cfg = load_config(CONFIG)
    cfg["screen"] = SCREEN
    os.chdir(CONFIG.parent)
    failed = 0
    modes = set()
    for name, spec in WALKS.items():
        if args.only and name != args.only:
            continue
        print(f"== {name}", flush=True)
        try:
            mode, records = walk(cfg, name, spec, out, not args.no_orca)
        except Exception as exc:  # report and go on with the next walk
            mode, records = "atspi", [{"step": f"{name}.0", "error": repr(exc)}]
        modes.add(mode)
        ok_walk = True
        for record in records:
            label = record["step"]
            if args.record:
                ok, why = check(record, None, mode)
                if "error" not in record:
                    recorded[label] = expected_of(record)
            else:
                ok, why = check(record, expectations.get(label), mode)
                if ok and label not in expectations:
                    ok, why = False, f"no expectation for this step (--record): {why}"
            ok_walk &= ok
            print(f"{label}: {'PASS' if ok else 'FAIL'}: {why}", flush=True)
        failed += not ok_walk
        (out / name / "steps.json").write_text(json.dumps(records, indent=2))
        print(json.dumps({"walk": name, "mode": mode, "ok": ok_walk,
                          "steps": len(records)}), flush=True)
        log = out / name / "gallery.log"
        if not ok_walk and log.exists():
            print(f"-- {name} gallery.log (tail)")
            print("\n".join(log.read_text(errors="replace").splitlines()[-20:]))
    if args.record:
        args.record.write_text(json.dumps(recorded, indent=2, ensure_ascii=False) + "\n")
        print(f"recorded {len(recorded)} steps to {args.record}")
    print(json.dumps({"modes": sorted(modes), "ok": not failed}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
