//! Global shortcuts: the key bindings of the system scope, registered
//! with the desktop so their command runs while the application is not
//! focused.
//!
//! A command declares one like any key, with a scope:
//! `CommandSpec::key_with("Ctrl+Alt+P", BindOptions::system())` (`system`
//! in keymap files). It shows in the shortcuts editor, the user rebinds
//! it there (it stays in the system scope), and inside the window it
//! fires as a global binding does. [`SystemKeysPlugin`] registers the
//! live ones ([`AppHandle::system_bindings`]) with the desktop at start
//! and after each change, and says how it went through the
//! `SYSTEM_KEYS` service (`egui_pop_api::system_keys`), which the
//! shortcuts editor shows: also outside the window, waiting for the
//! desktop, refused and why, or only in the window because the desktop
//! takes no global shortcuts.
//!
//! Its step (ADR 0011):
//!
//! - `keys.system`: a global shortcut was pressed, `{ "command", "keys",
//!   "backend" }`. By default the command runs through the host. Veto
//!   it while a modal task runs; replace it to bring the window up
//!   first.
//!
//! Backends: [`HotKeys`] through global-hotkey (Windows, macOS, X11),
//! [`Portal`] through the XDG GlobalShortcuts portal on Wayland (GNOME
//! 48 and later, KDE Plasma 5.27 and later; the desktop asks the user to
//! accept the shortcuts, and may let them pick other keys), [`detect`]
//! choosing between them, and [`Simulated`] for tests.
//!
//! A global shortcut is one chord (`Ctrl+Alt+P`); a sequence
//! (`Ctrl+K Ctrl+P`) is refused, and the editor says so. `Ctrl` is
//! Command on macOS, as in egui-pop's other bindings.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::system_keys::{KeyStatus, SYSTEM_KEYS};
//! use egui_pop_api::{Ctx, Plugin, PluginError};
//! use egui_pop_model::commands::CommandSpec;
//! use egui_pop_model::ids::{CommandId, PluginId};
//! use egui_pop_model::keymap::BindOptions;
//! use egui_pop_platform::keys::{Simulated, SystemKeysPlugin};
//!
//! struct Player;
//!
//! impl Plugin for Player {
//!     fn id(&self) -> PluginId { PluginId::from_static("player") }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.command(
//!             CommandSpec::new(CommandId::from_static("player.toggle"), "Play or pause", "Player")
//!                 .key_with("Ctrl+Alt+P", BindOptions::system()),
//!             |_| Ok(serde_json::Value::Null),
//!         )?;
//!         Ok(())
//!     }
//! }
//!
//! let desktop = Simulated::default();
//! let ctx = egui::Context::default();
//! let mut app = App::builder()
//!     .plugin(Player)
//!     .plugin(SystemKeysPlugin::new("org.example.Player").backend(desktop.clone()))
//!     .build(&ctx)
//!     .unwrap();
//! app.logic(&ctx);
//! let keys = app.handle().service(&SYSTEM_KEYS).unwrap();
//! assert_eq!(keys.status("Ctrl+Alt+P"), Some(KeyStatus::Registered));
//! desktop.press("Ctrl+Alt+P");
//! app.logic(&ctx);
//! assert_eq!(app.handle().history()[0].id.as_str(), "player.toggle");
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui_pop_api::hooks::On;
use egui_pop_api::steps::StepSpec;
use egui_pop_api::system_keys::{KeyStatus, SYSTEM_KEYS, Support, SystemKeys};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use egui_pop_model::keymap::{Action, Chord};
use serde_json::json;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.system_keys");
/// A global shortcut was pressed: `{ "command", "keys", "backend" }`.
pub const STEP_PRESSED: StepId = StepId::from_static("keys.system");

/// A shortcut to register.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Its keys as shown (`Ctrl+Alt+P`): its id with the desktop too.
    pub keys: String,
    pub chord: Chord,
    pub command: CommandId,
    /// What it does, for desktops that list shortcuts (the command's
    /// title).
    pub description: String,
}

