//! The `developer.agentSocket` setting: off by default, the socket
//! started and stopped live as it changes; `EGUI_POP_AGENT_SOCKET`
//! still turns it on whatever the setting says.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::{AgentSocket, AgentSocketExt as _, ENV, SETTING_AGENT_SOCKET};
use egui_pop_api::model::commands::Origin;
use egui_pop_api::model::values::{Change, Set, Value};
use egui_pop_api::{App, AppBuilder, DefaultPlugins};
use serde_json::{Value as Json, json};

/// A fresh private directory (mode 0700) for one test.
fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "egui-pop-agent-setting-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let builder = builder
        .plugins(DefaultPlugins)
        .app_id("org.example.AgentSetting");
    let mut h = egui_pop_test::app(builder, egui::vec2(800.0, 600.0));
    h.step();
    h
}

fn set(h: &mut Harness<'static, App>, on: bool) {
    let handle = h.state().handle().clone();
    let r = egui_pop_core::lock(&handle.store()).set(Set::new(
        &format!("settings/{SETTING_AGENT_SOCKET}"),
        Change::To(Value::Bool(on)),
        Origin::User,
    ));
    assert!(r.is_ok(), "{r:?}");
    h.step();
}

/// `describe` over the socket while the harness steps frames.
fn describe(h: &mut Harness<'static, App>, path: &Path) -> Json {
    let p = path.to_owned();
    let thread = std::thread::spawn(move || {
        let write = UnixStream::connect(&p).expect("the socket answers");
        write
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let mut read = BufReader::new(write.try_clone().unwrap());
        let mut write = write;
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "describe" });
        writeln!(write, "{req}").unwrap();
        let mut line = String::new();
        read.read_line(&mut line).expect("a response line");
        serde_json::from_str::<Json>(&line).expect("JSON")
    });
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the client did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the client passes")
}

#[test]
fn the_setting_is_off_by_default_and_shown() {
    let dir = private_dir("default");
    let path = dir.join("app.sock");
    let h = harness(App::builder().agent_socket_on_setting(AgentSocket::at(&path)));
    let handle = h.state().handle().clone();
    assert_eq!(handle.setting::<bool>(&SETTING_AGENT_SOCKET), Some(false));
    let entry = handle
        .settings()
        .into_iter()
        .find(|s| s.id == SETTING_AGENT_SOCKET)
        .expect("declared, so the settings window shows it");
    assert!(
        entry.descriptor.description.contains("Lets local programs"),
        "{}",
        entry.descriptor.description
    );
    assert!(!path.exists(), "nothing listens while the setting is off");
    assert!(UnixStream::connect(&path).is_err());
}

#[test]
fn toggling_the_setting_starts_and_stops_the_socket() {
    let dir = private_dir("toggle");
    let path = dir.join("app.sock");
    let mut h = harness(App::builder().agent_socket_on_setting(AgentSocket::at(&path)));
    assert!(UnixStream::connect(&path).is_err());

    set(&mut h, true);
    let r = describe(&mut h, &path);
    assert_eq!(r["result"]["protocol"], 1, "{r}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "mode {:o}", mode & 0o777);

    set(&mut h, false);
    assert!(!path.exists(), "the socket file is removed");
    assert!(UnixStream::connect(&path).is_err(), "refused once off");

    // And on again.
    set(&mut h, true);
    let r = describe(&mut h, &path);
    assert_eq!(r["result"]["protocol"], 1, "{r}");
    drop(h);
    assert!(!path.exists());
}

#[test]
fn a_client_connected_when_it_turns_off_is_refused() {
    let dir = private_dir("connected");
    let path = dir.join("app.sock");
    let mut h = harness(App::builder().agent_socket_on_setting(AgentSocket::at(&path)));
    set(&mut h, true);
    let write = UnixStream::connect(&path).expect("the socket answers");
    write
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    set(&mut h, false);
    let mut read = BufReader::new(write.try_clone().unwrap());
    let mut write = write;
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "describe" });
    writeln!(write, "{req}").unwrap();
    // The request reaches the application at its next pass.
    let thread = std::thread::spawn(move || {
        let mut line = String::new();
        read.read_line(&mut line).expect("a response line");
        serde_json::from_str::<Json>(&line).expect("JSON")
    });
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(start.elapsed() < Duration::from_secs(30), "no answer");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    let r = thread.join().unwrap();
    assert_eq!(r["error"]["code"], -32001, "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("off"),
        "{r}"
    );
}

/// The child's side of [`the_env_var_turns_it_on_whatever_the_setting`]:
/// runs only with the variable set by the parent.
#[test]
fn env_child() {
    let Ok(path) = std::env::var(ENV) else { return };
    if std::env::var("EGUI_POP_AGENT_SETTING_CHILD").is_err() {
        return;
    }
    let path = PathBuf::from(path);
    let mut h = harness(App::builder().agent_socket_from_env_or_setting());
    let handle = h.state().handle().clone();
    // Declared, and off: the variable wins.
    assert_eq!(handle.setting::<bool>(&SETTING_AGENT_SOCKET), Some(false));
    let r = describe(&mut h, &path);
    assert_eq!(r["result"]["protocol"], 1, "{r}");
    // Off it stays on: the variable asked for it.
    set(&mut h, false);
    let r = describe(&mut h, &path);
    assert_eq!(r["result"]["protocol"], 1, "{r}");
}

#[test]
fn the_env_var_turns_it_on_whatever_the_setting() {
    let dir = private_dir("env");
    let path = dir.join("app.sock");
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["env_child", "--exact", "--nocapture", "--test-threads=1"])
        .env(ENV, &path)
        .env("EGUI_POP_AGENT_SETTING_CHILD", "1")
        .output()
        .expect("the test binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("1 passed"), "the child ran: {stdout}");
}
