//! `dirs`' contract (docs/dirs.md): the state directory read from the
//! environment it is given, per platform; the places derived from it.
//! The environment of the process is read, never changed: the derived
//! places are checked against `state_dir`, whatever it says here.
//! `reveal` starts the system's opener and is not run.

use std::path::PathBuf;

use egui_pop_platform::dirs::{
    cache_dir, crash_dir, log_dir, pid, state_dir, state_dir_with, temp_dir,
};

fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        vars.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn on_linux_the_state_is_under_xdg_state_home_else_home() {
    let dir = |vars: &[(&str, &str)]| state_dir_with("demo", env(vars));
    assert_eq!(
        dir(&[("XDG_STATE_HOME", "/xdg"), ("HOME", "/home/u")]),
        Some(PathBuf::from("/xdg/demo"))
    );
    assert_eq!(
        dir(&[("HOME", "/home/u")]),
        Some(PathBuf::from("/home/u/.local/state/demo"))
    );
    // A relative place is no place: the next one is taken.
    assert_eq!(
        dir(&[("XDG_STATE_HOME", "state"), ("HOME", "/home/u")]),
        Some(PathBuf::from("/home/u/.local/state/demo"))
    );
    assert_eq!(dir(&[("XDG_STATE_HOME", "")]), None);
    assert_eq!(dir(&[("HOME", "home")]), None);
    assert_eq!(dir(&[]), None);
    // Windows' and macOS' variables mean nothing here.
    assert_eq!(dir(&[("LOCALAPPDATA", "/l")]), None);
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_the_state_is_in_application_support() {
    let dir = |vars: &[(&str, &str)]| state_dir_with("demo", env(vars));
    assert_eq!(
        dir(&[("HOME", "/Users/u"), ("XDG_STATE_HOME", "/xdg")]),
        Some(PathBuf::from("/Users/u/Library/Application Support/demo"))
    );
    assert_eq!(dir(&[("HOME", "u")]), None);
    assert_eq!(dir(&[]), None);
}

#[cfg(windows)]
#[test]
fn on_windows_the_state_is_in_local_app_data() {
    let dir = |vars: &[(&str, &str)]| state_dir_with("demo", env(vars));
    assert_eq!(
        dir(&[("LOCALAPPDATA", r"C:\Users\u\AppData\Local")]),
        Some(PathBuf::from(r"C:\Users\u\AppData\Local\demo"))
    );
    assert_eq!(dir(&[("LOCALAPPDATA", "Local")]), None);
    assert_eq!(dir(&[("HOME", r"C:\Users\u")]), None);
}

#[test]
fn the_logs_and_crash_reports_sit_under_the_state() {
    let state = state_dir("demo");
    assert_eq!(state, state_dir_with("demo", |n| std::env::var(n).ok()));
    assert_eq!(crash_dir("demo"), state.as_ref().map(|d| d.join("crashes")));
    if !cfg!(target_os = "macos") {
        assert_eq!(log_dir("demo"), state.as_ref().map(|d| d.join("logs")));
    }
    if let Some(s) = &state {
        assert!(s.is_absolute(), "{s:?}");
        assert!(s.ends_with("demo"), "{s:?}");
    }
}

#[test]
fn the_cache_is_the_apps_own_egui_pop_folder() {
    if let Some(c) = cache_dir("demo") {
        assert!(c.ends_with("demo/egui-pop"), "{c:?}");
    }
}

#[test]
fn temp_dir_and_pid_are_those_of_the_process() {
    assert_eq!(temp_dir(), std::env::temp_dir());
    assert_eq!(pid(), std::process::id());
}