/// Hands presses to the plugin from any thread, waking the interface.
#[derive(Clone, Debug)]
pub struct Presses {
    tx: Sender<String>,
    waker: egui::Context,
}

impl Presses {
    /// The shortcut with these keys was pressed.
    pub fn send(&self, keys: &str) {
        let _ = self.tx.send(keys.into());
        self.waker.request_repaint();
    }
}

/// What registers global shortcuts with the desktop. It lives on the
/// interface thread (global-hotkey's manager is tied to it on Windows
/// and macOS); its threads write each registration's state to `status`.
pub trait Backend: 'static {
    /// A short name: `x11`, `windows`, `macos`, `portal`, `simulated`.
    fn name(&self) -> String;
    /// Starts; says whether the desktop takes global shortcuts (it may
    /// say later, through `status`).
    fn start(&mut self, presses: Presses, status: Arc<SystemKeys>) -> Support;
    /// Registers exactly these (the earlier ones go), each one's state
    /// written to `status` now or later.
    fn register(&mut self, shortcuts: &[Shortcut]);
    /// Takes every shortcut back.
    fn stop(&mut self);
}

/// The backend for this session: the portal on Wayland, global-hotkey
/// elsewhere (nothing on the web).
pub fn detect(app_id: &str) -> Box<dyn Backend> {
    #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
    if crate::window::wayland() {
        return Box::new(Portal::new(app_id));
    }
    let _ = app_id;
    Box::new(HotKeys::default())
}

/// The key of a chord as egui names it, for the desktops' names.
fn egui_key(chord: &Chord) -> Option<egui::Key> {
    egui_pop_core::keys::key_of(chord).map(|(_, k)| k)
}

/// Global shortcuts through global-hotkey: Windows, macOS, X11.
#[derive(Default)]
pub struct HotKeys {
    #[cfg(not(target_arch = "wasm32"))]
    manager: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(not(target_arch = "wasm32"))]
    registered: Vec<global_hotkey::hotkey::HotKey>,
    /// Keys by hotkey id, read by the event handler.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    ids: Arc<Mutex<BTreeMap<u32, String>>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    status: Option<Arc<SystemKeys>>,
}

impl std::fmt::Debug for HotKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotKeys").finish_non_exhaustive()
    }
}

