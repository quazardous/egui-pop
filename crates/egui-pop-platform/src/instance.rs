//! A single instance, and deep links.
//!
//! [`single`], called at the top of `main`, before any window: the first
//! launch listens on a local socket named after the application (an
//! abstract socket on Linux, a named pipe on Windows, a socket in `/tmp`
//! elsewhere) and gets [`Single::First`]; a later launch hands its
//! arguments, its working directory and its activation token to the
//! running one and gets [`Single::Handed`]: it exits.
//!
//! [`InstancePlugin`] takes what the first launch holds into the
//! application. Its steps (ADR 0011):
//!
//! - `instance.second_launch`: a later launch handed over, with
//!   `{ "args", "cwd", "links" }` (`links`: the arguments in one of the
//!   plugin's [`schemes`](InstancePlugin::scheme)). By default the main
//!   window comes to the front ([`crate::window::raise`]) and each link
//!   runs `deeplink.open`. Veto it to stay in the background; replace it
//!   to open the files a second launch names.
//! - `deeplink.open`: a link to open, `{ "url" }`, from a later launch or
//!   from the first launch's own arguments (at start). By default the
//!   plugin's [`links`](InstancePlugin::links) command runs with
//!   `{ "url" }`; without one the step fails, and says so.
//!
//! The launches handed over are kept ([`LAUNCHES`], [`Launches::history`]);
//! [`Launches::deliver`] hands one over in-process, as a test or the
//! gallery does. Registering the scheme with the system is the
//! packager's part: see `docs/packaging.md`.
//!
//! On Linux a connection from another user is refused (the abstract
//! socket namespace is shared by the users of a machine). Inside a
//! Flatpak sandbox the socket is a file in the directory the
//! application's sandboxes share ([`flatpak_socket`]). Elsewhere the
//! socket or pipe is the user's own by its place (`/tmp` with the user's
//! name in the socket's, a pipe in the user's session).
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_platform::instance::{self, InstancePlugin, Launch};
//!
//! let launch = Launch { args: vec!["demo://open/7".into()], ..Launch::default() };
//! let first = instance::Instance::detached(launch);
//! let ctx = egui::Context::default();
//! let mut app = App::builder()
//!     .plugin(InstancePlugin::new(first).scheme("demo"))
//!     .build(&ctx)
//!     .unwrap();
//! app.logic(&ctx);
//! // No command opens links: the step says so.
//! let trace = app.handle().step_traces().into_iter().rev()
//!     .find(|t| t.step.as_str() == "deeplink.open").unwrap();
//! assert_eq!(trace.args["url"], "demo://open/7");
//! ```

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use egui_pop_api::hooks::On;
use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::StepSpec;
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.instance");
/// A later launch handed over: `{ "args", "cwd", "links" }`.
pub const STEP_SECOND_LAUNCH: StepId = StepId::from_static("instance.second_launch");
/// A link to open: `{ "url" }`.
pub const STEP_DEEPLINK: StepId = StepId::from_static("deeplink.open");
/// Opens a link of the application's own scheme (`url`), through
/// [`STEP_DEEPLINK`]: what the links service runs for a click on one.
pub const CMD_DEEPLINK: CommandId = CommandId::from_static("deeplink.open");
/// The launches handed over, as a service.
pub const LAUNCHES: ServiceToken<Launches> = ServiceToken::new("platform.instance");
/// How many launches [`Launches::history`] keeps.
pub const HISTORY: usize = 32;

/// What a launch says: its arguments (without the program), its working
/// directory, and the activation token the desktop gave it
/// (`XDG_ACTIVATION_TOKEN`, `DESKTOP_STARTUP_ID`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub activation: Option<String>,
}

impl Launch {
    /// This process's launch.
    pub fn current() -> Launch {
        Launch {
            args: std::env::args().skip(1).collect(),
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            activation: std::env::var("XDG_ACTIVATION_TOKEN")
                .or_else(|_| std::env::var("DESKTOP_STARTUP_ID"))
                .ok(),
        }
    }

