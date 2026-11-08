"""A headless mutter (GNOME's compositor) with the wbox scenario API.

wbox's own backends drive wlroots compositors (labwc). Bugs seen only on
GNOME need mutter itself, which takes neither wlroots' virtual pointer nor
grim. This backend gives a scenario the same calls as wbox's compositors
(`launch`, `mouse_move`, `click`, `drag`, `key`, `screenshot`, `stop`)
over mutter's own interfaces:

- mutter runs `--headless --wayland --no-x11 --virtual-monitor WxH` in a
  private D-Bus session and runtime directory, with PipeWire and
  WirePlumber beside it;
- input goes through `org.gnome.Mutter.RemoteDesktop`: one session, held by
  this process's D-Bus connection (mutter closes it when the connection
  goes), with absolute motion against its ScreenCast stream of the monitor;
- frames come from that stream through PipeWire: a GStreamer pipeline keeps
  the last frames as PNG files, and a screenshot is the first complete one
  after the request (a region is cropped by GStreamer too). The cursor is
  not drawn in them.

Needs mutter, pipewire, wireplumber, dbus-daemon, gst-launch-1.0 with
pipewiresrc, pngenc and videocrop, and jeepney (pure Python D-Bus) in the
interpreter. GSettings use a key file of the private configuration
directory: auto-maximize is off, so a window as large as the output stays
where mutter puts it, at the top left.

`backend.build_compositor` picks it for `compositor: mutter` (or
`WBOX_COMPOSITOR=mutter`).
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

from jeepney import DBusAddress, MatchRule, Properties, message_bus, new_method_call
from jeepney.io.blocking import open_dbus_connection
from jeepney.wrappers import unwrap_msg

RD_BUS = "org.gnome.Mutter.RemoteDesktop"
SC_BUS = "org.gnome.Mutter.ScreenCast"

# mutter gives the pointer focus to the surface under it a little after a
# motion (at its next pick): a press sent at once can go to no surface at
# all, and the application sees only the release
FOCUS_DELAY = 0.3

# evdev codes (linux/input-event-codes.h)
BUTTONS = {1: 0x110, 2: 0x112, 3: 0x111}
KEYS = {
    "esc": 1, "escape": 1, "minus": 12, "equal": 13, "backspace": 14, "tab": 15,
    "return": 28, "enter": 28, "ctrl": 29, "control": 29, "shift": 42,
    "alt": 56, "space": 57, "super": 125, "meta": 125, "logo": 125,
    "f1": 59, "f2": 60, "f3": 61, "f4": 62, "f5": 63, "f6": 64, "f7": 65,
    "f8": 66, "f9": 67, "f10": 68, "f11": 87, "f12": 88,
    "home": 102, "up": 103, "pageup": 104, "page_up": 104, "left": 105,
    "right": 106, "end": 107, "down": 108, "pagedown": 109, "page_down": 109,
    "insert": 110, "delete": 111, "comma": 51, "period": 52, "slash": 53,
    "semicolon": 39, "apostrophe": 40, "grave": 41, "backslash": 43,
    "bracketleft": 26, "bracketright": 27,
}
for _i, _c in enumerate("1234567890"):
    KEYS[_c] = 2 + _i
for _row, _first in (("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)):
    for _i, _c in enumerate(_row):
        KEYS[_c] = _first + _i


class _State:
    def __init__(self) -> None:
        self.screenshot_dir = Path("./screenshots")
        self.screenshot_seq = 0


class MutterCompositor:
    compositor_name = "mutter"
    # Where a window as large as the output lands, its top left corner.
    window_origin = (0, 0)

    def __init__(self, cfg: dict) -> None:
        w, h = str(cfg.get("screen", "1280x800")).lower().split("x")
        self.width, self.height = int(w), int(h)
        self.state = _State()
        self.log_dir: Path | None = None
        self.timeout = float(cfg.get("timeouts", {}).get("compositor_start", 30))
        self.procs: dict[str, subprocess.Popen] = {}
        self.app: subprocess.Popen | None = None
        self.runtime: Path | None = None
        self.conn = None
        self.rd_session: str | None = None
        self.stream: str | None = None
        self.node: int | None = None
        self.frames: Path | None = None
        self.env: dict[str, str] = {}
        self.wayland_display = ""
        self.pointer: tuple[float, float] | None = None

    # ── set-up ──────────────────────────────────────────────────────

    def set_log_dir(self, path) -> None:
        self.log_dir = Path(path)
        self.log_dir.mkdir(parents=True, exist_ok=True)

    def _log(self, name: str):
        if self.log_dir is None:
            return subprocess.DEVNULL
        return open(self.log_dir / f"mutter-{name}.log", "wb")

    def _spawn(self, name: str, cmd: list[str], env: dict[str, str] | None = None) -> subprocess.Popen:
        log = self._log(name)
        proc = subprocess.Popen(cmd, env=env or self.env, stdin=subprocess.DEVNULL,
                                stdout=log, stderr=subprocess.STDOUT,
                                start_new_session=True)
        self.procs[name] = proc
        return proc

    def _wait(self, what: str, check, timeout: float | None = None):
        deadline = time.monotonic() + (timeout or self.timeout)
        while time.monotonic() < deadline:
            value = check()
            if value:
                return value
            for name, proc in self.procs.items():
                if proc.poll() is not None:
                    raise RuntimeError(f"{name} exited ({proc.returncode}) while waiting for {what}")
            time.sleep(0.1)
        raise RuntimeError(f"timed out waiting for {what}")

    def start(self) -> None:
        rt = Path(tempfile.mkdtemp(prefix="wbox-mutter-"))
        os.chmod(rt, 0o700)
        self.runtime = rt
        config = rt / "config"
        keyfile = config / "glib-2.0" / "settings" / "keyfile"
        keyfile.parent.mkdir(parents=True)
        keyfile.write_text("[org/gnome/mutter]\nauto-maximize=false\n")
        env = {k: v for k, v in os.environ.items()
               if k not in ("WAYLAND_DISPLAY", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS")}
        env.update(XDG_RUNTIME_DIR=str(rt), XDG_CONFIG_HOME=str(config),
                   GSETTINGS_BACKEND="keyfile", NO_AT_BRIDGE="1",
                   GDK_BACKEND="wayland")
        # mutter's own traces in its log: WBOX_MUTTER_DEBUG=window-ops,...
        if os.environ.get("WBOX_MUTTER_DEBUG"):
            env["MUTTER_DEBUG"] = os.environ["WBOX_MUTTER_DEBUG"]
        self.env = env

        bus_path = rt / "bus"
        self._spawn("dbus", ["dbus-daemon", "--session", "--nofork", "--nopidfile",
                             f"--address=unix:path={bus_path}"])
        self._wait("the session bus", bus_path.exists)
        env["DBUS_SESSION_BUS_ADDRESS"] = f"unix:path={bus_path}"
        self._spawn("pipewire", ["pipewire"])
        self._wait("pipewire", lambda: (rt / "pipewire-0").exists())
        self._spawn("wireplumber", ["wireplumber"])
        self._spawn("mutter", ["mutter", "--headless", "--wayland", "--no-x11",
                               "--virtual-monitor", f"{self.width}x{self.height}"])

        self.conn = open_dbus_connection(bus=env["DBUS_SESSION_BUS_ADDRESS"])
        self._wait("mutter's RemoteDesktop on the bus",
                   lambda: self._has_owner(RD_BUS) and self._has_owner(SC_BUS))
        sockets = self._wait("mutter's Wayland socket",
                             lambda: sorted(p.name for p in rt.glob("wayland-*")
                                            if not p.name.endswith(".lock")))
        self.wayland_display = sockets[0]

        # A remote desktop session and its screen cast of the monitor.
        self.rd_session = self._call(RD_BUS, "/org/gnome/Mutter/RemoteDesktop",
                                     RD_BUS, "CreateSession")[0]
        session_id = unwrap_msg(self.conn.send_and_get_reply(
            Properties(DBusAddress(self.rd_session, RD_BUS, RD_BUS + ".Session"))
            .get("SessionId")))[0][1]
        sc_session = self._call(SC_BUS, "/org/gnome/Mutter/ScreenCast", SC_BUS,
                                "CreateSession", "a{sv}",
                                ({"remote-desktop-session-id": ("s", session_id)},))[0]
        self.stream = self._call(SC_BUS, sc_session, SC_BUS + ".Session", "RecordMonitor",
                                 "sa{sv}", (self._connector(), {"cursor-mode": ("u", 0)}))[0]
        rule = MatchRule(type="signal", interface=SC_BUS + ".Stream",
                         member="PipeWireStreamAdded", path=self.stream)
        unwrap_msg(self.conn.send_and_get_reply(message_bus.AddMatch(rule)))
        with self.conn.filter(rule) as queue:
            self._rd("Start")
            signal_msg = self.conn.recv_until_filtered(queue, timeout=self.timeout)
        self.node = int(signal_msg.body[0])
        # mutter makes the session's virtual pointer at its first event, and
        # the seat has no pointer until then: made later, a client binds its
        # wl_pointer after the motion that should have given it the focus,
        # and the next press goes to no surface. One motion now, before any
        # client.
        self._move(self.width - 1, self.height - 1)
        self.pointer = (self.width - 1, self.height - 1)

        # Every frame of the stream as a PNG file; the last few kept.
        self.frames = rt / "frames"
        self.frames.mkdir()
        self._spawn("capture", [
            "gst-launch-1.0", "-q", "pipewiresrc", f"path={self.node}",
            "always-copy=true", "keepalive-time=200", "!", "videoconvert", "!",
            "video/x-raw,format=RGB", "!", "pngenc", "compression-level=1", "!",
            "multifilesink", f"location={self.frames}/f%08d.png", "max-files=6"])
        self._wait("the first frame", lambda: len(list(self.frames.glob("f*.png"))) >= 2)

    def _has_owner(self, name: str) -> bool:
        reply = self.conn.send_and_get_reply(message_bus.NameHasOwner(name))
        return bool(unwrap_msg(reply)[0])

    def _call(self, bus: str, path: str, iface: str, method: str,
              signature: str | None = None, body: tuple = ()):
        msg = new_method_call(DBusAddress(path, bus, iface), method, signature, body)
        return unwrap_msg(self.conn.send_and_get_reply(msg, timeout=self.timeout))

    def _rd(self, method: str, signature: str | None = None, *args):
        return self._call(RD_BUS, self.rd_session, RD_BUS + ".Session", method,
                          signature, tuple(args))

    def _connector(self) -> str:
        state = self._call("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig",
                           "org.gnome.Mutter.DisplayConfig", "GetCurrentState")
        monitors = state[1]
        if not monitors:
            raise RuntimeError("mutter reports no monitor")
        return monitors[0][0][0]

    # ── the application ──────────────────────────────────────────────

    def launch(self, app_cmd: list[str], app_env: dict[str, str] | None = None) -> dict:
        try:
            if self.conn is None:
                self.start()
            env = dict(self.env)
            env["WAYLAND_DISPLAY"] = self.wayland_display
            env.update(app_env or {})
            log = self._log("app")
            self.app = subprocess.Popen(app_cmd, env=env, stdin=subprocess.DEVNULL,
                                        stdout=log, stderr=subprocess.STDOUT,
                                        start_new_session=True)
            return {"ok": True, "pid": self.app.pid, "wayland_display": self.wayland_display}
        except Exception as exc:  # the scenario reports it
            return {"error": f"{exc!r}; logs in {self.log_dir}"}

    def is_running(self) -> bool:
        proc = self.procs.get("mutter")
        return proc is not None and proc.poll() is None and self.conn is not None

    # ── input ────────────────────────────────────────────────────────

    def _move(self, x: float, y: float) -> None:
        self._rd("NotifyPointerMotionAbsolute", "sdd", self.stream, float(x), float(y))
        self.pointer = (x, y)

    def _focus(self, x: float, y: float) -> None:
        """The pointer at (x, y), and the surface there given its focus.

        mutter picks the surface under the pointer on a motion that moves
        it: the pointer goes a pixel aside first when it is there already
        (a window mapped under a still pointer has no focus yet), then
        waits for the pick before a press."""
        if self.pointer == (x, y):
            self._move(x + 1 if x + 1 < self.width else x - 1, y)
        self._move(x, y)
        time.sleep(FOCUS_DELAY)

    def _button(self, button: int, pressed: bool) -> None:
        self._rd("NotifyPointerButton", "ib", BUTTONS.get(button, button), pressed)

    def mouse_move(self, x: int, y: int) -> dict:
        if not self.is_running():
            return {"error": "compositor is not running"}
        self._move(x, y)
        return {"ok": True}

    def click(self, x: int, y: int, button: int = 1) -> dict:
        if not self.is_running():
            return {"error": "compositor is not running"}
        self._focus(x, y)
        self._button(button, True)
        time.sleep(0.05)
        self._button(button, False)
        return {"ok": True}

    def drag(self, x1: int, y1: int, x2: int, y2: int, button: int = 1,
             steps: int = 10, seconds: float = 0.3) -> dict:
        """Press at the start, travel in steps, release at the end."""
        if not self.is_running():
            return {"error": "compositor is not running"}
        steps = max(1, int(steps))
        self._focus(x1, y1)
        self._button(button, True)
        try:
            for i in range(1, steps + 1):
                time.sleep(max(0.0, seconds) / steps)
                self._move(x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)
            time.sleep(0.05)
        finally:
            self._button(button, False)
        return {"ok": True}

    def key(self, shortcut: str) -> dict:
        if not self.is_running():
            return {"error": "compositor is not running"}
        names = [n.strip().lower() for n in shortcut.split("+") if n.strip()]
        codes = []
        for name in names:
            if name not in KEYS:
                return {"error": f"unknown key {name!r}"}
            codes.append(KEYS[name])
        for code in codes:
            self._rd("NotifyKeyboardKeycode", "ub", code, True)
            time.sleep(0.02)
        for code in reversed(codes):
            self._rd("NotifyKeyboardKeycode", "ub", code, False)
            time.sleep(0.02)
        return {"ok": True}

    def keys(self, shortcuts: list[str], delay_ms: int = 100) -> dict:
        for n, shortcut in enumerate(shortcuts):
            done = self.key(shortcut)
            if "error" in done:
                return {"error": done["error"], "sent": n}
            time.sleep(max(0, delay_ms) / 1000)
        return {"ok": True, "sent": len(shortcuts)}

    # ── capture ──────────────────────────────────────────────────────

    def _frame_after(self, since: float, timeout: float = 10) -> Path:
        """The first complete frame written after `since` (time.time())."""
        def find():
            files = sorted(self.frames.glob("f*.png"))
            # the newest may still be written: the one before is complete
            for path in reversed(files[:-1]):
                try:
                    if path.stat().st_mtime >= since:
                        return path
                except FileNotFoundError:
                    continue
            return None
        return self._wait("a frame after the request", find, timeout)

    def screenshot(self, name: str | None = None, scale: float | None = None,
                   region: str | None = None) -> dict:
        """The output (or a region "x,y WxH" of it) as a PNG file."""
        if not self.is_running():
            return {"error": "compositor is not running"}
        self.state.screenshot_seq += 1
        if not name:
            name = f"mutter_{self.state.screenshot_seq:04d}.png"
        elif not name.endswith(".png"):
            name += ".png"
        self.state.screenshot_dir.mkdir(parents=True, exist_ok=True)
        dest = self.state.screenshot_dir / name
        try:
            # keepalive repeats the last frame every 200 ms: two periods on,
            # the frame shows what the output showed at the request
            frame = self._frame_after(time.time() + 0.45)
            tmp = dest.with_suffix(".src.png")
            shutil.copyfile(frame, tmp)
        except Exception as exc:
            return {"error": repr(exc)}
        if not region and not scale:
            tmp.replace(dest)
            return {"path": str(dest), "size": dest.stat().st_size}
        chain = []
        if region:
            pos, size = region.split()
            x, y = (int(v) for v in pos.split(","))
            w, h = (int(v) for v in size.lower().split("x"))
            chain += ["videocrop", f"left={x}", f"top={y}",
                      f"right={max(0, self.width - x - w)}",
                      f"bottom={max(0, self.height - y - h)}", "!"]
        if scale:
            chain += ["videoscale", "!", "video/x-raw,width=%d,height=%d" % (
                int(self.width * scale), int(self.height * scale)), "!"]
        done = subprocess.run(
            ["gst-launch-1.0", "-q", "filesrc", f"location={tmp}", "!", "pngdec", "!",
             "videoconvert", "!", *chain, "pngenc", "compression-level=1", "!",
             "filesink", f"location={dest}"],
            env=self.env, capture_output=True, text=True, timeout=30)
        tmp.unlink(missing_ok=True)
        if done.returncode != 0:
            return {"error": f"gst crop failed: {done.stderr.strip() or done.stdout.strip()}"}
        return {"path": str(dest), "size": dest.stat().st_size}

    # ── teardown ─────────────────────────────────────────────────────

    def stop(self) -> dict:
        if self.app is not None and self.app.poll() is None:
            self.app.terminate()
            try:
                self.app.wait(5)
            except subprocess.TimeoutExpired:
                self.app.kill()
        if self.conn is not None:
            try:
                if self.rd_session:
                    self._rd("Stop")
            except Exception:
                pass
            self.conn.close()
            self.conn = None
        capture = self.procs.get("capture")
        if capture is not None and capture.poll() is None:
            capture.send_signal(signal.SIGINT)
        for name in ("capture", "mutter", "wireplumber", "pipewire", "dbus"):
            proc = self.procs.get(name)
            if proc is None:
                continue
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        self.procs.clear()
        if self.runtime is not None:
            shutil.rmtree(self.runtime, ignore_errors=True)
            self.runtime = None
        return {"ok": True}