/// global-hotkey's code for an egui key.
#[cfg(not(target_arch = "wasm32"))]
pub fn code_of(key: egui::Key) -> Option<global_hotkey::hotkey::Code> {
    use egui::Key as K;
    use global_hotkey::hotkey::Code as C;
    let letters = [
        (K::A, C::KeyA),
        (K::B, C::KeyB),
        (K::C, C::KeyC),
        (K::D, C::KeyD),
        (K::E, C::KeyE),
        (K::F, C::KeyF),
        (K::G, C::KeyG),
        (K::H, C::KeyH),
        (K::I, C::KeyI),
        (K::J, C::KeyJ),
        (K::K, C::KeyK),
        (K::L, C::KeyL),
        (K::M, C::KeyM),
        (K::N, C::KeyN),
        (K::O, C::KeyO),
        (K::P, C::KeyP),
        (K::Q, C::KeyQ),
        (K::R, C::KeyR),
        (K::S, C::KeyS),
        (K::T, C::KeyT),
        (K::U, C::KeyU),
        (K::V, C::KeyV),
        (K::W, C::KeyW),
        (K::X, C::KeyX),
        (K::Y, C::KeyY),
        (K::Z, C::KeyZ),
        (K::Num0, C::Digit0),
        (K::Num1, C::Digit1),
        (K::Num2, C::Digit2),
        (K::Num3, C::Digit3),
        (K::Num4, C::Digit4),
        (K::Num5, C::Digit5),
        (K::Num6, C::Digit6),
        (K::Num7, C::Digit7),
        (K::Num8, C::Digit8),
        (K::Num9, C::Digit9),
        (K::F1, C::F1),
        (K::F2, C::F2),
        (K::F3, C::F3),
        (K::F4, C::F4),
        (K::F5, C::F5),
        (K::F6, C::F6),
        (K::F7, C::F7),
        (K::F8, C::F8),
        (K::F9, C::F9),
        (K::F10, C::F10),
        (K::F11, C::F11),
        (K::F12, C::F12),
        (K::F13, C::F13),
        (K::F14, C::F14),
        (K::F15, C::F15),
        (K::F16, C::F16),
        (K::F17, C::F17),
        (K::F18, C::F18),
        (K::F19, C::F19),
        (K::F20, C::F20),
        (K::F21, C::F21),
        (K::F22, C::F22),
        (K::F23, C::F23),
        (K::F24, C::F24),
        (K::ArrowUp, C::ArrowUp),
        (K::ArrowDown, C::ArrowDown),
        (K::ArrowLeft, C::ArrowLeft),
        (K::ArrowRight, C::ArrowRight),
        (K::Home, C::Home),
        (K::End, C::End),
        (K::PageUp, C::PageUp),
        (K::PageDown, C::PageDown),
        (K::Insert, C::Insert),
        (K::Delete, C::Delete),
        (K::Backspace, C::Backspace),
        (K::Enter, C::Enter),
        (K::Escape, C::Escape),
        (K::Tab, C::Tab),
        (K::Space, C::Space),
        (K::Minus, C::Minus),
        (K::Equals, C::Equal),
        (K::Comma, C::Comma),
        (K::Period, C::Period),
        (K::Slash, C::Slash),
        (K::Backslash, C::Backslash),
        (K::Semicolon, C::Semicolon),
        (K::Quote, C::Quote),
        (K::Backtick, C::Backquote),
        (K::OpenBracket, C::BracketLeft),
        (K::CloseBracket, C::BracketRight),
    ];
    letters.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

/// The hotkey of a chord, or why there is none.
#[cfg(not(target_arch = "wasm32"))]
pub fn hotkey_of(chord: &Chord) -> Result<global_hotkey::hotkey::HotKey, String> {
    use global_hotkey::hotkey::{CMD_OR_CTRL, HotKey, Modifiers};
    let key = egui_key(chord).ok_or_else(|| format!("`{}` is not a key", chord.key))?;
    let code =
        code_of(key).ok_or_else(|| format!("`{}` cannot be a global shortcut", chord.key))?;
    let mut mods = Modifiers::empty();
    if chord.ctrl {
        mods |= CMD_OR_CTRL;
    }
    if chord.alt {
        mods |= Modifiers::ALT;
    }
    if chord.shift {
        mods |= Modifiers::SHIFT;
    }
    if chord.meta {
        mods |= Modifiers::SUPER;
    }
    Ok(HotKey::new(Some(mods), code))
}

impl Backend for HotKeys {
    fn name(&self) -> String {
        if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_arch = "wasm32") {
            "none"
        } else {
            "x11"
        }
        .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(&mut self, presses: Presses, status: Arc<SystemKeys>) -> Support {
        use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
        if cfg!(all(unix, not(target_os = "macos"))) && std::env::var_os("DISPLAY").is_none() {
            return Support::Unavailable {
                reason: "no X display to grab keys on".into(),
            };
        }
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                return Support::Unavailable {
                    reason: e.to_string(),
                };
            }
        };
        let ids = self.ids.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state() == HotKeyState::Pressed
                && let Some(keys) = lock(&ids).get(&e.id()).cloned()
            {
                presses.send(&keys);
            }
        }));
        self.manager = Some(manager);
        self.status = Some(status);
        Support::Available {
            backend: self.name(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start(&mut self, _: Presses, _: Arc<SystemKeys>) -> Support {
        Support::Unavailable {
            reason: "no global shortcuts on the web".into(),
        }
    }

    fn register(&mut self, shortcuts: &[Shortcut]) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (Some(manager), Some(status)) = (&self.manager, &self.status) else {
                return;
            };
            let _ = manager.unregister_all(&self.registered);
            self.registered.clear();
            let mut ids = lock(&self.ids);
            ids.clear();
            for s in shortcuts {
                let state = match hotkey_of(&s.chord) {
                    Ok(hk) => match manager.register(hk) {
                        Ok(()) => {
                            ids.insert(hk.id(), s.keys.clone());
                            self.registered.push(hk);
                            KeyStatus::Registered
                        }
                        Err(e) => KeyStatus::Failed(e.to_string()),
                    },
                    Err(e) => KeyStatus::Failed(e),
                };
                status.set_status(&s.keys, state);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = shortcuts;
    }

    fn stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use global_hotkey::GlobalHotKeyEvent;
            if let Some(m) = &self.manager {
                let _ = m.unregister_all(&self.registered);
            }
            GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
            self.registered.clear();
            self.manager = None;
        }
    }
}

