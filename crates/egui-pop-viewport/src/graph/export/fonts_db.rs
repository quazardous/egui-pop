//! The fonts the PNG and PDF exports draw with: the faces egui-pop
//! installed for the theme, then egui's bundled faces, in a font
//! database of their own (never the system's, so an export is the same
//! on every machine).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};
use egui_pop_core::fonts::InstalledFace;
use fontdb::{Database, Language, Source};

/// A face's file, shared with egui rather than copied.
struct Bytes(Arc<FontData>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0.font
    }
}

/// A database of `faces`, then egui's bundled proportional faces (the
/// ones not already in), and the family text without a match falls to
/// (the first face's, also every generic family).
pub(super) fn database(faces: &[InstalledFace]) -> (Database, String) {
    let bundled = FontDefinitions::default();
    let tail = bundled
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut all: Vec<InstalledFace> = faces.to_vec();
    for key in tail {
        let Some(data) = bundled.font_data.get(&key) else {
            continue;
        };
        if all.iter().any(|f| f.data.font == data.font) {
            continue;
        }
        all.push(InstalledFace {
            // egui's `Ubuntu-Light` is the `Ubuntu` the built-in themes
            // name.
            family: (key == "Ubuntu-Light").then(|| "Ubuntu".to_string()),
            data: data.clone(),
        });
    }

    let mut db = Database::new();
    let mut first: Option<String> = None;
    for face in &all {
        let mut one = Database::new();
        one.load_font_source(Source::Binary(Arc::new(Bytes(face.data.clone()))));
        for info in one.faces() {
            if info.index != face.data.index {
                continue;
            }
            let mut info = info.clone();
            if let Some(name) = &face.family
                && !info.families.iter().any(|(f, _)| f == name)
            {
                // The name the theme gives it, after its own (the
                // first stays its English name).
                info.families
                    .push((name.clone(), Language::English_UnitedStates));
            }
            if first.is_none() {
                first = info.families.first().map(|(f, _)| f.clone());
            }
            db.push_face_info(info);
        }
    }
    let family = first.unwrap_or_else(|| "sans-serif".into());
    db.set_sans_serif_family(family.clone());
    db.set_serif_family(family.clone());
    db.set_monospace_family(family.clone());
    db.set_cursive_family(family.clone());
    db.set_fantasy_family(family.clone());
    (db, family)
}
