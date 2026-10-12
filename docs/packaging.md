# Packaging

An egui-pop application is one executable, but the desktop knows it by
more than that: a name and an icon in menus, an id its notifications,
shortcuts and links are tied to, a manifest on Windows, a bundle on
macOS. This page says what each platform expects, and what each platform
service of `egui-pop-platform` needs from the package. The examples use
an application whose id is `org.example.Viewer`, whose executable is
`viewer` and whose links are `viewer://...`.

Give the same id everywhere: to eframe (`ViewportBuilder::with_app_id`,
the Wayland app id and the X11 class), to the plugins
(`SystemKeysPlugin::new`, `TrayPlugin::app_id`, `Desktop::app_id`,
`instance::single`), and to the package's files below.

```rust
use egui_pop::platform::notify::{Desktop, NotifyPlugin};
use egui_pop::platform::tray::TrayPlugin;
use egui_pop::platform::keys::SystemKeysPlugin;

const APP_ID: &str = "org.example.Viewer";

let notifications = NotifyPlugin::new("Viewer").backend(Desktop::new("Viewer").app_id(APP_ID));
let tray = TrayPlugin::new("Viewer").app_id(APP_ID);
let keys = SystemKeysPlugin::new(APP_ID);
# let _ = (notifications, tray, keys);
```

## What each service needs

| Service | Linux | Windows | macOS |
|---|---|---|---|
| Single instance and deep links | the desktop file's `Exec=viewer %U` and `MimeType=x-scheme-handler/viewer` | a `viewer` URL protocol in the registry | `CFBundleURLTypes` in `Info.plist` (links arrive as Apple events, which winit does not hand to egui yet: a gap) |
| Notifications | the desktop file named after the id (the `desktop-entry` hint) | an AppUserModelID on the Start menu shortcut | the bundle identifier |
| Tray | nothing (a StatusNotifierItem host on the desktop; on GNOME the AppIndicator extension) | nothing | nothing; `LSUIElement` for an application living in the menu bar only |
| Global shortcuts | on Wayland, the desktop file named after the id: the portal refuses an application it cannot find | nothing | nothing |
| Logs and crash reports | nothing (`$XDG_STATE_HOME`) | nothing (`%LOCALAPPDATA%`) | nothing (`~/Library/Logs`, `~/Library/Application Support`) |
| Focus mode | nothing (the Settings portal; `kwinrc` on KDE) | nothing | nothing |

### Single instance and deep links

The single instance needs nothing installed: its socket is made at run
time. Deep links need the system to know the scheme: the desktop file's
`MimeType=x-scheme-handler/viewer` and `Exec=viewer %U` on Linux, a URL
protocol in the registry on Windows, `CFBundleURLTypes` on macOS (see
each platform below). A link opened while the application runs starts a
second launch, which hands it over (`instance.second_launch`, then
`deeplink.open`).

### Notifications

The desktop ties notifications to the application's id: the desktop
file's name on Linux, the AppUserModelID of the Start menu shortcut on
Windows, the bundle identifier on macOS. Give it with
`Desktop::app_id`. Inside Flatpak, allow the notification server's bus
name.

### Tray

Nothing to install. The desktop must run a StatusNotifierItem host on
Linux (KDE, Xfce, Cinnamon, LXQt, the bars of tiling compositors; on
GNOME the AppIndicator extension); inside Flatpak, allow the watcher's
bus name. On macOS, `LSUIElement` hides the Dock icon of an application
that lives in the menu bar.

### Global shortcuts

