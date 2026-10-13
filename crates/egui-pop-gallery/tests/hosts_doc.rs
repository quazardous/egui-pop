//! `docs/hosts.md` is generated from the mount declarations (ADR 0013):
//! the committed page must be what the declarations write. Regenerate
//! with `cargo xtask hosts` (or `UPDATE_HOSTS=1 cargo test -p
//! egui-pop-gallery --test hosts_doc`).

use std::path::Path;

#[test]
fn the_hosts_page_is_what_the_declarations_write() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = crate_dir.join("../../docs/hosts.md");
    let fresh = egui_pop::mounts::page();
    if std::env::var_os("UPDATE_HOSTS").is_some() {
        std::fs::write(&path, &fresh).expect("docs/hosts.md written");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed != fresh {
        let first = committed
            .lines()
            .zip(fresh.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| committed.lines().count().min(fresh.lines().count()));
        panic!(
            "docs/hosts.md is stale from line {}: run `cargo xtask hosts`.\n\
             ----- generated docs/hosts.md -----\n{fresh}----- end of generated docs/hosts.md -----",
            first + 1
        );
    }
}

/// Every component and host the gallery's application knows is on the
/// page, and the other way round.
#[test]
fn the_page_and_the_application_know_the_same_declarations() {
    let ctx = egui::Context::default();
    let gallery = egui_pop_gallery::Gallery::new(&ctx, egui_pop_gallery::Options::default())
        .expect("the gallery builds");
    let app = gallery.app.handle();
    let mut known: Vec<String> = app.mountables().into_iter().map(|m| m.id).collect();
    let mut page: Vec<String> = egui_pop::mounts::components()
        .into_iter()
        .map(|m| m.id)
        .collect();
    known.sort();
    page.sort();
    assert_eq!(known, page);
    let mut known: Vec<String> = app.host_offers().into_iter().map(|h| h.id).collect();
    let mut page: Vec<String> = egui_pop::mounts::hosts()
        .into_iter()
        .map(|h| h.id)
        .collect();
    known.sort();
    page.sort();
    assert_eq!(known, page);
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
}