/// The XDG shortcuts specification's trigger for a chord
/// (`CTRL+ALT+p`), as the GlobalShortcuts portal takes it.
pub fn xdg_trigger(chord: &Chord) -> Option<String> {
    use egui::Key as K;
    let key = egui_key(chord)?;
    let name = match key {
        K::ArrowUp => "Up".into(),
        K::ArrowDown => "Down".into(),
        K::ArrowLeft => "Left".into(),
        K::ArrowRight => "Right".into(),
        K::PageUp => "Page_Up".into(),
        K::PageDown => "Page_Down".into(),
        K::Backspace => "BackSpace".into(),
        K::Enter => "Return".into(),
        K::Space => "space".into(),
        K::Minus => "minus".into(),
        K::Equals => "equal".into(),
        K::Plus => "plus".into(),
        K::Comma => "comma".into(),
        K::Period => "period".into(),
        K::Slash => "slash".into(),
        K::Backslash => "backslash".into(),
        K::Semicolon => "semicolon".into(),
        K::Quote => "apostrophe".into(),
        K::Backtick => "grave".into(),
        K::OpenBracket => "bracketleft".into(),
        K::CloseBracket => "bracketright".into(),
        K::Home | K::End | K::Insert | K::Delete | K::Escape | K::Tab => format!("{key:?}"),
        k => {
            let n = k.name();
            if n.len() == 1 {
                n.to_lowercase()
            } else if n.starts_with('F') && n[1..].chars().all(|c| c.is_ascii_digit()) {
                n.to_string()
            } else {
                return None;
            }
        }
    };
    let mut out = String::new();
    for (on, m) in [
        (chord.ctrl, "CTRL"),
        (chord.alt, "ALT"),
        (chord.shift, "SHIFT"),
        (chord.meta, "LOGO"),
    ] {
        if on {
            out.push_str(m);
            out.push('+');
        }
    }
    out.push_str(&name);
    Some(out)
}

/// Global shortcuts through the XDG GlobalShortcuts portal (Wayland).
/// Its thread holds a portal session; each registration replaces the
/// session, and the desktop asks the user to accept the shortcuts.
#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
pub struct Portal {
    app_id: String,
    requests: Option<async_channel_lite::Sender<Vec<Shortcut>>>,
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
impl std::fmt::Debug for Portal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Portal")
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

/// A channel the portal's thread waits on with its other futures: a
/// queue and an `async_io` timer polling it (requests are rare).
#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
mod async_channel_lite {
    use std::sync::mpsc;
    use std::time::Duration;

    pub type Sender<T> = mpsc::Sender<T>;

