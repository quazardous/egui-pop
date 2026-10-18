//! A kit to install (`docs/kit-packages.md`): its data (tokens, faces,
//! licence: `egui_pop_tokens::kit::KitData`) and, for a kit with code,
//! the handlers its `shape` tokens name.
//!
//! A data-only kit is a folder or a `kit!`; a kit with code is a crate
//! exposing a function giving its source:
//!
//! ```
//! use std::sync::Arc;
//! use egui_pop_core::kit::{ComponentKit, KitCx, KitSource, Part, Primitive};
//!
//! struct Notch;
//! impl Primitive<ComponentKit> for Notch {
//!     fn measure(&self, data: &ComponentKit, label: egui::Vec2) -> egui::Vec2 {
//!         egui::vec2(label.x + 2.0 * data.padding_x, data.height)
//!     }
//!     fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
//!         let b = data.states.get(part.state);
//!         cx.painter.rect_filled(part.rect, b.radius, b.fill);
//!     }
//!     fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
//!         egui_pop_core::kit::paint_label(cx, data, part);
//!     }
//! }
//!
//! /// The kit's crate: `pub fn kit() -> KitSource`.
//! pub fn kit() -> KitSource {
//!     let data = egui_pop_tokens::kit::KitData::from_text(
//!         "notched",
//!         r#"{ "tab": { "shape": { "$type": "string", "$value": "notch" } } }"#,
//!     );
//!     KitSource::new(data).shape("tab", "notch", Arc::new(Notch))
//! }
//!
//! let ctx = egui::Context::default();
//! let source = kit();
//! source.register(&ctx);
//! let mut themes = egui_pop_tokens::Themes::default();
//! themes.add_kit(&source.data);
//! egui_pop_core::theme::install_with(&ctx, &themes, &egui_pop_tokens::ThemeSpec::new("notched"));
//! assert_eq!(egui_pop_core::kit::Kit::of(&ctx).tab.shape, "notch");
//! ```
//!
//! `AppBuilder::kit` does those three steps in an application.

use std::sync::Arc;

use egui::Context;
use egui_pop_tokens::kit::{Bytes, KitData};

use super::{COMPONENTS, ComponentKit, Primitive, register_shape};
use crate::fonts::{self, Face, Weight};

type Handler = Arc<dyn Primitive<ComponentKit>>;

/// A kit to install: its data and its handlers.
#[derive(Clone)]
pub struct KitSource {
    pub data: KitData,
    shapes: Vec<(String, String, Handler)>,
    problems: Vec<String>,
}

impl From<KitData> for KitSource {
    fn from(data: KitData) -> Self {
        KitSource::new(data)
    }
}

impl KitSource {
    /// A kit with no handlers of its own (data only).
    pub fn new(data: KitData) -> KitSource {
        KitSource {
            data,
            shapes: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// Adds a handler its tokens can name (`tab.shape`), for a component
    /// of [`COMPONENTS`]; one for another component is reported and not
    /// registered.
    #[must_use]
    pub fn shape(mut self, component: &str, name: &str, handler: Handler) -> KitSource {
        if COMPONENTS.iter().any(|(c, _)| *c == component) {
            self.shapes.push((component.into(), name.into(), handler));
        } else {
            self.problems.push(format!(
                "handler `{name}`: no kit component is named `{component}` (known: {}); not registered",
                COMPONENTS
                    .iter()
                    .map(|(c, _)| *c)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        self
    }

    /// The kit's theme name.
    pub fn name(&self) -> &str {
        &self.data.name
    }

    /// The handlers it adds: component and name.
    pub fn handlers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.shapes.iter().map(|(c, n, _)| (c.as_str(), n.as_str()))
    }

    /// Its faces, for `fonts::register`.
    pub fn faces(&self) -> Vec<Face> {
        self.data
            .fonts
            .iter()
            .map(|f| match &f.bytes {
                Bytes::Borrowed(b) => Face::from_static(&f.family, Weight(f.weight), b),
                Bytes::Owned(b) => Face::from_owned(&f.family, Weight(f.weight), b.clone()),
            })
            .collect()
    }

    /// What was wrong reading the kit and adding its handlers, as text.
    pub fn problems(&self) -> Vec<String> {
        self.data
            .problems()
            .iter()
            .map(ToString::to_string)
            .chain(self.problems.iter().cloned())
            .collect()
    }

    /// Registers its faces and handlers in a context, in use from the next
    /// theme install (its theme is added to the `Themes` the application
    /// resolves: `Themes::add_kit`). Called at start-up or in a "change
    /// kit" step, never per frame.
    pub fn register(&self, ctx: &Context) {
        for face in self.faces() {
            fonts::register(ctx, face);
        }
        for (component, name, handler) in &self.shapes {
            register_shape(ctx, component, name, handler.clone());
        }
    }
}

impl std::fmt::Debug for KitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KitSource")
            .field("name", &self.data.name)
            .field("handlers", &self.handlers().collect::<Vec<_>>())
            .field("fonts", &self.data.fonts)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_tokens::kit::KitFont;
    use egui_pop_tokens::{ThemeSpec, Themes};

    static INTER_VAR: &[u8] = include_bytes!("../../tests/fonts/InterVariable-subset.ttf");

    #[test]
    fn a_kit_registers_its_faces_and_its_theme_names_them() {
        let mut data = KitData::from_text(
            "inter",
            r#"{ "font": { "family": { "ui": { "$type": "fontFamily", "$value": ["Inter", "sans-serif"] } } } }"#,
        );
        data.fonts.push(KitFont {
            file: "Inter.ttf".into(),
            family: "Inter".into(),
            weight: 400,
            bytes: Bytes::Borrowed(INTER_VAR),
        });
        let source = KitSource::from(data);
        let ctx = Context::default();
        source.register(&ctx);
        let mut themes = Themes::default();
        themes.add_kit(&source.data);
        crate::theme::install_with(&ctx, &themes, &ThemeSpec::new("inter"));
        let report = fonts::report(&ctx).expect("installed");
        assert!(report.problems.is_empty(), "{report:?}");
        assert!(
            report
                .primary
                .iter()
                .any(|(_, p)| p.as_deref().is_some_and(|p| p.contains("Inter"))),
            "{report:?}"
        );
    }

    #[test]
    fn a_handler_for_no_component_is_reported() {
        let source = KitSource::new(KitData::from_text("k", "{}")).shape(
            "tabz",
            "x",
            Arc::new(super::super::shapes::Plain),
        );
        assert_eq!(source.handlers().count(), 0);
        assert!(
            source.problems()[0].contains("`tabz`"),
            "{:?}",
            source.problems()
        );
    }
}
