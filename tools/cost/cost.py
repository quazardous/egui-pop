#!/usr/bin/env python3
"""Measures what egui-pop costs an application, per feature set of the facade.

    python3 tools/cost/cost.py --out OUT [--sets a,b] [--runs N]
                               [--targets DIR] [--budget FILE]

Run by the bench box's `egui-pop-cost` job, never by CI (CI is for
compatibility): the figures are judged against a budget and reported,
and a run over budget does not fail. For each feature set of the
`egui-pop` crate (`none` is `--no-default-features`, `default` the
default features, any other name that feature alone without the
defaults, `a+b` two of them) it measures the release build of the
`cost_probe` example, the compile cache off:

- `deps`: the packages of `cargo tree -e normal` (normal dependencies on
  this platform, egui-pop's own crates included);
- `clean_build_s`, for `none` and `default` only: the build from an empty
  target directory (a clean build of each set would take hours);
- `added_build_s`: the build on top of `none`'s finished one, and
  `added_crates`, the packages it compiles: what the set adds to the
  bare build (`none` adds nothing). Each set starts from the same copy of
  `none`'s target directory, so no set reuses another's work;
- `incremental_build_s`: the build again after the example's source is
  touched (an application's own code changed);
- `binary_bytes`: the stripped binary;
- start-up, headless: `cost_probe --headless` over `MinimalPlugins` and
  over `DefaultPlugins`, `--runs` times each after a warm-up run: the
  median time from the process's start to its first frame drawn and
  tessellated (`start_minimal_ms`, `start_default_ms`), and beside it the
  application's build alone, the frame alone and the whole process from
  spawn to exit.

It writes OUT/cost.json (every figure, the budget's verdicts), OUT/cost.md
(the same as a table) and OUT/budget.json: this run's figures as a budget,
to commit as tools/cost/budget.json when the change is meant; all three
again after each set, so a run cut short keeps what it measured. Without a
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

# How much a figure may grow over its budget before it is reported, as a
# fraction and as a floor: build and start times are noisy on a shared
# machine, sizes and dependency counts are not.
TOLERANCE = {
    "deps": [0.05, 2],
    "clean_build_s": [0.25, 20],
    "added_build_s": [0.25, 20],
    "incremental_build_s": [0.5, 3],
    "binary_bytes": [0.05, 65536],
    "start_minimal_ms": [0.3, 10],
    "start_default_ms": [0.3, 10],
}

COLUMNS = [
    ("deps", "deps"),
    ("clean_build_s", "clean build (s)"),
    ("added_build_s", "added build (s)"),
    ("added_crates", "added crates"),
    ("incremental_build_s", "incremental (s)"),
    ("binary_bytes", "binary"),
    ("start_minimal_ms", "first frame, Minimal (ms)"),
    ("start_default_ms", "first frame, Default (ms)"),
]


def features(name):
    """The cargo arguments selecting a feature set."""
    if name == "default":
        return []
    if name == "none":
        return ["--no-default-features"]
    return ["--no-default-features", "--features", ",".join(name.split("+"))]


def run(cmd, env=None):
    p = subprocess.run(cmd, cwd=ROOT, env=env, text=True, check=False,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} exited with {p.returncode}\n{p.stderr[-4000:]}")
    return p.stdout


def deps(feats):
    """The normal dependency packages, egui-pop's own included."""
    out = run(["cargo", "tree", "--locked", "-p", "egui-pop", "-e", "normal",
               "--prefix", "none", *feats])
    names = set()
    for line in out.splitlines():
        parts = line.replace(" (*)", "").split()
        if len(parts) >= 2:  # "name vX.Y.Z", then a source or "(proc-macro)"
            names.add(f"{parts[0]} {parts[1]}")
    return sorted(names)


def build_env(target):
    env = dict(os.environ)
    env.pop("RUSTC_WRAPPER", None)  # the compile cache off: builds compile
    env["CARGO_TARGET_DIR"] = target
    env["CARGO_PROFILE_RELEASE_STRIP"] = "symbols"
    return env


def build(feats, target):
    """A release build of the probe: seconds, packages compiled."""
    t = time.perf_counter()
    out = run(["cargo", "build", "--locked", "--release", "-p", "egui-pop",
               "--example", "cost_probe", "--message-format", "json", *feats],
              env=build_env(target))
    took = time.perf_counter() - t
    built = set()
    for line in out.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") == "compiler-artifact" and not msg.get("fresh", False):
            built.add(msg["package_id"])
    return round(took, 1), len(built)


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

    def med(k):
        return round(statistics.median(s[k] for s in samples), 2)

    return {
        "first_frame_ms": med("to_first_frame_ms"),
        "app_build_ms": med("build_ms"),
        "draw_ms": med("first_frame_ms"),
        "second_frame_ms": med("second_frame_ms"),
        "process_ms": med("process_ms"),
        "runs": runs,
    }


def copy_tree(src, dst):
    shutil.rmtree(dst, ignore_errors=True)
    subprocess.run(["cp", "-a", "--reflink=auto", src, dst], check=True)


