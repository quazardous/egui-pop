//! The `ui` method's pointer actions over the socket: `hover` shows a
//! tooltip, `secondary_click` opens a context menu, in an application in
//! a kittest harness.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::core::debug::BridgeMode;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, PluginId};
use serde_json::{Value as Json, json};

/// What the widgets saw: right-clicks, and the menu item's clicks.
#[derive(Default)]
struct Seen {
    secondary: u32,
    item: u32,
}

/// A button with a tooltip, and one with a context menu.
struct Pointer {
    seen: Arc<Mutex<Seen>>,
}

impl Plugin for Pointer {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.pointer")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let seen = self.seen.clone();
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("test.pointer"), 0, move |ui, _| {
                ui.button("Hover me").on_hover_text("A tip");
                let r = ui.button("Right-click me");
                if r.secondary_clicked() {
                    seen.lock().unwrap().secondary += 1;
                }
                r.context_menu(|ui| {
                    if ui.button("Menu item").clicked() {
                        seen.lock().unwrap().item += 1;
                        ui.close();
                    }
                });
            }),
        );
        Ok(())
    }
}

fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn app(socket: &Path, seen: Arc<Mutex<Seen>>) -> Harness<'static, App> {
    let builder = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Pointer { seen })
        .debug_bridge(BridgeMode::On)
        .app_id("org.example.UiPointer")
        .agent_socket(AgentSocket::at(socket));
    let mut h = egui_pop_test::app(builder, egui::vec2(800.0, 600.0));
    h.step();
    h
}

/// A JSON-RPC client over the socket.
struct Client {
    read: BufReader<UnixStream>,
    write: UnixStream,
    next: u64,
}

impl Client {
    fn connect(path: &Path) -> Self {
        let write = UnixStream::connect(path).expect("the socket answers");
        write
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        Self {
            read: BufReader::new(write.try_clone().unwrap()),
            write,
            next: 1,
        }
    }

    /// The result, failing on an error.
    fn ui(&mut self, params: Json) -> Json {
        let id = self.next;
        self.next += 1;
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": "ui", "params": params });
        writeln!(self.write, "{req}").unwrap();
        let mut line = String::new();
        self.read.read_line(&mut line).expect("a response line");
        let r: Json = serde_json::from_str(&line).expect("JSON");
        assert_eq!(r["id"], id, "{r}");
        assert!(r.get("error").is_none(), "ui {params}: {r}");
        r["result"].clone()
    }

    /// How many nodes are labelled `label`, once there are some or at the
    /// deadline.
    fn appears(&mut self, label: &str) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let n = self.ui(json!({ "action": "tree", "label": label }))["count"]
                .as_u64()
                .unwrap();
            if n > 0 || Instant::now() > deadline {
                return n;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Runs `client` on its own thread while stepping frames, until it ends.
fn with_client<T: Send + 'static>(
    h: &mut Harness<'static, App>,
    client: impl FnOnce() -> T + Send + 'static,
) -> T {
    let thread = std::thread::spawn(client);
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the client did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the client passes")
}

#[test]
fn hover_shows_a_tooltip_and_a_secondary_click_opens_a_context_menu() {
    let dir = private_dir("pointer");
    let path = dir.join("app.sock");
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mut h = app(&path, seen.clone());
    let p = path.clone();
    with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        assert_eq!(c.appears("Hover me"), 1);
        assert_eq!(
            c.ui(json!({ "action": "tree", "label": "A tip" }))["count"],
            0
        );
        let r = c.ui(json!({ "action": "hover", "role": "Button", "label": "Hover me" }));
        assert_eq!(r["node"]["label"], "Hover me", "{r}");
        assert_eq!(c.appears("A tip"), 1, "the tooltip shows once hovered");

        assert_eq!(
            c.ui(json!({ "action": "tree", "label": "Menu item" }))["count"],
            0
        );
        c.ui(json!({ "action": "secondary_click", "label": "Right-click me" }));
        assert_eq!(c.appears("Menu item"), 1, "the context menu opens");
        c.ui(json!({ "action": "click", "label": "Menu item" }));
    });
    let seen = seen.lock().unwrap();
    assert_eq!(seen.secondary, 1, "one right-click");
    assert_eq!(seen.item, 1, "the menu item clicked");
}