    /// Its arguments that are links in one of `schemes` (`myapp:` or
    /// `myapp://`, whatever the case of the scheme).
    pub fn links(&self, schemes: &[String]) -> Vec<String> {
        self.args
            .iter()
            .filter(|a| {
                a.split_once(':')
                    .is_some_and(|(s, _)| schemes.iter().any(|w| w.eq_ignore_ascii_case(s)))
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default)]
struct State {
    pending: VecDeque<Launch>,
    history: VecDeque<Launch>,
    waker: Option<egui::Context>,
}

/// The launches handed over to this one: shared with the thread that
/// listens, taken at the next pass of the application.
#[derive(Clone, Debug, Default)]
pub struct Launches(Arc<Mutex<State>>);

impl Launches {
    /// Hands a launch over, as a later launch does through the socket.
    pub fn deliver(&self, launch: Launch) {
        let waker = {
            let mut s = lock(&self.0);
            s.pending.push_back(launch.clone());
            s.history.push_back(launch);
            while s.history.len() > HISTORY {
                s.history.pop_front();
            }
            s.waker.clone()
        };
        if let Some(ctx) = waker {
            ctx.request_repaint();
        }
    }

    /// The launches handed over, oldest first.
    pub fn history(&self) -> Vec<Launch> {
        lock(&self.0).history.iter().cloned().collect()
    }

    fn take(&self) -> Vec<Launch> {
        lock(&self.0).pending.drain(..).collect()
    }

    fn set_waker(&self, ctx: Option<egui::Context>) {
        lock(&self.0).waker = ctx;
    }
}

/// The first launch: what it was launched with, and the launches the
/// later ones hand over.
#[derive(Debug)]
pub struct Instance {
    first: Launch,
    launches: Launches,
    name: Option<String>,
}

impl Instance {
    /// An instance listening to nothing: the launches come through
    /// [`Launches::deliver`] (tests, the gallery, an application with
    /// its own way of being single).
    pub fn detached(first: Launch) -> Instance {
        Instance {
            first,
            launches: Launches::default(),
            name: None,
        }
    }

    /// What this launch was launched with.
    pub fn first(&self) -> &Launch {
        &self.first
    }

    /// The launches handed over.
    pub fn launches(&self) -> Launches {
        self.launches.clone()
    }

    /// The socket's name, when it listens.
    pub fn socket(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// What [`single`] found.
#[derive(Debug)]
pub enum Single {
    /// This is the first launch: go on, with the instance.
    First(Instance),
    /// Another instance runs and took this launch's arguments: exit.
    Handed,
}

/// The socket's name for an application: its id and the user's name,
/// kept to what every platform accepts.
pub fn socket_name(app_id: &str) -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    format!("{}.{}.instance", clean(app_id), clean(&user))
}

/// Inside a Flatpak sandbox, the socket's file in the directory every
/// instance of the application shares (`$XDG_RUNTIME_DIR/app/<id>`):
/// each sandbox may have a network namespace of its own, where abstract
/// sockets are not seen.
pub fn flatpak_socket(name: &str) -> Option<std::path::PathBuf> {
    flatpak_socket_with(name, |v| std::env::var(v).ok())
}

/// [`flatpak_socket`] from the environment `env` gives.
pub fn flatpak_socket_with(
    name: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Option<std::path::PathBuf> {
    let id = env("FLATPAK_ID").filter(|i| !i.is_empty())?;
    let run = env("XDG_RUNTIME_DIR").filter(|r| !r.is_empty())?;
    Some(
        std::path::Path::new(&run)
            .join("app")
            .join(id)
            .join(format!("{name}.sock")),
    )
}

/// Makes this launch the application's only instance, or hands it to the
/// one running (see the module). `app_id`: a reverse-DNS id
/// (`org.example.App`).
pub fn single(app_id: &str) -> io::Result<Single> {
    single_with(app_id, Launch::current())
}

/// [`single`] with a launch of the caller's.
#[cfg(not(target_arch = "wasm32"))]
pub fn single_with(app_id: &str, launch: Launch) -> io::Result<Single> {
    use interprocess::local_socket::{
        GenericFilePath, GenericNamespaced, ListenerOptions, prelude::*,
    };

    let name = socket_name(app_id);
    let in_sandbox = flatpak_socket(&name);
    let ns = || match &in_sandbox {
        Some(path) => path.as_path().to_fs_name::<GenericFilePath>(),
        None => name.as_str().to_ns_name::<GenericNamespaced>(),
    };
    if hand_over(ns()?, &launch).is_ok() {
        return Ok(Single::Handed);
    }
    let listener = match ListenerOptions::new()
        .name(ns()?)
        .try_overwrite(true)
        .max_spin_time(std::time::Duration::from_millis(500))
        .create_sync()
    {
        Ok(l) => l,
        // Another first launch won the race: hand over to it.
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            hand_over(ns()?, &launch)?;
            return Ok(Single::Handed);
        }
        Err(e) => return Err(e),
    };
    let launches = Launches::default();
    let to = launches.clone();
    std::thread::Builder::new()
        .name("egui-pop instance".into())
        .spawn(move || {
            for conn in listener.incoming() {
                match conn {
                    Ok(stream) => {
                        if let Err(e) = receive(stream, &to) {
                            log::warn!(target: "egui_pop", "a second launch: {e}");
                        }
                    }
                    Err(e) => log::warn!(target: "egui_pop", "instance socket: {e}"),
                }
            }
        })?;
    Ok(Single::First(Instance {
        first: launch,
        launches,
        name: Some(match in_sandbox {
            Some(p) => p.to_string_lossy().into_owned(),
            None => name,
        }),
    }))
}

/// [`single`] on the web: always the first.
#[cfg(target_arch = "wasm32")]
pub fn single_with(_: &str, launch: Launch) -> io::Result<Single> {
    Ok(Single::First(Instance::detached(launch)))
}

#[cfg(not(target_arch = "wasm32"))]
fn hand_over(name: interprocess::local_socket::Name<'_>, launch: &Launch) -> io::Result<()> {
    use interprocess::local_socket::prelude::*;
    use std::io::{BufRead as _, BufReader, Write as _};

    let mut stream = LocalSocketStream::connect(name)?;
    let mut line = serde_json::to_string(launch).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "the running instance answered `{}`",
            answer.trim()
        )))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn receive(stream: interprocess::local_socket::Stream, to: &Launches) -> io::Result<()> {
    use interprocess::local_socket::prelude::*;
    use std::io::{BufRead as _, BufReader, Write as _};

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt as _;
        let own = std::fs::metadata("/proc/self")?.uid();
        let peer = stream.peer_creds()?.euid();
        if peer.is_none_or(|uid| u64::from(uid) != u64::from(own)) {
            return Err(io::Error::other("a launch from another user, refused"));
        }
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let launch: Launch = serde_json::from_str(line.trim()).map_err(io::Error::other)?;
    to.deliver(launch);
    reader.get_mut().write_all(b"ok\n")?;
    Ok(())
}

/// The single instance in the application (see the module).
#[derive(Debug)]
pub struct InstancePlugin {
    instance: Instance,
    schemes: Vec<String>,
    links: Option<CommandId>,
}

impl InstancePlugin {
    pub fn new(instance: Instance) -> Self {
        Self {
            instance,
            schemes: Vec::new(),
            links: None,
        }
    }

