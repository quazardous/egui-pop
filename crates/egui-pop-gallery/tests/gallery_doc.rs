//! `docs/gallery.md` shows every snapshot the gallery's tests keep.

use std::path::Path;

#[test]
fn the_gallery_page_shows_every_snapshot() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(crate_dir.join("../../docs/gallery.md")).unwrap();
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(crate_dir.join("tests/snapshots")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let Some(stem) = name.strip_suffix(".png") else {
            continue;
        };
        if stem.ends_with(".new") || stem.ends_with(".old") || stem.ends_with(".diff") {
            continue;
        }
        if !doc.contains(&format!("tests/snapshots/{name}")) {
            missing.push(name);
        }
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "docs/gallery.md does not show these snapshots: {missing:?}"
    );
}

/// Four snapshots to a row: only a table's last row may be shorter.
#[test]
fn the_gallery_page_has_four_snapshots_to_a_row() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(crate_dir.join("../../docs/gallery.md")).unwrap();
    for (t, table) in doc.split("<table>").skip(1).enumerate() {
        let table = table.split("</table>").next().unwrap();
        let rows: Vec<usize> = table
            .split("<tr>")
            .skip(1)
            .map(|row| row.matches("<td").count())
            .collect();
        for (r, &cells) in rows.iter().enumerate() {
            let last = r + 1 == rows.len();
            assert!(
                cells == 4 || (last && (1..4).contains(&cells)),
                "docs/gallery.md, table {}, row {}: {cells} snapshots, not 4",
                t + 1,
                r + 1
            );
        }
    }
}
