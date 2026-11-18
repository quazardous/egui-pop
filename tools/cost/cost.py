#!/usr/bin/env python3
"""Measures what egui-pop costs an application, per feature set of the facade.

    python3 tools/cost/cost.py --out OUT [--sets a,b] [--runs N]
                               [--targets DIR] [--budget FILE]

Run by the bench box's `egui-pop-cost` job, never by CI (CI is for
compatibility): the numbers are judged against a budget and reported,
and a run over budget does not fail. For each feature set of the
`egui-pop` crate (`none` is `--no-default-features`, `default` the
default features, any other name that feature alone without the
defaults, `a+b` two of them) it measures:

- `deps`: the packages of `cargo tree -e normal` (normal dependencies on
  this platform, egui-pop's own crates included), and `crates_built`,
  the packages a build compiles (build scripts and proc macros too);
- `clean_build_s`: a release build of the `cost_probe` example from an
  empty target directory, the compile cache off;
- `incremental_build_s`: the same build again after the example's source
  is touched (an application's own code changed);
- `binary_bytes`: the stripped release binary of the probe;
- start-up, headless: `cost_probe --headless` over `MinimalPlugins` and
  over `DefaultPlugins`, `--runs` times each after a warm-up run, the
  median of the time from the process's start to its first frame drawn
  and tessellated (`first_frame_ms`), of the application's build alone
  (`app_build_ms`) and of the whole process, spawn to exit (`process_ms`).

It writes OUT/cost.json (every figure, the budget's verdicts), OUT/cost.md
(the same as a table) and OUT/budget.json: this run's figures as a budget,
to commit as tools/cost/budget.json when the change is meant. Without a
budget, or for a set the budget does not have, the run is the baseline.
It exits 0 whatever the budget says, 1 only when a set does not build.
"""

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
PROBE = os.path.join(ROOT, "crates", "egui-pop", "examples", "cost_probe.rs")

# The default features one by one, then the optional ones that pull
# dependencies of their own. views-video and views-webview need GStreamer
# and WebKitGTK installed; native-dialogs is a fallback kept off.
SETS = [
    "none",
    "default",
    "eframe",
    "phosphor",
    "bundled-fonts",
    "theme-zip",
    "image-previews",
    "viewport",
    "imaging",
    "views",
    "views-markdown",
    "editors",
    "i18n",
    "platform",
    "svg",
    "system-fonts",
    "graph-png",
]

# How much a figure may grow over its budget before it is reported: build
# times are noisy on a shared machine, sizes and dependency counts are not.
TOLERANCE = {
    "deps": 0.05,
    "crates_built": 0.05,
    "clean_build_s": 0.25,
    "incremental_build_s": 0.5,
    "binary_bytes": 0.05,
    "start_minimal_ms": 0.3,
    "start_default_ms": 0.3,
}


def features(name):
    """The cargo arguments selecting a feature set."""
    if name == "default":
        return []
    if name == "none":
        return ["--no-default-features"]
    return ["--no-default-features", "--features", ",".join(name.split("+"))]


