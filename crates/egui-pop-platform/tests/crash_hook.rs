//! The panic hook writes this session's panics, with the journal's tail
//! (a test binary of its own: the hook is process-wide).

#![cfg(feature = "crash")]

use egui_pop_api::App;
use egui_pop_platform::crash::{CrashPlugin, Report};

#[test]
fn a_panic_is_written_with_the_journals_tail() {
    let d = std::env::temp_dir().join(format!("egui-pop-crash-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(CrashPlugin::new("Demo", "2.0").dir(&d))
        .build(&ctx)
        .unwrap();
    app.handle()
        .publish("demo", None, "demo.saved", serde_json::json!({ "n": 3 }));
    app.logic(&ctx);
    let caught = std::panic::catch_unwind(|| panic!("the catalogue is gone"));
    assert!(caught.is_err());
    let file = d.join(format!("panic-{}.json", std::process::id()));
    let report: Report = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(report.kind, "panic");
    assert_eq!(report.version, "2.0");
    assert_eq!(report.panics[0].message, "the catalogue is gone");
    assert!(report.panics[0].location.contains("crash_hook.rs"));
    assert!(report.journal.iter().any(|e| e["name"] == "demo.saved"));
    // A caught panic is no crash: a clean exit takes it away.
    app.exit();
    assert!(!file.exists());
    std::fs::remove_dir_all(&d).unwrap();
}
