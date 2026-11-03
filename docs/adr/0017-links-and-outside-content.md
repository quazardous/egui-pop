# 0017: Links and outside content: one service opens them, an egui-native viewer shows what is safe, the rest goes outside

Date: 2026-10-28. Status: **accepted** (2026-10-28), revised on 2026-11-01
for local video ([below](#local-video-inside-revised-2026-11-01)), amended on
2026-11-03 by a YouTube trial behind a feature
([below](#youtube-inside-a-trial-amended-2026-11-03)).

The analysis behind it (what exists today, what egui and eframe do with
a link, a survey of openers, markdown and HTML renderers, webviews and
five applications, the questions answered, the flows, the sources) is
in the [appendix](0017-links-and-outside-content-analysis.md). Where
they differ, this page wins.

## Context

Links are about to appear everywhere: an About dialog with the
repository, tips and help pages with links in their text, a "Watch the
tutorial" pointing at a video, toasts saying "Exported: Show in
folder", links found in a terminal's output, and agents asking the
application to open something. Today there is no rule for any of it:

- `ctx.open_url` is the only way out, and on native it **does nothing**:
  egui-pop builds eframe without its `links` feature, so egui-winit only
  logs "Cannot open url" (the terminal's Ctrl+click and the gallery's
  hyperlink are silent no-ops, against ADR 0003). On the web build it
  calls `window.open` in a new tab or the same tab.
- egui's `Hyperlink` opens on a plain click, without showing where it
  goes (`url_in_tooltip` is off), without a context menu, without a
  confirmation, whatever the scheme.
- Nothing can render markdown or HTML, fetch an image only once the
  user agrees, reveal a file in the file manager, or open a local file.
- Nothing can observe, veto or record an open request; an agent has no
  verb for it; tests cannot see that a click asked for a URL.

A photo editor (Helios) needs help pages that work offline, links in
tips and About, "Show in folder" for images and exports, agents asking
to open a file, and a headless node that receives such a request with
no screen at all.

## Decision

**One `links` service, one step, a replaceable opener.** Every open
goes through `links::open(app, request)` and the `link.open` step
(ADR 0011): before-hooks veto or redirect, a run-hook replaces, the
journal records `link.opened` or `link.failed`. The request names the
**target**, **how** (`Auto`, `Outside`, `Inside`, `Reveal`), its
**source** (declared by the application, found in content, asked by an
agent) and its **origin** (`Origin::User`, `Origin::Agent(..)`). The
pure parts (parsing, classifying the target, the policy that decides
how and whether to confirm) live in `egui-pop-model::links`; the
service, the widget and the confirmation in `egui-pop-api::links`. The
actual opening is an `Opener` the host can replace (ADR 0010):

- `EguiOpener` (default, every build): web pages and `mailto:` through
  egui's `OpenUrl` (eframe's `links` feature turned on), a new tab on
  the web build;
- `SystemOpener` (`egui-pop-platform`, feature `platform-open`): adds
  local files, folders and **reveal**, through the desktop portal inside
  Flatpak and Snap, `xdg-open` and the file manager's D-Bus interface
  elsewhere on Linux, `open` and `open -R` on macOS, `ShellExecute` and
  Explorer's selection on Windows;
- `RecordingOpener` (tests): records each request and answers what the
  test scripted;
- `NullOpener` (headless): refuses with the reason, or forwards to an
  attached interface when the host offers one.

egui's own `OutputCommand::OpenUrl` (from `ui.hyperlink`, third-party
widgets, the markdown renderer) is **taken out of the frame's output**
by the service and routed through the same step, so no link bypasses
the policy.

**What a click does, per kind of target.** The policy classifies the
target and picks the default (`Auto`):

| Target | Plain click | Ctrl+click, middle click |
|---|---|---|
| Web page (`https:`, `http:`) | the system browser; a new tab on the web build | the same |
| Video on the web (YouTube, Vimeo, a remote video file) | the system browser; a YouTube video in the viewer's web view with the opt-in `webview` feature on Linux ([a trial](#youtube-inside-a-trial-amended-2026-11-03)) | the same |
| Local video file | the viewer when it has a video player (the opt-in `video` feature), else as a local file below | outside |
| Markdown or HTML page shipped by the application (`docs:` pages) | the in-app viewer when its plugin is present, else the online copy in the browser | outside |
| Remote markdown or HTML | the browser, unless its host is one the application declares as its documentation | outside |
| Image | local: the viewer, else the system's default application; remote: the browser | outside |
| Local file | the system's default application, after a confirmation; executables refused, Reveal offered instead | Reveal |
| Local folder | the file manager | the same |
| `mailto:` | the mail client | the same |
| The application's own scheme (`<app>://`) | `deeplink.open`, inside the application, no round trip through the system | the same |
| `command:` | runs the command, only in text the application declared | nothing |
| Anything else (`javascript:`, `data:`, `file:` to a device, unknown schemes) | blocked, with the reason | blocked |

**The link widget.** `links::link(ui, app, label, target)` draws a link
in the theme's link colour: a pointing hand, the real target on hover (a
tooltip with the full URL, the host emphasised, an internationalised
host shown in its punycode too), a context menu (**Open**, **Open
outside** when the default is inside, **Reveal in folder** for files,
**Copy link**), Enter and Space activate it, role `link` with the label
as its name and the target as its description. A link whose text looks
like a URL but differs from its target is always confirmed, both shown.
Short texts (tips, toasts, settings descriptions) accept an inline
subset (emphasis, code, links) drawn with this widget.

**An egui-native viewer, opt-in.** `ViewerPlugin` brings a panel (the
workbench can detach it into its own window) that shows markdown, a
safe subset of HTML and images, with **back and forward**, an
**address bar** (editable, showing the real target), its session
**history**, **find in page** (Ctrl+F), **zoom** (the interface zoom
keys, scoped to the page), **Open outside** and **Copy link**. Markdown
renders through egui_commonmark (MIT OR Apache-2.0, wrapped behind a
feature); HTML is parsed by html5ever, cleaned by ammonia's allow-list
(no scripts, no styles, no forms, no iframes) and mapped to the same
blocks as markdown; images go through egui's loaders. It works on
native and on the web build alike. What needs a real web engine
(YouTube, full HTML, JavaScript) opens outside. A local video file plays
inside when the opt-in video page is built in, without a web engine.
**No webview now**: wry
needs GTK and X11 for a child view on Linux and the egui integration is
experimental; Servo and Blitz bring MPL code that `deny.toml` refuses.
An opt-in webview plugin stays an open question.

**Trust and privacy.**

- Allowed schemes: `https`, `http`, `mailto`, `file`, the application's
  own scheme, `docs`, and `command` from declared text. The host may
  add or remove schemes; everything else is blocked.
- Confirmation (a `Confirm` dialog showing the full target, with Open,
  Copy link, Cancel, and "Trust <host> for this session"): never for a
  declared web or mail link; for a link from content to a host not yet
  trusted; **always** for an agent's request (the dialog names the
  agent) and for opening a local file. Reveal of a declared path is
  not confirmed.