    /// The next value, waiting asynchronously; none when the sender is
    /// gone.
    pub async fn recv<T>(rx: &mpsc::Receiver<T>) -> Option<T> {
        loop {
            match rx.try_recv() {
                Ok(v) => return Some(v),
                Err(mpsc::TryRecvError::Disconnected) => return None,
                Err(mpsc::TryRecvError::Empty) => {
                    async_io::Timer::after(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
impl Portal {
    /// `app_id`: the application's id (`org.example.App`), registered
    /// with the portal for an application outside a sandbox.
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: app_id.into(),
            requests: None,
        }
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
impl Backend for Portal {
    fn name(&self) -> String {
        "portal".into()
    }

    fn start(&mut self, presses: Presses, status: Arc<SystemKeys>) -> Support {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<Shortcut>>();
        let app_id = self.app_id.clone();
        let waker = presses.waker.clone();
        let spawned = std::thread::Builder::new()
            .name("egui-pop global shortcuts".into())
            .spawn(move || {
                futures_lite::future::block_on(portal_loop(&app_id, rx, presses, &status));
                waker.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.requests = Some(tx);
                // Said by the thread once the portal answers.
                Support::Unknown
            }
            Err(e) => Support::Unavailable {
                reason: e.to_string(),
            },
        }
    }

    fn register(&mut self, shortcuts: &[Shortcut]) {
        if let Some(tx) = &self.requests {
            let _ = tx.send(shortcuts.to_vec());
        }
    }

    fn stop(&mut self) {
        // The thread ends when its queue goes.
        self.requests = None;
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
async fn portal_loop(
    app_id: &str,
    rx: std::sync::mpsc::Receiver<Vec<Shortcut>>,
    presses: Presses,
    status: &SystemKeys,
) {
    use ashpd::desktop::CreateSessionOptions;
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use futures_lite::StreamExt as _;

    let unavailable = |reason: String| {
        status.set_support(Support::Unavailable { reason });
        presses.waker.request_repaint();
    };
    // Outside a sandbox the portal knows the application by its id,
    // registered first (it needs the application's desktop file).
    let registered = match app_id.parse::<ashpd::AppID>() {
        Ok(id) => ashpd::register_host_app(id)
            .await
            .map_err(|e| e.to_string()),
        Err(e) => Err(format!("`{app_id}` is not an application id: {e}")),
    };
    if let Err(e) = &registered {
        log::warn!(target: "egui_pop", "global shortcuts: the portal does not know the application: {e}");
    }
    let portal = match GlobalShortcuts::new().await {
        Ok(p) => p,
        Err(e) => {
            return unavailable(format!(
                "the desktop offers no global shortcuts portal ({e})"
            ));
        }
    };
    status.set_support(Support::Available {
        backend: "portal".into(),
    });
    presses.waker.request_repaint();
    let mut activated = match portal.receive_activated().await {
        Ok(s) => Box::pin(s),
        Err(e) => return unavailable(e.to_string()),
    };
    let mut session: Option<ashpd::desktop::Session<GlobalShortcuts>> = None;
    loop {
        enum Next {
            Request(Option<Vec<Shortcut>>),
            Pressed(Option<String>),
        }
        let next = futures_lite::future::or(
            async { Next::Request(async_channel_lite::recv(&rx).await) },
            async { Next::Pressed(activated.next().await.map(|a| a.shortcut_id().to_string())) },
        )
        .await;
        match next {
            Next::Request(None) | Next::Pressed(None) => break,
            Next::Pressed(Some(keys)) => presses.send(&keys),
            Next::Request(Some(shortcuts)) => {
                if let Some(old) = session.take() {
                    let _ = old.close().await;
                }
                for s in &shortcuts {
                    status.set_status(&s.keys, KeyStatus::Pending);
                }
                presses.waker.request_repaint();
                if shortcuts.is_empty() {
                    continue;
                }
                let new: Vec<NewShortcut> = shortcuts
                    .iter()
                    .map(|s| {
                        NewShortcut::new(s.keys.clone(), s.description.clone())
                            .preferred_trigger(xdg_trigger(&s.chord).as_deref())
                    })
                    .collect();
                let created = portal.create_session(CreateSessionOptions::default()).await;
                let s = match created {
                    Ok(s) => s,
                    Err(e) => {
                        for sc in &shortcuts {
                            status.set_status(&sc.keys, KeyStatus::Failed(e.to_string()));
                        }
                        continue;
                    }
                };
                let bound = match portal
                    .bind_shortcuts(&s, &new, None, BindShortcutsOptions::default())
                    .await
                {
                    Ok(request) => request.response().map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                // Say what the user can do about it.
                let bound = bound.map_err(|e| match &registered {
                    Err(why) if e.contains("app id") => format!(
                        "the portal does not know the application: install its desktop file `{app_id}.desktop` ({why})"
                    ),
                    _ if e.contains("no information") => {
                        "the desktop refused them (does the compositor offer global shortcuts?)".into()
                    }
                    _ => e,
                });
                match bound {
                    Ok(answer) => {
                        let granted: Vec<&str> =
                            answer.shortcuts().iter().map(|s| s.id()).collect();
                        for sc in &shortcuts {
                            let state = if granted.contains(&sc.keys.as_str()) {
                                KeyStatus::Registered
                            } else {
                                KeyStatus::Failed("not accepted on the desktop".into())
                            };
                            status.set_status(&sc.keys, state);
                        }
                    }
                    Err(e) => {
                        for sc in &shortcuts {
                            status.set_status(&sc.keys, KeyStatus::Failed(e.clone()));
                        }
                    }
                }
                presses.waker.request_repaint();
                session = Some(s);
            }
        }
    }
    if let Some(old) = session.take() {
        let _ = old.close().await;
    }
}

#[derive(Debug, Default)]
struct SimState {
    support: Option<Support>,
    registered: Vec<Shortcut>,
    taken: Vec<String>,
    presses: Option<Presses>,
    status: Option<Arc<SystemKeys>>,
}

/// A desktop that records what is registered and presses keys as a
/// user would. For tests and the gallery.
#[derive(Clone, Debug, Default)]
pub struct Simulated(Arc<Mutex<SimState>>);

impl Simulated {
    /// A desktop taking no global shortcuts, saying why.
    pub fn unavailable(reason: &str) -> Self {
        let s = Self::default();
        lock(&s.0).support = Some(Support::Unavailable {
            reason: reason.into(),
        });
        s
    }

    /// These keys are taken by another application: registering them
    /// fails.
    pub fn taken(self, keys: &str) -> Self {
        lock(&self.0).taken.push(keys.into());
        self
    }

    /// What is registered.
    pub fn registered(&self) -> Vec<Shortcut> {
        lock(&self.0).registered.clone()
    }

    /// The user presses these keys; false when nothing is registered on
    /// them.
    pub fn press(&self, keys: &str) -> bool {
        let s = lock(&self.0);
        let found = s.registered.iter().any(|r| r.keys == keys);
        match (&s.presses, found) {
            (Some(p), true) => {
                p.send(keys);
                true
            }
            _ => false,
        }
    }
}

impl Backend for Simulated {
    fn name(&self) -> String {
        "simulated".into()
    }

    fn start(&mut self, presses: Presses, status: Arc<SystemKeys>) -> Support {
        let mut s = lock(&self.0);
        let support = s.support.clone().unwrap_or(Support::Available {
            backend: "simulated".into(),
        });
        if matches!(support, Support::Available { .. }) {
            s.presses = Some(presses);
            s.status = Some(status);
        }
        support
    }

    fn register(&mut self, shortcuts: &[Shortcut]) {
        let mut s = lock(&self.0);
        let taken = s.taken.clone();
        s.registered = shortcuts
            .iter()
            .filter(|sc| !taken.contains(&sc.keys))
            .cloned()
            .collect();
        if let Some(status) = &s.status {
            for sc in shortcuts {
                status.set_status(
                    &sc.keys,
                    if taken.contains(&sc.keys) {
                        KeyStatus::Failed("taken by another application".into())
                    } else {
                        KeyStatus::Registered
                    },
                );
            }
        }
    }

    fn stop(&mut self) {
        let mut s = lock(&self.0);
        s.registered.clear();
        s.presses = None;
        s.status = None;
    }
}

/// The shortcuts of the system scope now, and the bindings refused
/// before any desktop sees them (a sequence of chords, keys of no
/// desktop's).
fn shortcuts(app: &AppHandle) -> (Vec<Shortcut>, Vec<(String, String)>) {
    let mut ok = Vec::new();
    let mut refused = Vec::new();
    for b in app.system_bindings() {
        let Action::Run(command) = &b.action else {
            continue;
        };
        let keys = b.keys.to_string();
        if b.keys.0.len() != 1 {
            refused.push((keys, "a global shortcut is a single chord".into()));
            continue;
        }
        if egui_key(&b.keys.0[0]).is_none() {
            refused.push((keys, format!("`{}` is not a key", b.keys.0[0].key)));
            continue;
        }
        let description = app
            .command(command)
            .map(|s| app.command_title(&s))
            .unwrap_or_else(|| command.to_string());
        ok.push(Shortcut {
            keys,
            chord: b.keys.0[0].clone(),
            command: command.clone(),
            description,
        });
    }
    (ok, refused)
}

/// The backend while connected.
type Running = Rc<RefCell<Option<Box<dyn Backend>>>>;

/// Global shortcuts, as a plugin (see the module).
pub struct SystemKeysPlugin {
    app_id: String,
    backend: Option<Box<dyn Backend>>,
    running: Option<Running>,
}

impl std::fmt::Debug for SystemKeysPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemKeysPlugin")
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

impl SystemKeysPlugin {
    /// The session's backend ([`detect`]), for the application
    /// `app_id` (`org.example.App`).
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: app_id.into(),
            backend: None,
            running: None,
        }
    }

    /// Another backend ([`Simulated`] in tests).
    pub fn backend(mut self, backend: impl Backend) -> Self {
        self.backend = Some(Box::new(backend));
        self
    }
}

impl Plugin for SystemKeysPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Global shortcuts".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.step(
            StepSpec::new(
                STEP_PRESSED,
                "A global shortcut was pressed (`command`, `keys`, `backend`); by default its command runs.",
            )
            .args(json!({ "command": "player.toggle", "keys": "Ctrl+Alt+P", "backend": "x11" })),
        )?;
        ctx.service(&SYSTEM_KEYS, SystemKeys::default())?;
        let status = ctx
            .app()
            .service(&SYSTEM_KEYS)
            .ok_or_else(|| PluginError::Failed("the service is not there".into()))?;
        let mut backend = self.backend.take().unwrap_or_else(|| detect(&self.app_id));
        let name = backend.name();
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        let presses = Presses {
            tx,
            waker: ctx.app().egui(),
        };
        let support = backend.start(presses, status.clone());
        if support != Support::Unknown || status.support() == Support::Unknown {
            status.set_support(support);
        }
        let backend: Running = Rc::new(RefCell::new(Some(backend)));
        self.running = Some(backend.clone());
        let mut last: Option<Vec<Shortcut>> = None;
        ctx.on(On::Pass, move |app, _| {
            let (list, refused) = shortcuts(app);
            if last.as_ref() != Some(&list) {
                status.clear();
                for (keys, why) in &refused {
                    status.set_status(keys, KeyStatus::Failed(why.clone()));
                }
                if let Support::Unavailable { .. } = status.support() {
                    // Nothing to register with: the editor says why.
                } else if let Some(b) = backend.borrow_mut().as_mut() {
                    b.register(&list);
                }
                last = Some(list.clone());
            }
            for keys in rx.try_iter() {
                let Some(s) = list.iter().find(|s| s.keys == keys) else {
                    continue;
                };
                let command = s.command.clone();
                app.step(
                    &STEP_PRESSED,
                    json!({ "command": command, "keys": keys, "backend": name }),
                    |app, args| {
                        let command = args["command"]
                            .as_str()
                            .and_then(|c| CommandId::new(c).ok())
                            .ok_or("`command` is missing")?;
                        app.run_as(command.clone(), serde_json::Value::Null, Origin::User);
                        Ok(json!({ "ran": command }))
                    },
                );
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, _ctx: &mut Ctx<'_>) {
        if let Some(b) = self.running.take()
            && let Some(mut b) = b.borrow_mut().take()
        {
            b.stop();
            self.backend = Some(b);
        }
    }
}
