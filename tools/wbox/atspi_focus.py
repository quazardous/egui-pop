"""What the accessibility bus says has the focus, one JSON line per change.

Run by tools/wbox/screen_reader.py with the system's Python (pyatspi comes
from the distribution, python3-pyatspi on Fedora), on the run's private
D-Bus session:

    python3 tools/wbox/atspi_focus.py OUT.jsonl

Each line is what a screen reader reads of the node that took the focus:
its role (as AT-SPI names it: "push button", "check box"), its name, its
states, and its value for a node with one (a slider). A window that
becomes active gets an `activate` line. The first line is `{"ready": true}`.
"""

from __future__ import annotations

import json
import sys

import pyatspi
from gi.repository import GLib

# The states a screen reader speaks; the others (showing, visible, ...)
# say nothing to a listener.
SPOKEN = {
    "checked", "expandable", "expanded", "collapsed", "selected", "pressed",
    "editable", "multiselectable", "read-only", "required", "invalid-entry",
    "indeterminate", "has-popup", "modal",
}


def state_names(acc) -> list[str]:
    names = set()
    try:
        states = acc.getState().getStates()
    except Exception:  # a node gone meanwhile
        return []
    for s in states:
        nick = getattr(s, "value_nick", None) or str(s).lower()
        names.add(nick.replace("_", "-"))
    out = sorted(names & SPOKEN)
    # A node that cannot be used says so; sensitive is the default.
    if "focusable" in names and "sensitive" not in names:
        out.append("disabled")
    return out


def describe(acc) -> dict:
    try:
        node = {"role": acc.getRoleName(), "name": acc.name or "", "states": state_names(acc)}
    except Exception as exc:  # a node gone meanwhile
        return {"error": repr(exc)}
    try:
        value = acc.queryValue().currentValue
        node["value"] = round(value, 4)
    except (NotImplementedError, AttributeError, Exception):
        pass
    return node


def main() -> int:
    out = open(sys.argv[1], "a", buffering=1)

    def write(line: dict) -> None:
        out.write(json.dumps(line) + "\n")

    def on_focus(event) -> None:
        if event.detail1:
            write({"event": "focus", **describe(event.source)})

    def on_activate(event) -> None:
        write({"event": "activate", **describe(event.source)})

    pyatspi.Registry.registerEventListener(on_focus, "object:state-changed:focused")
    pyatspi.Registry.registerEventListener(on_activate, "window:activate")
    GLib.idle_add(lambda: write({"ready": True}) or False)
    pyatspi.Registry.start()
    return 0


if __name__ == "__main__":
    sys.exit(main())
