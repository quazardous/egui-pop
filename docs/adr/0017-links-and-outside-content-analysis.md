# 0017, appendix: the analysis behind links and outside content

The analysis that led to [ADR 0017](0017-links-and-outside-content.md),
kept for the reasons it gives. Where it differs from the ADR, the ADR
wins.

## What exists today

Read from the code on `main` (2026-10-28), egui, eframe and egui-winit
0.36.2.

### In egui-pop

| Site | What it does | How | Gaps |
|---|---|---|---|
| Terminal, Ctrl+click on a link (`egui-pop-editors/src/terminal/view.rs`, around line 415) | opens `http://`, `https://` and `file://` addresses found in the output (`terminal/links.rs`), and emits `TerminalSignal::Link` | `ctx.open_url(OpenUrl::new_tab(..))` | **bug:** a no-op on native (see below); a `file://` link would go to a browser, never to the file manager; no step, no confirmation for text a program printed |
| Gallery, built-in widgets page (`egui-pop-gallery/src/pages/builtins.rs`) | `ui.hyperlink_to("Hyperlink", <repository>)` | egui's `Hyperlink` | the same no-op on native; no hover target |
| Hover cards (`egui-pop-core/src/hover_card.rs`) | "links" that run a command with arguments | `ui.link`, the owner runs the command | fine: command links declared by code; they are the model for `command:` links |
| Toasts (`egui-pop-api/src/toasts.rs`) | an action button running a command; the module's example uses `files.reveal` | a command | `files.reveal` exists in no plugin: "Show in folder" has nothing to run |
| Single instance and deep links (`egui-pop-platform`, `docs/instance.md`) | `<app>://` links given at launch or by a second launch run `deeplink.open` | a step and a command | fine: the application's own scheme already has its way in; a click inside the application can reuse it |
| Theme (`egui-pop-core/src/theme.rs`) | `hyperlink_color` from the accent | egui visuals | `url_in_tooltip` stays off: a link never shows where it goes |
| Tips (`docs/tips.md`) | title and plain text | | no link in a tip; "tips in files (`tips/<id>.md`)" is listed as not done |
| Files (`docs/files.md`) | choosing files and folders | rfd, the portal | nothing reveals or opens a file outside |
| Dialogs (`egui_pop::api::dialogs::Confirm`) | a confirmation running a command | | ready for an open confirmation |
| Debug bridge, agents | no `links` collector, no open verb | | an agent cannot ask to open anything, a test cannot see a request |

### What egui and eframe do with a link

- `egui::Hyperlink` draws a `Link` (accessibility role `link`, egui's
  `WidgetType::Link`), and on a click emits
  `OutputCommand::OpenUrl { url, new_tab }`; a click "to open in the
  background" (Ctrl or Cmd held, or the middle button) forces
  `new_tab: true`. The URL shows on hover only when the style's
  `url_in_tooltip` is on (off by default). Every scheme is accepted.