- No request to a third party before the user asks: remote images in
  the viewer, video thumbnails and link previews load only after a
  click on "Load images from <host>" (remembered per host in the
  settings). A video link shows as a card (title, host, duration when
  declared) with no image fetched.
- Opens need a user gesture or a confirmation: content cannot open
  anything on its own; an agent has one pending confirmation per
  session, further requests are queued (at most five) then refused.

**Offline and failures.** An outside open that fails (no browser, no
mail client, the portal refused, the file is gone) ends in a warning
toast with the reason and a **Copy link** action; a blocked scheme says
which. The viewer shows an error page in place (no network, not found,
too large, a type it cannot render) with **Retry**, **Open outside** and
**Copy link**. A dead link opened outside is the browser's to report.

**Checking.** A link lint (`links::lint`, run by a test in the gate)
walks the links the application declares (menus, About, tips, toasts'
actions, `docs:` pages and their own links): each must parse, use an
allowed scheme, name an existing command or `docs:` page or bundled
file. Checking that remote links are alive is a separate, opt-in xtask,
never in the gate. Tests install `RecordingOpener` and click: the
record holds the target, how, source and origin. Agents get
`links.open` (`target`, `how`, `dry_run`: a dry run answers the kind,
how it would open, whether it needs a confirmation and the policy's
verdict, opening nothing) and `links.check`, plus a `links` collector
on the debug bridge (the recent requests and their outcome).

