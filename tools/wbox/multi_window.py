"""Several OS windows in one process, through wbox: the multi-window spike.

Runs the gallery's `multi_window_spike` example (a hidden coordinator root
and two deferred windows, "Spike A" animating and "Spike B" idle) in a
nested compositor and records, per backend, what the platform does:

- whether the root really hides (it should not be listed);
- frames per window over a few seconds: A advances, B and the root stay
  still (independent repaint);
- focus: A focused by the compositor, then `F` pressed in A asks the OS to
  raise B (`ViewportCommand::Focus`); which window ends up activated;
- close B through the compositor: B goes, A and the process stay;
- close A: the process quits by itself;
- geometry: a second start with B's size and position edited in the state
  file the first run saved; what B reports.

`labwc` runs two backends: Wayland, then X11 through Xwayland
(`WAYLAND_DISPLAY` unset). `WBOX_COMPOSITOR=mutter` runs Wayland on a
headless mutter.

    cargo build --release -p egui-pop-gallery --example multi_window_spike
    python tools/wbox/multi_window.py [--out DIR] [--backends wayland,x11]

The script builds the example itself when `--build` is given (a bench box
job). One JSON line per backend on stdout and `multi_window.json` in the
output directory; these are findings, so the exit code is 1 only when the
harness itself failed (no window ever showed).
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import time
from pathlib import Path

from wbox.config import load_config, resolve_dir

HERE = Path(__file__).resolve().parent
CONFIG = HERE / "config.yaml"
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from backend import build_compositor, compositor_name  # noqa: E402

TITLES = {"root": "Spike root", "a": "Spike A", "b": "Spike B"}


def binary() -> Path:
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    return target / "release" / "examples" / "multi_window_spike"


def lines(jsonl: Path) -> list[dict]:
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


def events(jsonl: Path, kind: str) -> list[dict]:
    return [e for e in lines(jsonl) if e.get("event") == kind]


def wait_for(pred, timeout: float, step: float = 0.2):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = pred()
        if value:
            return value
        time.sleep(step)
    return None


def titles(comp) -> list[dict]:
    listed = comp.list_windows()
    return [w for w in listed.get("windows", []) if str(w.get("title", "")).startswith("Spike")]


def close_window(comp, comp_name: str, title: str) -> dict:
    """Closes a window as the user would, through the compositor."""
    if comp_name == "labwc" and shutil.which("wlrctl"):
        result = subprocess.run(
            ["wlrctl", "toplevel", "close", f"title:{title}"],
            env=comp._wl_env(),
            capture_output=True,
            text=True,
            timeout=5,
        )
        return {"how": "wlrctl close", "rc": result.returncode, "stderr": result.stderr.strip()}
    comp.focus_window(title=title)
    time.sleep(0.3)
    return {"how": "alt+F4", "result": comp.key("alt+F4")}


def launch(cfg: dict, backend: str, out: Path, tag: str):
    jsonl = out / f"spike-{tag}.jsonl"
    log = out / f"spike-{tag}.log"
    for f in (jsonl, log):
        if f.exists():
            f.unlink()
    unset = "unset WAYLAND_DISPLAY; " if backend == "x11" else ""
    command = (
        f"{unset}exec {shlex.quote(str(binary()))} "
        f"> {shlex.quote(str(jsonl))} 2> {shlex.quote(str(log))}"
    )
    comp = build_compositor(cfg)
    comp.state.screenshot_dir = resolve_dir(cfg, "screenshot_dir", "./screenshots")
    if hasattr(comp, "set_log_dir"):
        comp.set_log_dir(resolve_dir(cfg, "log.dir", "./log"))
    env = {"SPIKE_STATE": str(out / "spike-state.json"), "RUST_LOG": "eframe=debug,egui_winit=debug"}
    started = comp.launch(["sh", "-c", command], env)
    return comp, jsonl, started


def shot(comp, out: Path, name: str) -> None:
    taken = comp.screenshot(name=name)
    if "path" in taken:
        shutil.move(taken["path"], out / Path(taken["path"]).name)


def last_counts(jsonl: Path) -> dict | None:
    counts = events(jsonl, "counts")
    return counts[-1] if counts else None


def run_backend(cfg: dict, comp_name: str, backend: str, out: Path) -> dict:
    result: dict = {"compositor": comp_name, "backend": backend}
    state = out / "spike-state.json"
    if state.exists():
        state.unlink()
    comp, jsonl, started = launch(cfg, backend, out, f"{comp_name}-{backend}-1")
    try:
        if "error" in started:
            return {**result, "error": f"launch: {started['error']}"}
        ready = wait_for(lambda: events(jsonl, "ready"), 20)
        if not ready:
            return {**result, "error": "no ready line"}
        result["ready"] = ready[0]
        shown = wait_for(lambda: len([w for w in titles(comp) if w["title"] in (TITLES["a"], TITLES["b"])]) == 2, 20)
        result["windows_at_start"] = titles(comp)
        if not shown:
            return {**result, "error": "the two windows never showed"}
        time.sleep(1.5)
        listed = titles(comp)
        result["root_listed"] = any(w["title"] == TITLES["root"] for w in listed)
        result["root_hide_sent"] = events(jsonl, "root_hide_sent")
        shot(comp, out, f"spike-{comp_name}-{backend}-start.png")

        # Independent repaint: frames per window over three seconds.
        c0 = wait_for(lambda: last_counts(jsonl), 5)
        time.sleep(3.0)
        c1 = last_counts(jsonl)
        if c0 and c1:
            dt = c1["t"] - c0["t"]
            delta = {k: c1["windows"][k] - c0["windows"].get(k, 0) for k in c1["windows"]}
            delta["root"] = c1["root"] - c0["root"]
            cpu = None
            if c0.get("cpu_ms") is not None and c1.get("cpu_ms") is not None and dt > 0:
                cpu = round((c1["cpu_ms"] - c0["cpu_ms"]) / (dt * 1000) * 100, 1)
            result["repaint"] = {"seconds": round(dt, 2), "frames": delta, "cpu_percent": cpu}
        else:
            result["repaint"] = {"error": "no counts lines", "c0": c0, "c1": c1}

        # Focus: the compositor focuses A, then A asks the OS to raise B.
        result["focus_a"] = comp.focus_window(title=TITLES["a"])
        time.sleep(0.8)
        result["after_focus_a"] = titles(comp)
        result["key_f"] = comp.key("f")
        time.sleep(1.0)
        result["after_raise_b"] = titles(comp)
        result["focus_events"] = events(jsonl, "focus")
        result["raise_events"] = events(jsonl, "raise")

        # Close B: A and the process stay.
        before_close = last_counts(jsonl)
        result["close_b"] = close_window(comp, comp_name, TITLES["b"])
        time.sleep(2.5)
        after_close = last_counts(jsonl)
        result["closed_b"] = [e for e in events(jsonl, "closed") if e.get("window") == "b"]
        result["after_close_b"] = titles(comp)
        result["a_still_running"] = bool(
            before_close and after_close and after_close["windows"]["a"] > before_close["windows"]["a"]
        )
        result["opened"] = events(jsonl, "opened")
        shot(comp, out, f"spike-{comp_name}-{backend}-after-close-b.png")

        # Close A, the last: the process quits.
        result["close_a"] = close_window(comp, comp_name, TITLES["a"])
        quit_line = wait_for(lambda: events(jsonl, "quit"), 8)
        result["quit"] = quit_line[0] if quit_line else None
        time.sleep(1.5)
        result["after_close_a"] = titles(comp)
        result["errors"] = events(jsonl, "error")
    finally:
        comp.stop()

    # Geometry: a second start with B's saved geometry edited.
    try:
        saved = json.loads(state.read_text())
    except (OSError, ValueError):
        saved = {}
    result["saved_state"] = dict(saved)
    want = {"size": [500.0, 330.0], "pos": [300.0, 200.0]}
    saved["b"] = want
    state.write_text(json.dumps(saved))
    comp, jsonl, started = launch(cfg, backend, out, f"{comp_name}-{backend}-2")
    try:
        if "error" in started:
            result["restore"] = {"error": f"launch: {started['error']}"}
        else:
            opened = wait_for(
                lambda: [e for e in events(jsonl, "opened") if e.get("window") == "b"], 20
            )
            result["restore"] = {"wanted": want, "seen": opened[0] if opened else None}
            shot(comp, out, f"spike-{comp_name}-{backend}-restored.png")
    finally:
        comp.stop()
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, default=HERE / "out")
    parser.add_argument("--backends", default=None, help="wayland,x11 (labwc default) or wayland")
    parser.add_argument("--build", action="store_true", help="build the example first")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    # A bench box job (CARGO_TARGET_DIR set) builds every time: its target is a
    # cache another branch may have left.
    if args.build or os.environ.get("CARGO_TARGET_DIR") or not binary().exists():
        subprocess.run(
            ["cargo", "build", "--release", "--locked", "-p", "egui-pop-gallery",
             "--example", "multi_window_spike"],
            cwd=ROOT,
            check=True,
        )
    cfg = load_config(CONFIG)
    os.chdir(CONFIG.parent)
    comp_name = compositor_name(cfg)
    backends = (args.backends or ("wayland,x11" if comp_name == "labwc" else "wayland")).split(",")
    results = []
    harness_failed = False
    for backend in backends:
        try:
            result = run_backend(cfg, comp_name, backend, out)
        except Exception as exc:  # report and go on with the next backend
            result = {"compositor": comp_name, "backend": backend, "error": repr(exc)}
        harness_failed |= "error" in result
        results.append(result)
        print(json.dumps(result), flush=True)
    (out / "multi_window.json").write_text(json.dumps(results, indent=2))
    return 1 if harness_failed else 0


if __name__ == "__main__":
    sys.exit(main())
