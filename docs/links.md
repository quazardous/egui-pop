# Links

Every link in an egui-pop application opens the same way: through one
service, one policy and one step, with an opener the host may replace.
The decision and its reasons are [ADR 0017](adr/0017-links-and-outside-content.md);
this page is how to use it. Everything below is
`egui_pop::api::links` (the service, the widget, the openers, the
plugin) and `egui_pop::model::links` (targets, requests, the policy,
no egui). `LinksPlugin` is in `DefaultPlugins`.

## Drawing a link

```rust,ignore
use egui_pop::api::links::{self, Link};

// A link the application declares (About, help, a tip).
links::link(ui, app, "Source code", "https://github.com/me/app");
// A link found in content (a document, a program's output).
Link::content("the report", "https://example.org/report").show(ui, app);
```

The widget draws in the theme's link colour with a pointing hand, and:

- shows the real target on hover: the full address, then its host (an
  internationalised host with its punycode beside it,
  `bücher.example (xn--bcher-kva.example)`);
- opens on a click, Enter or Space; Ctrl+click (Cmd+click on macOS) or a
  middle click opens it outside;
- has a context menu: **Open**, **Open outside** (when it would open
  inside), **Show in folder** (a local file or folder), **Copy link**
  (through the clipboard: a `text/uri-list` item with its text);
- is a `link` for screen readers, its label as its name and its target
  as its description.

A link whose text looks like an address (`https://bank.example`,
`bank.example/login`) but goes elsewhere is always confirmed, both
shown.

`ui.hyperlink`, `ui.hyperlink_to`, a third-party widget or
`ctx.open_url` still work: their `OpenUrl` is taken out of each frame's
output and opened through the same service on the next pass, as a
declared link for `http(s):` and `mailto:`, as content otherwise.
Nothing reaches the browser without the policy.

## What a click does

The policy classifies the target from its scheme, host and extension
(never by fetching it) and picks the way:

| Target | A click | Ctrl+click, middle click |
|---|---|---|
| Web page (`https:`, `http:`) | the system browser; a new tab on the web build | the same |
| Video (YouTube, Vimeo, a remote video file) | the browser, never embedded | the same |
| `mailto:` | the mail client (the same tab on the web, which hands it over) | the same |
| `docs:` page | the viewer when there is one, else its online copy (`Policy::docs_online`) | outside |
| Local file | its default application, after a confirmation; a program refused, Show in folder offered | Show in folder |
| Local folder | the file manager | the same |
| The application's own scheme (`<app>://`) | `deeplink.open`, inside | the same |
| `command:<id>?<json>` | runs the command, from declared text only | nothing |
| Anything else (`javascript:`, `data:`, a device, an unknown scheme) | blocked, with the reason | blocked |

## Trust

Every request says where it comes from: **declared** by the
application, found in **content**, or asked by an **agent**.

| Source | Web and mail | Local file | Show in folder | Command |
|---|---|---|---|---|
| Declared | opens | confirmed | opens | runs |
| Content | opens when its host is trusted, else confirmed | confirmed | confirmed | refused |
| Agent | always confirmed | always confirmed | always confirmed | refused |

The confirmation shows the full target, its host (and punycode), why it
asks, **Copy link**, **Open** and **Cancel**; for a content link to an
untrusted host, **Trust <host> for this session** (the host and its
subdomains, until the application quits). Only the user answers it: an
agent answering through the debug bridge is refused (it may cancel).

- Content opens only on a click: a content request not made by the
  user is refused.
- A program or a script (by its extension, or the executable bit of a
  file without one) is never opened; Show in folder is offered instead.
- An agent has one confirmation open at a time; five more requests wait
  behind it, then the next is refused with the reason.
- Allowed schemes: `https`, `http`, `mailto`, `file`, `docs`, `command`
  and the application's own (the single-instance plugin adds its
  schemes). The host changes the policy through the service:

```rust,ignore
use egui_pop::api::links::{LINKS, LinksPlugin, Policy};

App::builder().plugin(
    LinksPlugin::default().policy(
        Policy::default()
            .app_scheme("helios")
            .docs_host("docs.example.org"),
    ),
);
// Later: app.service(&LINKS).unwrap().update_policy(|p| { p.schemes.remove("http"); });
```

## Openers

The opening itself is an `Opener` the host replaces (ADR 0010):

- `EguiOpener` (the default): web pages and mail through egui's
  `OpenUrl`; on native eframe's `links` feature (webbrowser) opens the
  system browser, on the web a new tab (`mailto:` in the same tab).
- `SystemOpener` (`egui_pop::platform::open`, feature `platform-open`;
  install it with `OpenPlugin`): local files and folders with the
  system's default application, and Show in folder (the file manager
  with the file selected: the freedesktop file manager interface, else
  the OpenURI portal; `open -R`; Explorer). Inside Flatpak, `xdg-open`
  is the portal's. Not on the web.
- `RecordingOpener` (tests): records each call (target, how, kind,
  source, origin) and answers as scripted.
- `NullOpener` (a node without a screen): refuses with the reason, or
  forwards each request to an attached interface (`NullOpener::forward`).

```rust,ignore
let rec = RecordingOpener::default();
app.service(&LINKS).unwrap().set_opener(Arc::new(rec.clone()));
harness.get_by_role_and_label(Role::Link, "Source code").click();
harness.run();
assert_eq!(rec.records()[0].target, "https://github.com/me/app");
```

## The step and the journal

`links::open(app, request)` runs the `link.open` step (ADR 0011): a
before-hook vetoes it or redirects it (another target, another way), a
run-hook opens it its own way, an after-hook sees the result. Its
result says the kind, the way, whether it asked, and the status:
`opened`, `opening` (an opener answering later), `asked` or `queued`
(waiting for the user). The journal says `link.opened`, `link.failed`
(with the reason), `link.refused`, `link.asked` and `link.cancelled`.

A refusal or a failure the user asked for shows in a warning toast with
the reason and **Copy link** (**Show in folder** for a program).

## Commands and agents

| Command | What |
|---|---|
| `links.open` | `target`, `how` (`auto`, `outside`, `inside`, `reveal`), `label`, `dry_run`: opens a link; from an agent, always confirmed; a dry run says the kind, the way, whether it would ask and the verdict, opening nothing |
| `links.check` | `target`, `how`, `source`: the policy's decision and the opener's capabilities, without the step |
| `files.reveal` | `path`: shows a file in its folder |
| `links.copy` | `target`: copies a link (a toast's action) |

The debug bridge's `links` collector shows the opener and what it can
do, the policy, the requests waiting for an answer (and queued per
agent), the recent requests with their decision and outcome, and the
links declared.

## The lint

A plugin declares the links it shows (`ctx.link(place, target)`, or
`links::declare` from the application); links drawn with the widget as
declared count too. `links::lint::problems(app)` checks them, with the
menu items running `links.open` or `files.reveal`, the links in the
tips' texts and the toasts' actions: each must parse, use an allowed
scheme, name a declared command (`command:`) and a file that is there.
In a test, after drawing the screens:

```rust,ignore
egui_pop_test::assert_links_clean(&app);
```

The gallery's links pass it. Checking that remote links answer is not
part of it (an opt-in task, never in the gate).

## Not yet

- The viewer inside the application (markdown, `docs:` pages, a safe
  HTML subset, images, back and forward, find, zoom) is the next step
  of ADR 0017; until then `docs:` links open their online copy, or are
  refused with the reason.
- The inline subset for short texts (emphasis, code, links in tips and
  toasts) comes with it.
- No webview: full web pages and videos open outside.