def measure(name, targets, runs):
    """One set's figures. `none` is built first and kept as the base."""
    feats = features(name)
    base = os.path.join(targets, "base")
    work = os.path.join(targets, "work")
    r = {"features": " ".join(feats) or "(default)"}
    listed = deps(feats)
    r["deps"] = len(listed)
    if name == "none":
        shutil.rmtree(work, ignore_errors=True)
        r["clean_build_s"], r["clean_crates"] = build(feats, work)
        r["added_build_s"], r["added_crates"] = 0.0, 0
        copy_tree(work, base)
    else:
        if name == "default":
            clean = os.path.join(targets, "clean")
            shutil.rmtree(clean, ignore_errors=True)
            try:
                r["clean_build_s"], r["clean_crates"] = build(feats, clean)
            finally:
                shutil.rmtree(clean, ignore_errors=True)
        copy_tree(base, work)  # the same start for every set, at the same path
        r["added_build_s"], r["added_crates"] = build(feats, work)
    os.utime(PROBE)
    r["incremental_build_s"], _ = build(feats, work)
    binary = os.path.join(work, "release", "examples", "cost_probe")
    r["binary_bytes"] = os.path.getsize(binary)
    minimal = start(binary, "minimal", runs)
    default = start(binary, "default", runs)
    r["start_minimal_ms"] = minimal["first_frame_ms"]
    r["start_default_ms"] = default["first_frame_ms"]
    r["start"] = {"minimal": minimal, "default": default}
    r["dependencies"] = listed
    return r


def judge(results, budget):
    """Each figure against the budget: the ones over it, the sets without one."""
    over, baseline = [], []
    tol = dict(TOLERANCE, **budget.get("tolerance", {}))
    for name, r in results.items():
        if "error" in r:
            continue
        base = budget.get("sets", {}).get(name)
        if base is None:
            baseline.append(name)
            continue
        for key, (frac, floor) in tol.items():
            if key not in base or key not in r:
                continue
            limit = max(base[key] * (1.0 + frac), base[key] + floor)
            if r[key] > limit:
                over.append({"set": name, "figure": key, "value": r[key],
                             "budget": base[key], "limit": round(limit, 2)})
    return over, baseline


def cell(key, value):
    if key == "binary_bytes":
        return f"{value / 1048576:.1f} MiB"
    return str(value)


def table(results, over):
    flagged = {(o["set"], o["figure"]) for o in over}
    lines = ["| set | " + " | ".join(c[1] for c in COLUMNS) + " |",
             "|---|" + "---:|" * len(COLUMNS)]
    for name, r in results.items():
        if "error" in r:
            lines.append(f"| {name} | build failed |" + " |" * (len(COLUMNS) - 1))
            continue
        cells = []
        for key, _ in COLUMNS:
            c = cell(key, r[key]) if key in r else ""
            if (name, key) in flagged:
                c += " (over)"
            cells.append(c)
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def report(out, results, budget, rustc, final):
    over, baseline = judge(results, budget)
    with open(os.path.join(out, "cost.json"), "w", encoding="utf-8") as f:
        json.dump({"rustc": rustc, "complete": final, "sets": results,
                   "over_budget": over, "baseline": baseline}, f, indent=1)
    proposal = {
        "about": "egui-pop's cost per feature set, measured by tools/cost/cost.py on the bench "
                 "box (egui-pop-cost); a figure above max(value * (1 + fraction), value + floor) "
                 "is reported, never a failure",
        "rustc": rustc,
        "tolerance": TOLERANCE,
        "sets": {n: {k: r[k] for k in TOLERANCE if k in r}
                 for n, r in results.items() if "error" not in r},
    }
    with open(os.path.join(out, "budget.json"), "w", encoding="utf-8") as f:
        json.dump(proposal, f, indent=1)
        f.write("\n")
    md = ["# egui-pop's cost per feature set", "", f"`{rustc}`", "", table(results, over), ""]
    if over:
        md += ["Over budget:", ""]
        md += [f"- {o['set']}: {o['figure']} {o['value']} (budget {o['budget']}, "
               f"limit {o['limit']})" for o in over]
    elif budget and final:
        md.append("Every figure is within its budget.")
    if baseline:
        md += ["", "No budget yet (this run is the baseline): " + ", ".join(baseline) + "."]
    if not final:
        md += ["", "(The run is not finished.)"]
    with open(os.path.join(out, "cost.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")
    return md


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--sets", default=",".join(SETS))
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--targets", default=os.path.join(ROOT, "target", "cost"))
    ap.add_argument("--budget", default=os.path.join(ROOT, "tools", "cost", "budget.json"))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    os.makedirs(args.targets, exist_ok=True)
    try:
        with open(args.budget, encoding="utf-8") as f:
            budget = json.load(f)
    except FileNotFoundError:
        budget = {}
    rustc = run(["rustc", "--version"]).strip()

    # `none` first: every other set's added build starts from it
    sets = ["none"] + [s for s in args.sets.split(",") if s and s != "none"]
    results, failed = {}, False
    try:
        for name in sets:
            print(f"cost: {name}", flush=True)
            try:
                r = measure(name, args.targets, args.runs)
            except RuntimeError as e:
                failed = True
                results[name] = {"error": str(e)}
                print(f"cost: {name} failed: {e}", flush=True)
                if name == "none":
                    break
                continue
            results[name] = r
            print(f"cost: {name}: {r['deps']} deps, added {r['added_build_s']} s "
                  f"({r['added_crates']} crates), incremental {r['incremental_build_s']} s, "
                  f"{cell('binary_bytes', r['binary_bytes'])}, first frame "
                  f"{r['start_minimal_ms']} / {r['start_default_ms']} ms", flush=True)
            report(args.out, results, budget, rustc, final=False)
    finally:
        for d in ("base", "work", "clean"):
            shutil.rmtree(os.path.join(args.targets, d), ignore_errors=True)
    print("\n".join(report(args.out, results, budget, rustc, final=True)), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