def run(cmd, env=None, capture=True):
    p = subprocess.run(
        cmd, cwd=ROOT, env=env, text=True, check=False,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if p.returncode != 0:
        tail = (p.stderr or "")[-4000:]
        raise RuntimeError(f"{' '.join(cmd)} exited with {p.returncode}\n{tail}")
    return p.stdout


def deps(feats):
    """The normal dependency packages, egui-pop's own included."""
    out = run(["cargo", "tree", "--locked", "-p", "egui-pop", "-e", "normal",
               "--prefix", "none", *feats])
    names = set()
    for line in out.splitlines():
        line = line.replace(" (*)", "").strip()
        if not line:
            continue
        # "name vX.Y.Z" then an optional "(source)" or "(proc-macro)"
        parts = line.split()
        if len(parts) >= 2:
            names.add(f"{parts[0]} {parts[1]}")
    return sorted(names)


def build(feats, env):
    """A release build of the probe: seconds, packages compiled."""
    t = time.perf_counter()
    out = run(["cargo", "build", "--locked", "--release", "-p", "egui-pop",
               "--example", "cost_probe", "--message-format", "json", *feats], env=env)
    took = time.perf_counter() - t
    packages = set()
    for line in out.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") == "compiler-artifact" and not msg.get("fresh", False):
            packages.add(msg["package_id"])
    return took, len(packages)


def start(binary, plugins, runs):
    """Medians of `runs` headless starts after one warm-up."""
    samples = []
    for i in range(runs + 1):
        t = time.perf_counter()
        out = run([binary, "--headless", plugins])
        process_ms = (time.perf_counter() - t) * 1000.0
        if i == 0:
            continue
        line = json.loads(out.strip().splitlines()[-1])
        line["process_ms"] = process_ms
        samples.append(line)
    med = lambda k: round(statistics.median(s[k] for s in samples), 2)  # noqa: E731
    return {
        "first_frame_ms": med("to_first_frame_ms"),
        "app_build_ms": med("build_ms"),
        "draw_ms": med("first_frame_ms"),
        "second_frame_ms": med("second_frame_ms"),
        "process_ms": med("process_ms"),
        "runs": runs,
    }


def measure(name, targets, runs):
    feats = features(name)
    target = os.path.join(targets, name.replace("+", "_"))
    shutil.rmtree(target, ignore_errors=True)
    env = dict(os.environ)
    env.pop("RUSTC_WRAPPER", None)  # a clean build compiles, no cache
    env["CARGO_TARGET_DIR"] = target
    env["CARGO_PROFILE_RELEASE_STRIP"] = "symbols"
    try:
        listed = deps(feats)
        clean_s, built = build(feats, env)
        os.utime(PROBE)
        incr_s, _ = build(feats, env)
        binary = os.path.join(target, "release", "examples", "cost_probe")
        size = os.path.getsize(binary)
        minimal = start(binary, "minimal", runs)
        default = start(binary, "default", runs)
    finally:
        shutil.rmtree(target, ignore_errors=True)
    return {
        "features": " ".join(feats) or "(default)",
        "deps": len(listed),
        "crates_built": built,
        "clean_build_s": round(clean_s, 1),
        "incremental_build_s": round(incr_s, 1),
        "binary_bytes": size,
        "start_minimal_ms": minimal["first_frame_ms"],
        "start_default_ms": default["first_frame_ms"],
        "start": {"minimal": minimal, "default": default},
        "dependencies": listed,
    }


def judge(results, budget):
    """Each figure against the budget: the ones over it."""
    over, baseline = [], []
    tol = dict(TOLERANCE, **budget.get("tolerance", {}))
    for name, r in results.items():
        if "error" in r:
            continue
        base = budget.get("sets", {}).get(name)
        if base is None:
            baseline.append(name)
            continue
        for key, t in tol.items():
            if key not in base or key not in r:
                continue
            limit = base[key] * (1.0 + t)
            if key in ("deps", "crates_built"):
                limit = max(limit, base[key] + 2)
            if r[key] > limit:
                over.append({"set": name, "figure": key, "value": r[key],
                             "budget": base[key], "limit": round(limit, 2)})
    return over, baseline


def size(n):
    return f"{n / 1048576:.1f} MiB"


def table(results, over):
    flagged = {(o["set"], o["figure"]) for o in over}
    cols = [("deps", "deps", str), ("crates_built", "crates built", str),
            ("clean_build_s", "clean build s", str),
            ("incremental_build_s", "incremental s", str),
            ("binary_bytes", "binary", size),
            ("start_minimal_ms", "first frame, Minimal ms", str),
            ("start_default_ms", "first frame, Default ms", str)]
    lines = ["| set | " + " | ".join(c[1] for c in cols) + " |",
             "|---|" + "---:|" * len(cols)]
    for name, r in results.items():
        if "error" in r:
            lines.append(f"| {name} | build failed |" + " |" * (len(cols) - 1))
            continue
        cells = []
        for key, _, fmt in cols:
            cell = fmt(r[key])
            if (name, key) in flagged:
                cell += " (over)"
            cells.append(cell)
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--sets", default=",".join(SETS))
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--targets", default=os.path.join(ROOT, "target", "cost"))
    ap.add_argument("--budget", default=os.path.join(ROOT, "tools", "cost", "budget.json"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    results, failed = {}, False
    for name in [s for s in args.sets.split(",") if s]:
        print(f"cost: {name}", flush=True)
        try:
            results[name] = measure(name, args.targets, args.runs)
            r = results[name]
            print(f"cost: {name}: {r['deps']} deps, {r['clean_build_s']} s clean, "
                  f"{r['incremental_build_s']} s incremental, {size(r['binary_bytes'])}, "
                  f"first frame {r['start_minimal_ms']} / {r['start_default_ms']} ms", flush=True)
        except RuntimeError as e:
            failed = True
            results[name] = {"error": str(e)}
            print(f"cost: {name} failed: {e}", flush=True)

    try:
        with open(args.budget, encoding="utf-8") as f:
            budget = json.load(f)
    except FileNotFoundError:
        budget = {}
    over, baseline = judge(results, budget)

    rustc = run(["rustc", "--version"]).strip()
    report = {"rustc": rustc, "sets": results, "over_budget": over, "baseline": baseline}
    with open(os.path.join(args.out, "cost.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1)

    figures = list(TOLERANCE)
    proposal = {
        "about": "egui-pop's cost per feature set, measured by tools/cost/cost.py on the bench box "
                 "(egui-pop-cost); a figure above its value times 1 + its tolerance is reported",
        "rustc": rustc,
        "tolerance": TOLERANCE,
        "sets": {n: {k: r[k] for k in figures} for n, r in results.items() if "error" not in r},
    }
    with open(os.path.join(args.out, "budget.json"), "w", encoding="utf-8") as f:
        json.dump(proposal, f, indent=1)
        f.write("\n")

    md = ["# egui-pop's cost per feature set", "", f"`{rustc}`", "", table(results, over), ""]
    if over:
        md += ["Over budget:", ""]
        md += [f"- {o['set']}: {o['figure']} {o['value']} (budget {o['budget']}, limit {o['limit']})"
               for o in over]
    elif budget:
        md.append("Every figure is within its budget.")
    if baseline:
        md += ["", "No budget yet (this run is the baseline): " + ", ".join(baseline) + "."]
    with open(os.path.join(args.out, "cost.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")
    print("\n".join(md), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
