//! An application's own icons: SVG files registered by name, drawn like
//! egui-pop's own ([`Icon::Custom`]) and nameable by a kit's glyphs
//! (`glyph.<role>: "<name>"`).
//!
//! An icon is single-colour ([`Paint::Mono`], tinted at draw with the
//! colour a widget gives, like the built-in icons) or full-colour
//! ([`Paint::Colour`], an application's logo, drawn as it is).
//!
//! Nothing is done while drawing: [`install`] (run with the theme,
//! `theme::install_tokens`) rasterises every registered icon at the icon
//! sizes of the theme (`size.icon.*`) and the window's pixels per point,
//! as textures; drawing picks the nearest size and paints one image.
//! A single-colour icon is a white mask, so one texture serves every
//! colour. Rasterising needs the `svg` feature (resvg); without it,
//! [`register`] refuses.

use std::sync::{Arc, RwLock};

use egui::{Color32, Context, Id, Painter, Rect, TextureHandle};

use crate::icon::Icon;
use crate::theme::Tokens;

/// How an icon is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    /// One colour, given at draw (its shape is the SVG's alpha).
    Mono,
    /// Its own colours.
    Colour,
}

/// An icon an application registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomIcon(u16);

/// Why an icon could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconError {
    /// The name is one of egui-pop's icons.
    Taken(String),
    /// The file is not an SVG resvg can read.
    Svg(String),
    /// Built without the `svg` feature.
    Unsupported,
}

impl std::fmt::Display for IconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IconError::Taken(n) => write!(f, "`{n}` is one of egui-pop's icons"),
            IconError::Svg(e) => write!(f, "not a readable SVG: {e}"),
            IconError::Unsupported => write!(f, "egui-pop was built without the `svg` feature"),
        }
    }
}

impl std::error::Error for IconError {}

struct Entry {
    name: &'static str,
    svg: Arc<[u8]>,
    paint: Paint,
}

/// The registered icons, for the whole process: an icon's name and id
/// are the same in every window.
static SET: RwLock<Vec<Entry>> = RwLock::new(Vec::new());

/// Registers an SVG icon under a name (kebab case, like `play-circle`),
/// or replaces the one registered under it. In use from the next theme
/// install. Each new name is kept for the whole process.
pub fn register(name: &str, svg: impl Into<Arc<[u8]>>, paint: Paint) -> Result<Icon, IconError> {
    if Icon::ALL.iter().any(|i| i.name() == name) {
        return Err(IconError::Taken(name.into()));
    }
    let svg = svg.into();
    check(&svg)?;
    let mut set = SET.write().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = set.iter().position(|e| e.name == name) {
        set[i].svg = svg;
        set[i].paint = paint;
        return Ok(Icon::Custom(CustomIcon(i as u16)));
    }
    let id = u16::try_from(set.len()).expect("fewer than 65536 icons");
    set.push(Entry {
        name: Box::leak(name.to_string().into_boxed_str()),
        svg,
        paint,
    });
    Ok(Icon::Custom(CustomIcon(id)))
}

/// A registered icon by name.
pub fn by_name(name: &str) -> Option<Icon> {
    let set = SET.read().unwrap_or_else(|e| e.into_inner());
    set.iter()
        .position(|e| e.name == name)
        .map(|i| Icon::Custom(CustomIcon(i as u16)))
}

/// Every registered icon's name, in registration order.
pub fn names() -> Vec<&'static str> {
    let set = SET.read().unwrap_or_else(|e| e.into_inner());
    set.iter().map(|e| e.name).collect()
}

pub(crate) fn name(icon: CustomIcon) -> &'static str {
    let set = SET.read().unwrap_or_else(|e| e.into_inner());
    set.get(usize::from(icon.0)).map_or("custom", |e| e.name)
}

#[cfg(feature = "svg")]
fn check(svg: &[u8]) -> Result<(), IconError> {
    resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .map(|_| ())
        .map_err(|e| IconError::Svg(e.to_string()))
}

#[cfg(not(feature = "svg"))]
fn check(_svg: &[u8]) -> Result<(), IconError> {
    Err(IconError::Unsupported)
}

/// One icon's textures, smallest first.
#[derive(Clone)]
struct Rasters {
    paint: Paint,
    /// (side in physical pixels, texture).
    sizes: Vec<(u32, TextureHandle)>,
}

#[derive(Clone, Default)]
struct Installed {
    ppp: f32,
    icons: Vec<Rasters>,
}

fn installed_id() -> Id {
    Id::new("egui_pop_core::icon_set::installed")
}

/// Rasterises every registered icon at the theme's icon sizes and the
/// context's pixels per point. Called by `theme::install_tokens`, between
/// frames; a no-op when nothing is registered.
pub fn install(ctx: &Context, tokens: &Tokens) {
    let set = SET.read().unwrap_or_else(|e| e.into_inner());
    if set.is_empty() {
        return;
    }
    let ppp = ctx.pixels_per_point();
    let mut sides: Vec<u32> = [
        tokens.size.icon_inline,
        tokens.size.icon_toolbar,
        tokens.size.icon_panel,
    ]
    .iter()
    .map(|s| (s * ppp).round().max(1.0) as u32)
    .collect();
    sides.sort_unstable();
    sides.dedup();
    let icons = set
        .iter()
        .map(|e| Rasters {
            paint: e.paint,
            sizes: sides
                .iter()
                .filter_map(|&px| {
                    let image = rasterise(&e.svg, px, e.paint)?;
                    let name = format!("egui_pop.icon.{}.{px}", e.name);
                    Some((
                        px,
                        ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
                    ))
                })
                .collect(),
        })
        .collect();
    ctx.data_mut(|d| d.insert_temp(installed_id(), Arc::new(Installed { ppp, icons })));
}

