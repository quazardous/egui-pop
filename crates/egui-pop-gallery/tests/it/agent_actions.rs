//! Every collector action of the gallery, as an agent meets it: those
//! that only read and `commands.run` are offered, every one that writes
//! is refused before it runs, and the journal says so.

use egui_pop_gallery::{Gallery, Options};
use serde_json::Value as Json;

/// What agents may call in the gallery.
const OFFERED: [&str; 12] = [
    "bridge.describe",
    "bridge.wait",
    "commands.run",
    "fonts.check",
    "frame.hit",
    "gallery.viewport.wait_idle",
    "icons.search",
    "journal.since",
    "mounts.check",
    "mounts.where_can",
    "state.slot",
    "store.get",
];

fn gallery() -> Gallery {
    let ctx = egui::Context::default();
    let options = Options {
        no_persistence: true,
        ..Options::default()
    };
    let mut g = Gallery::new(&ctx, options).expect("the gallery builds");
    g.app.logic(&ctx);
    g
}

#[test]
fn agents_are_offered_reads_and_refused_every_write() {
    let mut g = gallery();
    let h = g.app.handle().clone();
    let mut offered = Vec::new();
    let mut refused: Vec<(String, String, Json)> = Vec::new();
    for c in h.debug_describe().as_array().unwrap() {
        for a in c["actions"].as_array().unwrap() {
            let name = format!(
                "{}.{}",
                c["name"].as_str().unwrap(),
                a["name"].as_str().unwrap()
            );
            let mutates = a["mutates"] == true;
            if a["agent"] == true {
                assert!(!mutates || name == "commands.run", "{name} writes");
                offered.push(name);
            } else {
                assert!(mutates, "{name} only reads");
                refused.push((
                    c["name"].as_str().unwrap().into(),
                    a["name"].as_str().unwrap().into(),
                    a["args"].clone(),
                ));
            }
        }
    }
    offered.sort();
    assert_eq!(offered, OFFERED);
    for (c, a, args) in &refused {
        let before = h.last_event();
        let f = g.app.debug_call(c, a, args);
        assert!(
            !f.ok && f.message.contains("not offered to agents"),
            "{f:?}"
        );
        let events = h.events_since(before);
        assert!(
            events.iter().any(|e| e.name == "debug.call.refused"
                && e.source == "agent:debug-bridge"
                && e.data["collector"] == c.as_str()
                && e.data["action"] == a.as_str()),
            "{c}.{a}: {events:?}"
        );
    }
    let mut names: Vec<String> = refused.iter().map(|(c, a, _)| format!("{c}.{a}")).collect();
    names.sort();
    eprintln!("offered: {offered:?}\nrefused: {names:?}");
    assert!(names.len() >= 20, "{names:?}");
}