    /// A URL scheme whose links the application opens (`myapp`, for
    /// `myapp://...`).
    pub fn scheme(mut self, scheme: &str) -> Self {
        self.schemes.push(scheme.trim_end_matches(':').into());
        self
    }

    /// The command `deeplink.open` runs by default, with `{ "url" }`.
    pub fn links(mut self, command: CommandId) -> Self {
        self.links = Some(command);
        self
    }
}

/// Runs `deeplink.open` for a link.
fn open_link(app: &AppHandle, links: Option<&CommandId>, url: &str) {
    let links = links.cloned();
    app.step(&STEP_DEEPLINK, json!({ "url": url }), move |app, args| {
        let Some(command) = links else {
            return Err("no command opens links (InstancePlugin::links)".into());
        };
        app.run(command.clone(), json!({ "url": args["url"] }));
        Ok(json!({ "command": command }))
    });
}

impl Plugin for InstancePlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Single instance".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.step(
            StepSpec::new(
                STEP_SECOND_LAUNCH,
                "A later launch handed its arguments over (`args`, `cwd`, `links`); by default the window comes to the front and each link is opened.",
            )
            .args(json!({ "args": ["--open", "photo.jpg"], "cwd": "/home/me", "links": [] })),
        )?
        .step(
            StepSpec::new(
                STEP_DEEPLINK,
                "A link to open (`url`); by default the plugin's links command runs with it.",
            )
            .args(json!({ "url": "myapp://open/42" })),
        )?;
        // A click on one of these links inside the application (the
        // links service) opens it here, through the same step.
        let links = self.links.clone();
        ctx.command(
            CommandSpec::new(CMD_DEEPLINK, "Open an application link", "Links")
                .description("Opens a link of the application's own scheme (`url`) through the `deeplink.open` step.")
                .args(json!({ "url": "myapp://open/42" }))
                .hidden()
                .no_agent(),
            move |call| {
                let url = call.args["url"].as_str().ok_or("`url` is missing")?;
                open_link(call.app, links.as_ref(), url);
                Ok(json!({ "url": url }))
            },
        )?;
        let schemes = self.schemes.clone();
        ctx.on(On::Started, move |app, _| {
            if let Some(l) = egui_pop_api::links::links(app) {
                l.update_policy(|p| {
                    for s in &schemes {
                        *p = std::mem::take(p).app_scheme(s);
                    }
                });
            }
        });
        let launches = self.instance.launches();
        ctx.service(&LAUNCHES, launches.clone())?;
        launches.set_waker(Some(ctx.app().egui()));
        // The first launch's own links, once started.
        let first = self.instance.first().links(&self.schemes);
        let links = self.links.clone();
        let mut opened = false;
        ctx.on(On::Started, move |app, _| {
            if !std::mem::replace(&mut opened, true) {
                for url in &first {
                    open_link(app, links.as_ref(), url);
                }
            }
        });
        let (schemes, links) = (self.schemes.clone(), self.links.clone());
        ctx.on(On::Pass, move |app, _| {
            for launch in launches.take() {
                let found = launch.links(&schemes);
                let args = json!({
                    "args": launch.args,
                    "cwd": launch.cwd,
                    "links": found,
                });
                let links = links.clone();
                app.step(&STEP_SECOND_LAUNCH, args, move |app, args| {
                    crate::window::raise(&app.egui());
                    let urls: Vec<String> = args["links"]
                        .as_array()
                        .map(|l| {
                            l.iter()
                                .filter_map(Json::as_str)
                                .map(String::from)
                                .collect()
                        })
                        .unwrap_or_default();
                    for url in &urls {
                        open_link(app, links.as_ref(), url);
                    }
                    Ok(json!({ "raised": true, "links": urls.len() }))
                });
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        self.instance.launches.set_waker(None);
    }
}