#[cfg(feature = "svg")]
fn rasterise(svg: &[u8], px: u32, paint: Paint) -> Option<egui::ColorImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let (dx, dy) = (
        (px as f32 - size.width() * scale) / 2.0,
        (px as f32 - size.height() * scale) / 2.0,
    );
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    if paint == Paint::Mono {
        // A white mask: premultiplied, white at alpha a is (a, a, a, a).
        for p in rgba.chunks_exact_mut(4) {
            let a = p[3];
            p.copy_from_slice(&[a, a, a, a]);
        }
    }
    Some(egui::ColorImage::from_rgba_premultiplied(
        [px as usize, px as usize],
        &rgba,
    ))
}

#[cfg(not(feature = "svg"))]
fn rasterise(_svg: &[u8], _px: u32, _paint: Paint) -> Option<egui::ColorImage> {
    None
}

/// Paints a registered icon in a square: the texture of the nearest
/// size at or above the square's, tinted with `color` when single-colour.
/// Nothing is drawn before the first install that saw it.
pub(crate) fn paint(painter: &Painter, icon: CustomIcon, rect: Rect, color: Color32) {
    let Some(installed) = painter
        .ctx()
        .data(|d| d.get_temp::<Arc<Installed>>(installed_id()))
    else {
        return;
    };
    let Some(r) = installed.icons.get(usize::from(icon.0)) else {
        return;
    };
    let want = (rect.width().min(rect.height()) * installed.ppp).round() as u32;
    let Some((px, texture)) = r
        .sizes
        .iter()
        .find(|(px, _)| *px >= want)
        .or_else(|| r.sizes.last())
    else {
        return;
    };
    // Drawn at its own pixel size when it fits, on the pixel grid: crisp.
    let side = if *px <= want + 1 {
        *px as f32 / installed.ppp
    } else {
        rect.width().min(rect.height())
    };
    let min = rect.center() - egui::Vec2::splat(side / 2.0);
    let min = egui::pos2(
        (min.x * installed.ppp).round() / installed.ppp,
        (min.y * installed.ppp).round() / installed.ppp,
    );
    let tint = match r.paint {
        Paint::Mono => color,
        Paint::Colour => Color32::WHITE.gamma_multiply(f32::from(color.a()) / 255.0),
    };
    painter.image(
        texture.id(),
        Rect::from_min_size(min, egui::Vec2::splat(side)),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        tint,
    );
}

#[cfg(all(test, feature = "svg"))]
mod tests {
    use super::*;

    const RING: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="black" stroke-width="2"/></svg>"#;

    #[test]
    fn a_registered_icon_has_a_name_and_an_id_and_is_rasterised_as_a_mask() {
        let icon = register("test-ring", RING, Paint::Mono).unwrap();
        assert_eq!(by_name("test-ring"), Some(icon));
        assert_eq!(icon.name(), "test-ring");
        assert_eq!(Icon::by_name("test-ring"), Some(icon));
        let image = rasterise(RING, 24, Paint::Mono).unwrap();
        // The ring's top edge is drawn, white at some alpha; the centre
        // is empty.
        let top = image[(12, 3)];
        assert!(top.a() > 100 && top.r() == top.a(), "{top:?}");
        assert_eq!(image[(12, 12)].a(), 0);
    }

    #[test]
    fn a_built_in_name_or_a_broken_file_is_refused() {
        assert_eq!(
            register("check", RING, Paint::Mono),
            Err(IconError::Taken("check".into()))
        );
        assert!(matches!(
            register("broken", &b"<svg"[..], Paint::Mono),
            Err(IconError::Svg(_))
        ));
    }

    #[test]
    fn a_kit_glyph_can_name_an_application_icon() {
        let ring = register("test-kit-ring", RING, Paint::Mono).unwrap();
        let mut themes = egui_pop_tokens::Themes::default();
        themes
            .add_text(
                "ring-check",
                r#"
                $extends: "pop-dark"
                glyph:
                  check: {$type: "string", $value: "test-kit-ring"}
                "#,
            )
            .unwrap();
        let t = Tokens::new(themes.resolve(&egui_pop_tokens::ThemeSpec::new("ring-check")));
        let kit = crate::kit::Kit::compile(&t, &crate::kit::Shapes::default());
        assert_eq!(kit.glyph(crate::kit::Glyph::Check), ring);
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    }

    #[test]
    fn install_makes_one_texture_per_icon_size() {
        register("test-dot", RING, Paint::Colour).unwrap();
        let ctx = Context::default();
        install(&ctx, &Tokens::default());
        let installed = ctx
            .data(|d| d.get_temp::<Arc<Installed>>(installed_id()))
            .unwrap();
        let t = Tokens::default();
        let mut sides = vec![t.size.icon_inline, t.size.icon_toolbar, t.size.icon_panel];
        sides.dedup();
        let at = names().iter().position(|n| *n == "test-dot").unwrap();
        assert_eq!(installed.icons[at].sizes.len(), sides.len());
    }
}