**Survey.** egui's `Hyperlink`, `OpenUrl` and `clicked_with_open_in_background`
are **kept** and intercepted. webbrowser (through eframe's `links`) and
opener (with `reveal`) are **adopted** behind `EguiOpener` and
`SystemOpener`; ashpd (already in the tree through rfd) is **wrapped**
for the portals. egui_commonmark is **wrapped** in the viewer; html5ever
and ammonia are **adopted** for the HTML subset, its mapping to blocks
**owned**. The policy, the widget, the viewer's navigation and the
confirmation are **owned**: no crate has them. wry and egui_webview
**wait**; Servo, Blitz and CEF are **rejected** (licence, size).

## Consequences

- Every link goes through one policy and one step: observable,
  vetoable, testable, the same for the user, a widget, the terminal and
  an agent.
- Links work on native again (today a silent no-op), and on the web
  build they never replace the application's tab.
- Help pages and shipped HTML render offline, on native and the web,
  with no web engine in the binary.
- No third-party request happens before the user asks.
- Remote videos and full web pages are not shown inside; that is a choice,
  revisited only with the webview question or a trial of an embedded player
  (no promise either way). Local videos play inside with the `video` feature.
- The terminal's Ctrl+click moves onto `links::open` (source: content);
  the `files.reveal` command the toasts' example names becomes real.

## Open questions

Each with the default this ADR proposes (the analysis gives the
reasons):

1. **The viewer: a panel or its own window?** A panel in the end side
   bar, detachable through the workbench; one page at a time, no tabs.
2. **Markdown: egui_commonmark or our own renderer?** egui_commonmark in
   the viewer (behind a feature); our own inline subset for short texts.
3. **HTML: render a safe subset inside?** Yes for pages the
   application ships or declares; remote HTML opens outside.
4. **Confirm before leaving the application?** Not for declared web
   and mail links; yes for content links to an untrusted host, for
   local files, and always for agents.
5. **Remote images and video thumbnails?** Never before a click, then
   remembered per host.
6. **A webview?** Not now; later an opt-in plugin, native only, in its
   own window. The web build opens a new tab.
7. **Take egui's `OpenUrl` out of every frame?** Yes, so every link
   meets the policy.
8. **`file:` links in content?** Confirmed, executables refused with
   Reveal offered.
9. **`command:` links?** Only in text the application declares.
10. **Visited links?** No state kept, beyond the viewer's session
    history.
11. **Checking remote links alive?** An opt-in xtask, never in the gate.
12. **An open request on a headless node?** Refused with the reason, or
    forwarded to an attached interface when the host offers one.
13. **Web build: new tab or same tab?** Always a new tab, except
    `mailto:` (same tab, which hands it to the mail client).
14. **Rate limits?** One pending confirmation per agent session, five
    queued, then refused; content opens only on a click.

## Answers to the open questions (2026-10-28)

The owner accepted the proposed defaults above, with the analysis's
fifteenth: the viewer is a `viewer` module in `egui-pop-views` behind a
`markdown` feature rather than a new crate. Steps 1 to 4 of the
analysis come first; the viewer (step 5) follows.

## Done ahead of the steps

Nothing yet. Step 1 of the analysis is the fix found while reading the
code: links do nothing on native because eframe's `links` feature is
off.

## Built (2026-10-28)

Steps 1 to 4 of the analysis: eframe's `links` feature on; the model
in `egui-pop-model::links` (targets, requests, the policy, the lint's
check); the service in `egui-pop-api::links` (`LinksPlugin`, in
`DefaultPlugins`: the `link.open` step, `EguiOpener`,
`RecordingOpener`, `NullOpener`, the interceptor of egui's `OpenUrl`,
the confirmation with a per-session trust, the agents' rate limits,
warning toasts with Copy link, `links.open`, `links.check`,
`files.reveal`, the `links` collector, the widget and the lint);
`SystemOpener` in `egui-pop-platform` (feature `open`, through the
opener crate, whose Reveal falls back on the OpenURI portal; inside
Flatpak `xdg-open` is the portal's, so ashpd is not needed for it); the
terminal's Ctrl+click on the service; a gallery page. The guide is
[docs/links.md](../links.md). Left: the viewer and the inline subset
for short texts (step 5, built below), the HTML subset (step 6), an
opt-in online check of declared links. The gallery's built-in widgets page keeps
egui's own `Hyperlink`, which it shows as one of egui's widgets: the
interceptor routes it through the policy.

Step 5 (2026-10-28): the viewer, `viewer` in `egui-pop-views` behind
the `markdown` feature (egui_commonmark 0.25, the release on egui
0.36): `ViewerPlugin`, a closed panel of the end side bar, installs
itself in the links service as its `Inside` way, so `docs:` pages
(from the `Docs` the application ships), documentation hosts' markdown
and local images open there; back, forward, reload, the address bar,
find in page, the page's zoom, anchors, the error page, remote images
behind a per-host click remembered in the `viewer.image_hosts`
setting, `viewer.state` and the `viewer` collector for agents. A page
is drawn block by block (its top-level blocks, each through
egui_commonmark) so find in page and anchors know where each block
is; links in a page are taken through egui_commonmark's link hooks
and opened through the links service. Remote pages and images need a
`Fetch` the host gives: egui-pop adds no HTTP client. The inline
subset for short texts (`links::Inline`) is owned, without a markdown
crate, and drawn in tips, toasts and settings descriptions. Left: the
HTML subset (step 6, its pages show an error page with Open outside),
the lint walking the links inside `docs:` pages, an opt-in online check
of declared links.

Step 6 (2026-10-30): the HTML subset, feature `html` of `egui-pop-views`
(the facade's `views-html`). A page is parsed by html5ever (MIT OR
Apache-2.0) and mapped to the viewer's blocks through markdown, as
decided. **ammonia is replaced (decided by the owner, 2026-10-30)**: every
release of it since 4.1 depends on cssparser, cssparser-macros and
dtoa-short (MPL-2.0, not optional), which `deny.toml` refuses, and the
4.0 line, the last without them, fails its advisories check
(RUSTSEC-2021-0157, RUSTSEC-2021-0158, RUSTSEC-2026-0373). The allow-list
is owned instead, in the mapping: only `href`, `src`, `alt`, a heading's
`id` and a list's `start` are read, addresses are checked against the
policy's schemes, scripts, forms, frames and media go with their content
and are named on the page, text is escaped, and no HTML is written back
for anything to read again, so a sanitiser's reason to exist (markup
handed to a browser) does not arise. This replaces what the decision and
the survey above say of ammonia. Left: the lint walking the
links inside `docs:` pages, an opt-in online check of declared links.

What a page in the viewer reaches (2026-10-30): one check for markdown and
HTML. A link keeps its markup when what it resolves to uses a scheme the
page may use, and stays as text otherwise, so a page from the network has
no `file:` link to confirm; the confirmation of question 8 stays for a
page or an application that may link to a file. An image reads a scheme the
page may use and, from the network, never `docs:`, though a link there
stays; what is refused is named on the page. Nesting past 512 levels
is refused while the page is parsed, on an error page: parsing stays on
the interface's thread, as it must on the web build, and is quadratic in
the depth. A limit needs no second path through the viewer; parsing on a
thread would need one on native alone, and the limit on the web all the
same.

## Local video inside (revised 2026-11-01)

This revises "video opens outside" for **local files only**, as the owner
decided on 2026-11-01. With the opt-in `video` feature of `egui-pop-views` (the
facade's `views-video`), a local video file plays in the viewer: play and pause,
seek, volume, the time, with sound. Without the feature, on the web build, or
with a host that gives no player, it opens outside as any local file, after the
same confirmation.

- **The policy** classifies a local file with a video extension as `Kind::Video`
  and sends it inside when `Caps::video` is set, which the links service reads
  from the viewer (`Inside::plays_video`). It is confirmed as a local image is:
  not inside, except for an agent; always outside.
- **Remote video and YouTube stay outside.** Whether an embedded player is worth
  a trial is an **open question**: nothing here promises one, and a trial decides
  nothing until the owner has read its result.
- **GStreamer** decodes (`gstreamer`, `gstreamer-app`, `gstreamer-video`, MIT OR
  Apache-2.0). Its libraries are LGPL and installed on the system, not crates:
  no LGPL code enters the crate tree, and `deny.toml` is unchanged. The survey is
  in the [appendix](0017-links-and-outside-content-analysis.md#playing-a-local-video).
  The player sits behind `VideoBackend`, which a host may replace.
- **Default builds and the web build do not need GStreamer.** The crates are
  optional and never built for `wasm32`.
- **A machine without GStreamer's libraries does not start a binary built with
  the feature**: the bindings link them. An application offering the feature
  declares them as a package dependency. What is missing at run time beyond that
  (the `playbin` plugin, a demuxer, a decoder) and what cannot be read (a bad
  file, a refused permission) ends on the viewer's error page with **Open
  outside**.

## YouTube inside: a trial (amended 2026-11-03)

A trial of an embedded player, behind an opt-in feature, off by default. It
decides nothing about a general web view; whether it stays is the owner's call.

- **What.** With the `webview` feature of `egui-pop-views` (the facade's
  `views-webview`), on Linux, a YouTube link (`watch?v=`, `youtu.be/`, `embed/`,
  `shorts/`, `live/`, over `https:`) plays in the viewer, in a web view: wry
  (MIT OR Apache-2.0) over the system's WebKitGTK. The policy sends a remote
  video inside only when the viewer says it plays it (`Caps::web_video`,
  `Inside::plays_web_video`) and the link names a YouTube video; everything
  else (other hosts, Ctrl+click, no feature, another system, the web build)
  opens outside as before. A link from content is still confirmed for an
  untrusted host, as outside: the web view reaches YouTube as the browser
  would.
- **What loads.** The privacy-enhanced player
  (`youtube-nocookie.com/embed/<id>`) as the page itself, its request carrying
  a `Referer` (`ViewerPlugin::embed_referrer`, `https://localhost/` by
  default) through wry's `with_url_and_headers`: without one YouTube's player
  refuses to play (its error 153). Every other navigation is refused in the web
  view and handed to the links service, outside.
- **Where.** On X11 (Xwayland included) a child window over the panel, following
  it; on Wayland, or with the panel detached, a window of its own: wry's child
  web views need X11. GTK starts on the interface's thread and its loop is
  turned once per pass, without blocking, while a web view is open.
- **Costs.** WebKitGTK must be installed to build and to run (the binary links
  it). A child web view floats above egui: egui's popups over the panel go
  under it. On Linux wry brings `option-ext` (through `dirs`), MPL-2.0, which
  `deny.toml` refuses: `cargo deny check` passes with default features and
  fails with `--all-features`; the Android-only part of wry adds `dom_query`,
  `selectors`, `cssparser`, `cssparser-macros` and `dtoa-short`, MPL-2.0 too.
  `deny.toml` is unchanged. **Open question for the owner:** accept that crate
  behind the feature, find a way around it, or drop the trial.
- **What was checked**, in a headless nested compositor: the trial's
  [results](0017-links-and-outside-content-analysis.md#youtube-in-a-web-view-the-trial).