- **Native** (egui-winit's `open_url_in_browser`): calls
  `webbrowser::open(url)` when the `links` feature is on, else logs
  "Cannot open url - feature \"links\" not enabled." and does nothing.
  eframe's `links` feature is in its default set, but egui-pop depends
  on eframe with `default-features = false` and does not list `links`;
  `webbrowser` is not in `Cargo.lock`. **Every link is a silent no-op on
  native today** (ADR 0003). An application that turns the feature on
  itself gets it back through feature unification, unknowingly.
- **Web** (eframe's `web::open_url`):
  `window.open(url, new_tab ? "_blank" : "_self")`. `_self` replaces the
  application's page: the application is gone. A pop-up blocker may
  refuse `_blank` when the browser no longer counts the click as a
  user gesture.
- The commands are in the frame's `PlatformOutput` until eframe handles
  them after the frame: an application can read and remove them
  (`ctx.output_mut(|o| o.commands.retain(..))`) at the end of its own
  frame. That is how the service intercepts every `OpenUrl`, whichever
  widget made it.

### What the inventory says

1. The fix comes first: links do nothing on native.
2. The terminal already distinguishes "click" from "Ctrl+click" and
   reports a signal; it needs a target classified (a `file://` path is
   not a web page) and a policy (its text comes from a program).
3. The application's own scheme and command links already exist; they
   only lack a way in from a click.
4. Nothing to migrate except two `open_url` uses and one hyperlink.

## Survey

### Openers (native and web)

| Crate | Licence | What it does | Verdict |
|---|---|---|---|
| egui `OpenUrl` and eframe | MIT OR Apache-2.0 | web pages; new or same tab on the web | **keep**, intercepted; turn eframe's `links` on |
| webbrowser 1.x | MIT OR Apache-2.0 | guarantees a **browser** opens, even for local files; `$BROWSER` honoured on Linux; a `hardened` feature refuses everything but http(s); wasm opens the default browser | **adopt** (through eframe's `links`) for web pages |
| opener 0.9 | MIT OR Apache-2.0 | `open` (the default application), `open_browser`, `reveal` (feature `reveal`, D-Bus through zbus on Linux) | **adopt** in `SystemOpener` for files, folders, `mailto:` and reveal |
| showfile | MIT | reveal only | skip: opener covers it |
| ashpd 0.13 | MIT | the desktop portals: `OpenURI.OpenURI`, `OpenFile` (version 2), `OpenDirectory` (version 3, opens the folder of a file) | **wrap** inside Flatpak and Snap; already in the tree through rfd (with zbus) |
| own spawn of `xdg-open`, `open`, `ShellExecuteW` | | | no: the crates above do it and know the corner cases (WSL, `$BROWSER`) |

Per platform:

- **Linux.** `xdg-open` picks the desktop's handler. The freedesktop
  `org.freedesktop.FileManager1.ShowItems` D-Bus call reveals a file
  selected in its folder (Nautilus, Dolphin, Nemo and others implement
  it; a fallback opens the parent folder). Inside **Flatpak** the
  runtime's `xdg-open` forwards to the `OpenURI` portal; `OpenFile`
  takes a file descriptor, so the sandbox can only open what it can
  read; a writable descriptor must say so or the portal rejects it.
  Users report links doing nothing when no portal backend answers
  OpenURI: the error must reach a toast. Inside **Snap**, the base's
  `xdg-open` forwards to the host through `snap userd` (the `desktop`
  plug); its scheme list has been short (http, https, mailto), and
  local files go through a prompt.
- **macOS.** `open <url>`, `open -R <path>` to reveal (NSWorkspace
  underneath); opener does both.
- **Windows.** `ShellExecuteW` with the `open` verb (it would also *run*
  an executable: the policy refuses those before), and
  `SHOpenFolderAndSelectItems` (or `explorer /select,`) to reveal.
- **Web build.** Only `window.open`: no local files, no reveal, no
  default application. `mailto:` works in the same tab (the browser
  hands it over and the page stays); in a new tab it leaves an empty
  tab behind in some browsers.

### Showing markdown, HTML and images without a web engine

| Crate | Licence | What it brings | Verdict |
|---|---|---|---|
| egui_commonmark 0.26 | MIT OR Apache-2.0 | CommonMark plus GitHub tables, strikethrough, task lists, footnotes; images through egui's loaders (`fetch` for remote ones, `svg`), code highlighted by syntect (`better_syntax_highlighting`, syntect already in the tree for the editors), a hook to render inline HTML (`RenderHtmlFn`) and maths; links drawn as egui hyperlinks (so intercepted); it follows egui's minors (0.26 is on egui 0.37) | **wrap** in the viewer, behind a feature, its version tied to egui's |
| pulldown-cmark | MIT | the parser egui_commonmark uses | through egui_commonmark; directly for our inline subset |
| egui_markdown, egui demo's "easy mark" | not checked | smaller renderers | skip: less complete |
| html5ever | MIT OR Apache-2.0 | a spec-compliant HTML parser (Servo's, published on its own) | **adopt** for the HTML subset |
| ammonia | MIT OR Apache-2.0 | an allow-list HTML sanitiser on html5ever | **adopt**: no scripts, styles, forms, iframes, event attributes; links' schemes checked |
| own mapping | | the cleaned tree mapped to the viewer's blocks (headings, paragraphs, lists, tables, code, quotes, images, links) | **own**: no crate draws HTML in egui |
| egui_extras loaders | MIT OR Apache-2.0 | `file` and `image` (and `svg`) loaders; `http` fetches any `http(s)` URI an image is given (through ehttp) | **adopt** `file`, `image`, `svg`; the `http` loader is not installed: remote images go through a loader of ours that asks the policy first |

Rendering inside needs no network for shipped pages and works the
same on the web build: these are egui widgets.

### Web engines

| Option | Licence | Platforms | Cost | Verdict |
|---|---|---|---|---|
| wry (Tauri) | MIT OR Apache-2.0 | WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux | little in the binary (the system's engine), but on Linux GTK must be initialised and its loop pumped beside winit; a child view inside a winit window works **on X11 only**, Wayland needs a GTK container (so its own GTK window); WebKitGTK must be installed (in Flatpak, a runtime that ships it, such as GNOME's; to be checked per runtime version); the web view floats above egui (no egui popup over it) | **wait**: an opt-in plugin, in its own window, a later question |
| egui_webview (hello_egui) | MIT | wry inside egui | experimental, unreleased, `unsafe` around Send and Sync, needs a wry change for screenshots | **wait** |
| Servo (`servo` 0.1 on crates.io, 2026) | MPL-2.0 | an engine compiled in | large (not measured here), an embedding API still moving | **reject**: `deny.toml` refuses MPL |
| Blitz (Dioxus) | MIT OR Apache-2.0, on Stylo (MPL-2.0) | HTML and CSS, **no JavaScript**, beta; renders through Vello or into a wgpu texture | would be the natural "safe HTML" engine, but Stylo is MPL and it brings a second renderer | **reject** for now; watch |
| CEF | BSD-3-Clause | Chromium | well over 100 MB per platform, its own process model | **reject** |
| An `<iframe>` over the canvas (web build) | | the browser itself | a DOM element above egui's canvas: popups and menus go under it, focus leaves egui | **wait**, with the webview question |

So: YouTube, video and full pages need a real engine, which costs a
GTK dependency or MPL code or 100 MB; none fits by default. The
**thumbnail that opens outside** gives most of the value: a card with
the title, the host and the duration the application declares, a play
icon, a click opening the browser. Its image is either shipped with the
application or fetched only after the user allows that host.

### Prior art

| Application | Links and help | Embedded content | What carries over |
|---|---|---|---|
| **VS Code** | Links in the editor follow on Ctrl+click (a hover says so); a link to a host not in the user's **trusted domains** opens a dialog showing the full URL with Open, Copy, Cancel and "Configure Trusted Domains"; `command:` URIs in hovers and webviews run only in markdown marked trusted | Markdown preview in a webview with a **Strict** security level by default: no scripts, no `http` images, an alert offering to change it; an integrated browser for localhost links | Ctrl+click, the confirmation dialog with Copy and a per-host trust, command links only in trusted text, remote content blocked until allowed |
| **Blender** | F1 or the context menu's "Online Manual" opens the page for the control under the pointer in the system browser; Help menu links likewise; an alert when a control has no page | none: the manual is a website | context help from a control to a docs page; no embedded browser |
| **Krita** | The welcome screen's community links (help, development fund, source, community) open the browser; the handbook is online | the welcome screen's **news is off by default**, because showing it fetches from krita.org | no third-party request before the user opts in |
| **Obsidian** | Notes are markdown rendered in the application; links to notes stay inside, external links open the system browser | the **Web viewer** core plugin (desktop, off by default) opens external links in tabs inside, on Chromium's webview, with ad blocking and a reader mode; its help warns that plugins can read its cookies | markdown inside; a web engine only as an opt-in plugin, with its risks written down |
| **egui ecosystem** | `Hyperlink` opens on a click, Ctrl or middle click for a new tab, no confirmation, no URL on hover by default | egui_commonmark for markdown, egui_webview experimental | keep the widget's habits (click, Ctrl+click, middle click), add the target on hover and the policy |

### What carries over

- Plain click opens, Ctrl+click (and middle click) opens outside, as in
  egui and editors.
- The real target is always visible before the click (hover) and in the
  confirmation; Copy is offered wherever Open is.
- A per-host trust, asked once per session, avoids a dialog on every
  click without opening everything silently.
- Remote content (images, previews, news) loads only after the user
  allows it.
- Markdown is rendered inside; a web engine is an opt-in extra, never
  the base.

## The questions, answered

### 1. What a click does, per kind of target

The policy classifies a target from its scheme, its host and its path's
extension (never by fetching it):

- **Web page:** outside (the system browser; a new tab on the web
  build). Inside only when the host is one the application declares as
  its documentation *and* the page is markdown or HTML the viewer can
  render.
- **Video** (`youtube.com`, `youtu.be`, `vimeo.com`, a remote `.mp4` or
  `.webm`): outside, always. A link declared as a video can be drawn as
  a card that opens outside.
- **Markdown, HTML:** `docs:` pages (shipped with the application,
  resolved by the host) open inside when the viewer is present; without
  it, the host's online copy opens outside, or a toast says there is
  none.
- **Image:** a local one inside (the viewer's image page), else in the
  system's default application; a remote one outside.
- **Local file:** the system's default application, after a
  confirmation; an executable or a script (by the platform's rules:
  the executable bit, `.exe`, `.bat`, `.sh`, `.desktop`, `.app`...) is
  refused and Reveal is offered.
- **Folder:** the file manager.
- **`mailto:`:** the mail client.
- **The application's own scheme:** `deeplink.open`, in-process.
- **`command:<id>?<json>`:** runs the command, from declared text only.

Modifiers: Ctrl+click (Cmd+click on macOS) and middle click mean
**outside**; Shift is left free. Hover shows the real target (a tooltip
after the usual delay, and the status bar's message slot when the
application has one). The context menu offers Open, Open outside (when
the default is inside), Reveal in folder (files), Copy link. Keyboard:
links are in the Tab order, Enter and Space open, Shift+F10 or the Menu
key opens the context menu. Accessibility: role `link`, the label as
name, the target as description.

### 2. The viewer

A panel (`links.viewer`), detachable, one page at a time:

- a toolbar: back, forward, reload, the **address bar** (shows the real
  target; typing a target opens it through the same policy), Open
  outside, Copy link;
- **history** for the session (back and forward stacks), no history
  kept across runs;
- **find in page** (Ctrl+F, F3 and Shift+F3), searching the rendered
  text;
- **zoom** (Ctrl+plus, Ctrl+minus, Ctrl+0 while it has the focus),
  scaling the page only;
- links inside a page go through the service: a link to another `docs:`
  page stays inside, a web link goes outside;
- anchors (`#section`) scroll to the heading;
- remote images replaced by a placeholder and one bar, "Images from
  <host> are not loaded: Load", remembered per host.

What renders without a web engine: markdown (egui_commonmark), the HTML
subset (html5ever, ammonia, our mapping), images (egui's loaders). What
does not: JavaScript, CSS layout, video, YouTube's player, forms: those
open outside. Because the viewer is egui widgets, it works on the web
build too, where `docs:` pages are fetched from the same origin as the
application.

### 3. Opening on each platform

`EguiOpener` emits egui's `OpenUrl` after the policy approved it (the
service marks it so its own interceptor lets it through). On native it
needs eframe's `links` feature (webbrowser); on the web, a new tab,
except `mailto:` in the same tab. `SystemOpener` (in
`egui-pop-platform`) adds files, folders and reveal: inside Flatpak or
Snap (detected by `/.flatpak-info` or `$SNAP`) through the portal
(ashpd: `OpenURI`, `OpenFile`, `OpenDirectory`), elsewhere through
opener (`open`, `reveal`). On the web build `SystemOpener` is not
available: reveal and local files are refused with the reason. A host
replaces the opener entirely (a kiosk that opens nothing, a host that
opens in its own browser).

### 4. Trust and privacy

Every request carries its **source**:

| Source | Example | Web and mail | Local file | Reveal | Command |
|---|---|---|---|---|---|
| Declared | About, menus, tips and toasts shipped by the application, `docs:` pages | open | confirm | open | run |
| Content | a document, a remote page, the terminal's output, a paste, text from an agent displayed in a page | open when the host is trusted, else confirm | confirm | confirm | refused |
| Agent | `links.open` | confirm, always | confirm, always | confirm, always | refused (agents run commands directly, under their own rules) |

Disguised links: the label and the target are both shown when the
label looks like a URL and differs; an internationalised host shows its
punycode form beside it; the host is emphasised in the dialog. Schemes
allowed by default: `https`, `http`, `mailto`, `file`, `docs`, the
application's own, `command` (declared). Blocked: everything else,
`javascript:` and `data:` first. Privacy: no request to a third party
before a click (remote images, video thumbnails, favicons, link
previews); the viewer never sends a referrer or cookies (it has none);
the "load images from this host" choice is a setting the user can
clear. Rate limits: content opens only on a click (one click, one
open); an agent has one confirmation pending per session, five
requests queued, then refusals with the reason.

### 5. Offline and failures

| Case | Outside | Inside (viewer) |
|---|---|---|
| No network | the browser reports it | error page: "No connection", Retry, Open outside, Copy link |
| Dead link (404, host unknown) | the browser reports it | error page with the status |
| Blocked scheme | warning toast naming the scheme, Copy link | the same |
| No handler (no browser, no mail client, portal refused or absent) | warning toast with the opener's reason and Copy link | n/a |
| File gone | warning toast, Reveal the folder if it exists | error page |
| Too large or a type it cannot render | n/a | error page, Open outside |
| Headless | `link.failed` with the reason in the journal | n/a |

Every failure also ends the `link.open` step with an error, so a hook,
an agent or a test sees it.

### 6. Checking

- `links::lint(&app)` lists every declared link: menu items whose
  command is `links.open`, About's links, tips' and toasts' texts,
  `docs:` pages and the links inside them. Each must parse, use an
  allowed scheme, name an existing command, `docs:` page, anchor or
  bundled file. A test in the gate runs it on the gallery and on the
  extension template.
- `cargo xtask links --online` (opt-in, never in the gate) checks that
  declared remote links answer.
- `RecordingOpener` records `(target, how, source, origin)` and answers
  as scripted (success, a given failure); kittest tests click a link by
  its role and name and read the record.
- Agents: `links.open` (`target`, `how`, `dry_run`) and `links.check`
  (`target`: the classification and the verdict, nothing opened); the
  `links` collector shows the recent requests, their source, the
  decision and the outcome.

### 7. The API

```rust,ignore
// egui-pop-model::links (pure)
pub enum How { Auto, Outside, Inside, Reveal }
pub enum Source { Declared, Content, Agent }
pub enum Kind { Web, Video, Doc, Markdown, Html, Image, File, Folder, Mail, AppLink, Command, Blocked(Reason) }
pub struct Target { /* the parsed URL or path */ }
pub struct Request { pub target: Target, pub how: How, pub source: Source, pub origin: Origin }
pub struct Decision { pub kind: Kind, pub how: How, pub confirm: bool, pub refused: Option<Reason> }
pub struct Policy { /* schemes, trusted hosts, docs hosts, video hosts */ }
impl Policy { pub fn decide(&self, r: &Request, caps: &Caps) -> Decision; }

// egui-pop-api::links
pub const LINKS: ServiceToken<Links> = ServiceToken::new("app.links");
pub const STEP_OPEN: StepId = StepId::from_static("link.open");
pub trait Opener: Send + Sync {
    fn caps(&self) -> Caps;                       // web, files, reveal, inside
    fn open(&self, target: &Target, how: How) -> Pending<Result<(), OpenError>>;
}
pub fn open(app: &AppHandle, request: Request) -> Pending<Result<Opened, OpenError>>;
pub fn link(ui: &mut egui::Ui, app: &AppHandle, label: &str, target: &str) -> egui::Response;
pub struct LinksPlugin;  // in DefaultPlugins: the service, the step, the interceptor, `links.open`
pub struct ViewerPlugin; // opt-in: the panel and the `Inside` way
```

`ctx.open_url` stays usable: whatever emits it, the interceptor routes
it as `Source::Declared` for `http(s)` and `mailto` (what an
application's own hyperlink is) and as `Source::Content` otherwise. The
terminal moves to `links::open` with `Source::Content` and
`How::Outside` (a `file://` link becomes a confirmed open, or Reveal
from its context menu). An application's About uses `links::link`. A
plugin's `files.reveal` command is `links::open` with `How::Reveal`.

### 8. In a photo editor

- **Help:** "Documentation" in the Help menu opens the `docs:` index in
  the viewer, offline (pages shipped with the application); its
  "Online manual" opens the website outside. F1 over a control opens
  the page the control's command declares (as Blender does), a later
  step.
- **Tips with links:** a tip's text links a `docs:` page (inside) or a
  tutorial video (a card, outside, no thumbnail fetched unless shipped).
- **About:** the repository and the website open outside without a
  confirmation (declared); the licence opens inside; each has Copy link.
- **"Show in folder":** the toast after an export and the library's
  context menu run `files.reveal` (`How::Reveal`, declared): the file
  manager with the file selected, through the portal in Flatpak.
- **An agent asking to open a file:** `links.open { target, how:
  "reveal" }` raises a confirmation naming the agent and the path; the
  answer goes back to the agent; a dry run first tells it a
  confirmation will be asked.
- **A headless node receiving an open request:** its host installs
  `NullOpener`, or a forwarding opener when an interface is attached:
  the request is answered "no interface to open it in" or relayed to the
  interface, where the user confirms.

## Flows

### A link in About, on native and on the web

The user clicks "Source code". The widget calls `links::open` with
`Source::Declared`. The policy classifies a web page, `Auto` gives
outside, no confirmation. `link.open` runs, `EguiOpener` emits the
approved `OpenUrl`; on native webbrowser opens the default browser, on
the web build a new tab. `link.opened` goes to the journal.

### A tip with a tutorial video

The tip declares a video link. The card shows the title, the host and
the duration, a play icon, no image. A click opens the browser. Had the
application shipped a thumbnail, it would be shown; it never fetches
one from the video host.

### A link printed in the terminal

A program prints `https://example.org/report`. Ctrl+click asks with
`Source::Content`. The host is not trusted: a confirmation shows the
full URL with Open, Copy link, Cancel and "Trust example.org for this
session". Open: the browser opens, the terminal's `Link` signal is
still emitted.

### An agent asking to reveal an export

The agent calls `links.open` with a dry run: the answer says kind
`File`, how `Reveal`, confirmation required. It calls again without the
dry run: the dialog says "Agent <name> asks to show
`<path>` in the file manager". The user accepts; the file manager opens;
the agent receives the result. Refused: the agent receives the refusal.

### A docs page offline, with a remote image

The user opens "Documentation". The viewer shows the shipped page; an
image on another host is a placeholder with "Images from <host> are not
loaded: Load". Nothing has left the machine. Load: the image is fetched
through our loader; the choice is remembered for that host.

## Open questions, with the defaults proposed

1. **The viewer: a panel or its own window?** A panel, detachable; one
   page at a time.
2. **Markdown renderer?** egui_commonmark in the viewer, behind a
   feature; our own inline subset (emphasis, code, links) for tips,
   toasts and settings descriptions, so short texts need no extra crate.
3. **HTML inside?** The safe subset, for declared pages only.
4. **Confirmation?** Declared web and mail links: none. Content links:
   when the host is not trusted. Local files: always. Agents: always.
5. **Remote images and thumbnails?** After a click, remembered per
   host.
6. **A webview?** Not now; later an opt-in plugin, native, own window.
7. **Intercept egui's `OpenUrl`?** Yes.
8. **`file:` in content?** Confirmed; executables refused, Reveal
   offered.
9. **`command:` links?** Declared text only.
10. **Visited links?** Not tracked.
11. **Remote links alive?** An opt-in xtask.
12. **Headless?** Refuse with the reason, or forward to an attached
    interface.
13. **Web build?** New tab; `mailto:` same tab.
14. **Rate limits?** One confirmation pending per agent, five queued,
    then refused; content opens one per click.
15. **Where does the viewer live?** A `viewer` module in
    `egui-pop-views` behind a `markdown` feature (an L4 component),
    rather than a new crate.

## Steps

1. **Fix:** turn on eframe's `links` feature in `egui-pop-api`, so
   links open on native again (webbrowser, MIT OR Apache-2.0); a test
   that a hyperlink click reaches the platform output.
2. `egui-pop-model::links` (parse, classify, policy) and
   `egui-pop-api::links` (service, step, interceptor, `EguiOpener`,
   `RecordingOpener`, the confirmation, toasts on failure, the widget,
   `links.open`, `links.check`, the collector); the terminal moved onto
   it; a gallery page.
3. `SystemOpener` in `egui-pop-platform` (`platform-open`): files,
   folders, reveal, the portals; `files.reveal`.
4. The lint and its test; the inline subset for short texts.
5. The viewer: markdown, `docs:` pages, navigation, find, zoom, the
   image consent.
6. The HTML subset.
7. The webview question, if the owner asks for it.

## Sources

Read on 2026-10-28.

- egui and eframe source, 0.36.2: `egui/src/widgets/hyperlink.rs`,
  `egui-winit/src/lib.rs` (`open_url_in_browser`), eframe's
  `Cargo.toml` (`links`), <https://github.com/emilk/egui/blob/main/crates/eframe/src/web/mod.rs>
- webbrowser: <https://docs.rs/webbrowser/latest/webbrowser/>
- opener: <https://docs.rs/opener/latest/opener/>
- showfile: <https://lib.rs/crates/showfile>
- freedesktop file manager interface:
  <https://wiki.freedesktop.org/www/Specifications/file-manager-interface>
- Flatpak portals: <https://github.com/flatpak/flatpak/wiki/Portals>,
  the OpenURI interface versions
  (<https://nest-open-source.googlesource.com/manifest_repos/glib/+/HEAD/gio/org.freedesktop.portal.OpenURI.xml>),
  the writable descriptor rejection
  (<https://github.com/flatpak/libportal/pull/26>), links doing nothing
  without a backend
  (<https://forum.manjaro.org/t/link-in-flatpak-apps-wont-open-on-click-since-anymore-last-update/149907?page=2>)
- Snap's `xdg-open`:
  <https://forum.snapcraft.io/t/allowing-xdg-open-to-open-files/3789>,
  <https://forum.snapcraft.io/t/stric-confinement-application-must-open-firefox-web-browser/9789>
- egui_commonmark: <https://github.com/lampsitter/egui_commonmark>,
  <https://docs.rs/egui_commonmark/latest/egui_commonmark/>
- egui_extras loaders: <https://docs.rs/egui_extras>
- wry: <https://docs.rs/crate/wry/latest>,
  <https://github.com/tauri-apps/wry/discussions/996>
- egui_webview: <https://github.com/lucasmerlin/hello_egui>
- Servo on crates.io: <https://lwn.net/Articles/1067467/>,
  <https://en.wikipedia.org/wiki/Servo_(software)>
- Blitz: <https://github.com/DioxusLabs/blitz>
- YouTube's privacy-enhanced mode:
  <https://www.stefanjudis.com/blog/the-lie-of-youtubes-privacy-enhanced-embed-mode>,
  <https://axbom.com/embed-youtube-videos-without-cookies/>
- VS Code: link protection
  (<https://github.com/orgs/community/discussions/58344>,
  <https://docs.posit.co/ide/server-pro/user/vs-code/reference/faq.html>),
  markdown preview security
  (<https://code.visualstudio.com/docs/languages/markdown>), command
  URIs (<https://www.eliostruyf.com/command-uri-vscode-webview-open-files-links/>),
  the integrated browser
  (<https://code.visualstudio.com/docs/debugtest/integrated-browser>)
- Blender's help system:
  <https://docs.blender.org/manual/en/latest/getting_started/help.html>
- Krita's welcome screen: <https://docs.krita.org/reference_manual/welcome_screen.html>,
  the discussion that made its update check opt-in
  <https://bugs.kde.org/show_bug.cgi?id=363319>
- Obsidian's Web viewer: <https://obsidian.md/help/plugins/web-viewer>,
  <https://obsidian.md/changelog/2025-01-30-desktop-v1.8.3>