On Wayland the GlobalShortcuts portal must find the application: a
sandboxed one by its Flatpak id, another by its desktop file, named
after the id given to `SystemKeysPlugin::new` and installed where the
desktop looks (`/usr/share/applications`,
`~/.local/share/applications`). Without it the portal refuses ("an app
id is required") and the shortcuts editor says so. Elsewhere nothing is
needed.

### Logs and crash reports

Nothing to install: the files go to the user's state directory. A
package may point its bug report instructions at the `logs.reveal`
command.

### Focus mode

Nothing to install; inside Flatpak on KDE, allow reading `kwinrc`.

## Linux

### The desktop file

`org.example.Viewer.desktop`, installed in
`/usr/share/applications` (or `~/.local/share/applications` for one
user). Its name is the application's id: the global shortcuts portal
looks the application up by it, notification servers find its icon and
settings through it, and the compositor matches the window's app id to
it.

```ini
[Desktop Entry]
Type=Application
Name=Viewer
Comment=Look at photos
Exec=viewer %U
Icon=org.example.Viewer
Terminal=false
Categories=Graphics;Viewer;
MimeType=x-scheme-handler/viewer;image/jpeg;image/png;
StartupNotify=true
StartupWMClass=org.example.Viewer
SingleMainWindow=true
X-GNOME-UsesNotifications=true
```

- `%U` passes files and links, so a link opened from a browser starts
  `viewer viewer://album/3`, which `instance::single` hands to the
  running instance.
- `MimeType=x-scheme-handler/viewer` makes it the handler of
  `viewer://` once `update-desktop-database` ran (a package's scripts
  do); `xdg-mime default org.example.Viewer.desktop
  x-scheme-handler/viewer` makes it the default for one user.
- `StartupNotify=true` makes the launcher pass an activation token
  (`XDG_ACTIVATION_TOKEN`), which a second launch hands over.
- `X-GNOME-UsesNotifications=true` lists it in GNOME's notification
  settings.

### AppStream

Software centres read `/usr/share/metainfo/org.example.Viewer.metainfo.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>org.example.Viewer</id>
  <name>Viewer</name>
  <summary>Look at photos</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <launchable type="desktop-id">org.example.Viewer.desktop</launchable>
  <provides>
    <binary>viewer</binary>
  </provides>
  <description>
    <p>A photo viewer built with egui-pop.</p>
  </description>
  <releases>
    <release version="1.0.0" date="2026-10-12"/>
  </releases>
  <content_rating type="oars-1.1"/>
</component>
```

Check it with `appstreamcli validate`.

### Icons

Install the icon in the hicolor theme under the id's name:
`/usr/share/icons/hicolor/scalable/apps/org.example.Viewer.svg`, or PNGs
at `16x16`, `32x32`, `48x48`, `64x64`, `128x128`, `256x256` and
`512x512` in `hicolor/<size>/apps/`. The window's own icon comes from
eframe (`ViewportBuilder::with_icon`); the tray's from its RGBA pixels
(`tray::Icon`), not from the theme.

### Flatpak

The sandbox changes a few things:

```yaml
finish-args:
  - --socket=wayland
  - --socket=fallback-x11
  - --share=ipc
  - --device=dri
  # Notifications (notify-rust talks to the notification server).
  - --talk-name=org.freedesktop.Notifications
  # The tray (a StatusNotifierItem without a well-known name).
  - --talk-name=org.kde.StatusNotifierWatcher
  # The focus mode on KDE: kwinrc, read only.
  - --filesystem=xdg-config/kwinrc:ro
```

- The single instance's socket is a file in `$XDG_RUNTIME_DIR/app/<id>`,
  which every sandbox of the application shares.
- Global shortcuts and the GNOME focus mode go through portals, which
  know a sandboxed application by its Flatpak id: no permission needed.
- Logs and crash reports land in `~/.var/app/<id>/.local/state`.
- X11 global shortcuts work only on an X11 session
  (`--socket=x11`); on Wayland the portal is used.

## Windows

### The manifest and the icon

Embed an application manifest and an icon with a build script, with
[winresource](https://crates.io/crates/winresource) or
[embed-resource](https://crates.io/crates/embed-resource). The manifest
declares the DPI awareness egui expects (per monitor, version 2), the
common controls version 6 (the tray's menus use them) and the supported
systems:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="org.example.Viewer" version="1.0.0.0"/>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
```

The icon is an `.ico` holding 16, 24, 32, 48 and 256 pixel images.
`#![windows_subsystem = "windows"]` in `main.rs` keeps a console from
opening.

### Installer options

- **MSIX**: the modern package; it gives the application an identity
  (notifications work without a shortcut) and declares the URL protocol
  in its manifest (`windows.protocol`).
- **MSI** through WiX ([cargo-wix](https://crates.io/crates/cargo-wix)),
  **Inno Setup** or **NSIS**: classic installers; they create the Start
  menu shortcut and the registry keys below.
- [cargo-packager](https://crates.io/crates/cargo-packager) makes NSIS
  and WiX installers, macOS bundles and Linux packages from one
  configuration.

What the installer sets for the platform services:

- The Start menu shortcut carries the AppUserModelID
  (`System.AppUserModel.ID` = `org.example.Viewer`): Windows ties toasts
  to it. Without it, notify-rust shows them as PowerShell's.
- The URL protocol, per user or per machine:

  ```text
  HKCU\Software\Classes\viewer                      (Default) = "URL:Viewer"
  HKCU\Software\Classes\viewer                      URL Protocol = ""
  HKCU\Software\Classes\viewer\DefaultIcon          (Default) = "C:\...\viewer.exe,0"
  HKCU\Software\Classes\viewer\shell\open\command   (Default) = "C:\...\viewer.exe" "%1"
  ```

### Code signing

Unsigned executables meet SmartScreen warnings. Sign the executable and
the installer with `signtool` and a code signing certificate, or with
Azure Trusted Signing; see Microsoft's documentation on code signing for
Windows applications.

## macOS

### The bundle

An application is a bundle, `Viewer.app/Contents/` with `MacOS/viewer`,
`Resources/Viewer.icns` and `Info.plist` (cargo-packager or
[cargo-bundle](https://crates.io/crates/cargo-bundle) make it):

```xml
<key>CFBundleIdentifier</key>      <string>org.example.Viewer</string>
<key>CFBundleName</key>            <string>Viewer</string>
<key>CFBundleExecutable</key>      <string>viewer</string>
<key>CFBundleIconFile</key>        <string>Viewer</string>
<key>CFBundleShortVersionString</key> <string>1.0.0</string>
<key>NSHighResolutionCapable</key> <true/>
<key>CFBundleURLTypes</key>
<array>
  <dict>
    <key>CFBundleURLName</key>    <string>org.example.Viewer</string>
    <key>CFBundleURLSchemes</key> <array><string>viewer</string></array>
  </dict>
</array>
<!-- An application living in the menu bar only, without a Dock icon: -->
<!-- <key>LSUIElement</key> <true/> -->
```

- Notifications are tied to the bundle identifier
  (`Desktop::app_id`); outside a bundle they show as another
  application's.
- macOS keeps a bundle single by itself, and delivers links and files
  opened later as Apple events; winit does not hand those to egui yet,
  so a link reaches a running bundle only when it starts it. A gap.
- Global shortcuts (Carbon hot keys) need no accessibility permission.

### Signing and notarisation

Gatekeeper refuses unsigned downloads. Sign the bundle with a Developer
ID certificate and the hardened runtime (`codesign --options runtime`),
send it to Apple's notary service (`xcrun notarytool submit --wait`),
and staple the ticket (`xcrun stapler staple Viewer.app`); see Apple's
documentation on notarizing macOS software before distribution.
