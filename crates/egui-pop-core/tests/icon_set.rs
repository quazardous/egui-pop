//! An application's own icons drawn in frames (docs/icons.md, section 8):
//! a single-colour icon is one image tinted with the colour a widget
//! gives, a full-colour one is drawn as it is, at its own pixel size; a
//! name works where a name does, from the next theme install, and stops
//! at once when unregistered. Without the `svg` feature, registering is
//! refused. The registry's own rules are its unit tests.

#[cfg(not(feature = "svg"))]
#[test]
fn without_the_svg_feature_registering_is_refused() {
    use egui_pop_core::icon_set::{self, IconError, Paint};
    assert_eq!(
        icon_set::register("test-frame:no-svg", &b"<svg/>"[..], Paint::Mono),
        Err(IconError::Unsupported)
    );
    assert_eq!(icon_set::by_name("test-frame:no-svg"), None);
}

#[cfg(feature = "svg")]
mod with_svg {
    use egui::{Color32, Pos2, Rect, TextureId, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use egui_pop_core::icon::Icon;
    use egui_pop_core::icon_set::{self, Paint};
    use egui_pop_core::placeholder::EmptyState;
    use egui_pop_core::theme::{self, ThemeSpec};

    const RING: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="black" stroke-width="2"/></svg>"#;
    const LOGO: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#3070c0"/></svg>"##;

    /// An image drawn this frame from an icon's texture.
    #[derive(Debug, PartialEq)]
    struct Image {
        /// The texture's name, `egui_pop.icon.<name>.<px>`.
        texture: String,
        rect: Rect,
        tint: Color32,
    }

    fn images(h: &Harness<'_>) -> Vec<Image> {
        let tex = h.ctx.tex_manager();
        let tex = tex.read();
        h.output()
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Mesh(m) if m.texture_id != TextureId::default() => {
                    let name = tex.meta(m.texture_id)?.name.clone();
                    name.starts_with("egui_pop.icon.").then(|| Image {
                        texture: name,
                        rect: m.calc_bounds(),
                        tint: m.vertices.first().map_or(Color32::TRANSPARENT, |v| v.color),
                    })
                }
                _ => None,
            })
            .collect()
    }

    fn square(at: f32, side: f32) -> Rect {
        Rect::from_min_size(Pos2::new(at, 10.0), vec2(side, side))
    }

    #[test]
    fn a_mono_icon_is_tinted_with_the_colour_given_and_a_colour_icon_drawn_as_it_is() {
        let ring = icon_set::register("test-frame:ring", RING, Paint::Mono).unwrap();
        let logo = icon_set::register("test-frame:logo", LOGO, Paint::Colour).unwrap();
        let mut h = Harness::builder()
            .with_size(vec2(200.0, 80.0))
            .build_ui(move |ui| {
                ring.paint(ui.painter(), square(10.0, 24.0), Color32::RED);
                logo.paint(ui.painter(), square(50.0, 24.0), Color32::RED);
            });
        theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
        h.run();
        let drawn = images(&h);
        assert_eq!(drawn.len(), 2, "one image each: {drawn:?}");
        assert!(
            drawn[0]
                .texture
                .starts_with("egui_pop.icon.test-frame:ring.")
        );
        assert_eq!(drawn[0].tint, Color32::RED, "a mask tinted");
        assert_eq!(drawn[1].tint, Color32::WHITE, "its own colours");
        assert_eq!(drawn[0].rect, square(10.0, 24.0), "at its pixel size");
    }

    #[test]
    fn an_icon_at_another_size_is_rasterised_at_that_size_not_scaled() {
        let ring = icon_set::register("test-frame:odd", RING, Paint::Mono).unwrap();
        let mut h = Harness::builder()
            .with_size(vec2(200.0, 80.0))
            .with_pixels_per_point(2.0)
            .build_ui(move |ui| ring.paint(ui.painter(), square(10.0, 15.0), Color32::WHITE));
        theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
        h.run();
        let drawn = images(&h);
        assert_eq!(drawn.len(), 1, "{drawn:?}");
        assert_eq!(
            drawn[0].texture, "egui_pop.icon.test-frame:odd.30",
            "15 points at 2 pixels per point"
        );
        assert_eq!(drawn[0].rect.size(), vec2(15.0, 15.0));
    }

    #[test]
    fn a_name_is_drawn_from_the_next_install_and_stops_at_once_when_unregistered() {
        let mut h = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .build_ui(|ui| {
                EmptyState::new("test-frame:late", "Nothing yet").show(ui);
            });
        theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
        h.run();
        assert!(
            images(&h).is_empty(),
            "nobody registered it: the placeholder"
        );

        icon_set::register("test-frame:late", RING, Paint::Mono).unwrap();
        h.run();
        assert!(
            images(&h).is_empty(),
            "registered after the theme's install: not drawn yet"
        );
        theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
        h.run();
        let drawn = images(&h);
        assert_eq!(drawn.len(), 1, "{drawn:?}");
        assert!(
            drawn[0]
                .texture
                .starts_with("egui_pop.icon.test-frame:late.")
        );

        assert!(icon_set::unregister("test-frame:late"));
        h.run();
        assert!(images(&h).is_empty(), "unregistered: the placeholder again");
        assert_eq!(Icon::by_name("test-frame:late"), None);
        assert!(h.query_by_label("Nothing yet").is_some());
    }
}
