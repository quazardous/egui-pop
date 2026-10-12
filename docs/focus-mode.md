# The focus mode

egui-pop follows the window manager's focus mode: where the keyboard
goes when the pointer moves. In **click** mode it goes where the user
clicks; in **hover** mode the surface under the pointer gets the keys
(see `egui_pop::core::focus_mode`). The `appearance.focus` setting
(`system`, `click`, `hover`) lets the user decide; `system` asks the
desktop.

By default `run_native` asks the desktop once, at start, through a
helper program (`gsettings`, `kreadconfig6`, `reg`). With
`egui-pop-platform` (feature `platform`, or `platform-focus` alone) the
same answers are read in the process, and followed while the
application runs:

```toml
egui-pop = { version = "0.1", features = ["platform-focus"] }
```

```rust,no_run
use egui_pop::platform::focus::{self, FocusModePlugin};
use egui_pop::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::builder()
        .plugins(DefaultPlugins)
        // Read in-process now: `run_native` starts no helper.
        .system_focus(focus::read())
        // Read again, from a thread, each time the window gets the focus
        // back: a change in the desktop's settings applies at once.
        .plugin(FocusModePlugin::new())
        .run_native("Viewer", Default::default())?;
    Ok(())
}
```

## What each desktop says

| | Read from | Hover when |
|---|---|---|
| GNOME, and desktops using its settings (Budgie, Pantheon...) | the XDG Settings portal: `org.gnome.desktop.wm.preferences`, `focus-mode` | `sloppy` or `mouse` |
| KDE Plasma | `kwinrc` (`$XDG_CONFIG_HOME`), group `Windows`, `FocusPolicy`, read as a file: the portal does not carry KWin's settings | `FocusFollowsMouse`, `FocusUnderMouse`, `FocusStrictlyUnderMouse` |
| Windows | `HKCU\Control Panel\Desktop\UserPreferencesMask`, its active window tracking bit: what `SystemParametersInfo(SPI_GETACTIVEWINDOWTRACKING)` reports, read through the registry API (egui-pop has no unsafe code, so not through `SystemParametersInfo` itself) | the bit is set ("Activate a window by hovering over it with the mouse") |
| macOS | nothing: its window manager has no focus following the pointer | never |
| Other Linux desktops (sway, Hyprland, Xfce...), the web | nothing read | never |

`EGUI_POP_FOCUS_MODE` (`click` or `hover`) stands in for the desktop,
for tests and for desktops egui-pop does not read. The portal is asked
with a two-second timeout, so a session without one starts as fast.

The plugin reads again when the window gets the focus back rather than
listening to each desktop's change signal: a user changes the focus mode
in the desktop's settings, then comes back to the application, which
reads it then. `FocusModePlugin::fixed` answers a given mode instead,
for tests and pictures.
