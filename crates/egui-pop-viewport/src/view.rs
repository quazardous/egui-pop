//! The view model: what part of the image is shown, at what zoom, and the
//! one mapping between image and screen that drawing, hit testing and the
//! pixel readout all use.
//!
//! - **Zoom is physical**: [`View::zoom`] counts physical screen pixels
//!   per image pixel, so 100 % is exactly `1.0` and means one image pixel
//!   on one pixel of the window's surface, whatever the window's scale.
//!   Each window converts with its own `pixels_per_point`, so the same
//!   view shown in two windows is 1:1 in both.
//! - **Whole-pixel offsets**: [`Mapping`] snaps the image's origin to a
//!   whole physical pixel, so at 100 %, at whole zooms (200 %, 300 %) and
//!   at their inverses no column is repeated or skipped.
//! - **The ladder**: [`zoom_in`] and [`zoom_out`] walk [`ZOOM_LADDER`];
//!   the wheel ([`wheel_zoom`]) is continuous but stops at 100 % when it
//!   crosses it, so exact pixels are always one notch away.
//!
//! ```
//! use egui_pop_viewport::view::{Mapping, View};
//!
//! let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(400.0, 300.0));
//! // 100 % on a window at 150 %: one image pixel per physical pixel.
//! let view = View::actual([500.0, 400.0]);
//! let m = Mapping::new(&view, [1000.0, 800.0], rect, 1.5);
//! assert_eq!(m.zoom, 1.0);
//! let p = m.image_to_screen([500.0, 400.0]);
//! assert_eq!(m.pixel_at(p), Some([500, 400]));
//! ```

use egui::{Pos2, Rect, Vec2, pos2};

/// The zoom steps of [`zoom_in`] and [`zoom_out`], in physical pixels
/// per image pixel: whole ratios first (100 % is `1.0`).
pub const ZOOM_LADDER: &[f64] = &[
    1.0 / 16.0,
    1.0 / 12.0,
    1.0 / 8.0,
    1.0 / 6.0,
    1.0 / 4.0,
    1.0 / 3.0,
    1.0 / 2.0,
    2.0 / 3.0,
    1.0,
    2.0,
    3.0,
    4.0,
    8.0,
    16.0,
    32.0,
];

/// The smallest zoom (a very large image fitted in a small view).
pub const MIN_ZOOM: f64 = 1.0 / 256.0;
/// The largest zoom.
pub const MAX_ZOOM: f64 = 64.0;

/// How the zoom is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// The whole image in the view, centred; follows the view's size.
    Fit,
    /// Physical screen pixels per image pixel (100 % is `1.0`).
    Px(f64),
}

/// What is shown: a zoom and the image point at the centre of the view.
/// The application owns it (a window, a document); the viewport changes
/// it through gestures and says so with signals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: Zoom,
    /// The image point at the centre of the view, in image pixels
    /// (ignored while fitting).
    pub center: [f64; 2],
}

impl Default for View {
    fn default() -> Self {
        View::fit()
    }
}

impl View {
    /// The whole image, centred.
    pub fn fit() -> View {
        View {
            zoom: Zoom::Fit,
            center: [0.0, 0.0],
        }
    }

    /// 100 %: one image pixel per physical pixel, around `center`.
    pub fn actual(center: [f64; 2]) -> View {
        View {
            zoom: Zoom::Px(1.0),
            center,
        }
    }

    /// A zoom (clamped to [`MIN_ZOOM`]..[`MAX_ZOOM`]) around `center`.
    pub fn at(zoom: f64, center: [f64; 2]) -> View {
        View {
            zoom: Zoom::Px(zoom.clamp(MIN_ZOOM, MAX_ZOOM)),
            center,
        }
    }

    /// Whether the view follows the window (fit).
    pub fn is_fit(&self) -> bool {
        self.zoom == Zoom::Fit
    }

    /// The same view with fit resolved against a mapping (what is shown,
    /// as explicit numbers).
    pub fn resolved(&self, m: &Mapping) -> View {
        View::at(m.zoom, m.center)
    }
}

/// The zoom that shows a whole image in a view of `rect_px` physical
/// pixels.
pub fn fit_zoom(image: [f64; 2], rect_px: Vec2) -> f64 {
    if image[0] <= 0.0 || image[1] <= 0.0 {
        return 1.0;
    }
    (f64::from(rect_px.x) / image[0])
        .min(f64::from(rect_px.y) / image[1])
        .clamp(MIN_ZOOM, MAX_ZOOM)
}

const EPS: f64 = 1e-9;

/// The next ladder step above `zoom` (or [`MAX_ZOOM`]'s step).
pub fn zoom_in(zoom: f64) -> f64 {
    ZOOM_LADDER
        .iter()
        .copied()
        .find(|z| *z > zoom * (1.0 + EPS))
        .unwrap_or(ZOOM_LADDER[ZOOM_LADDER.len() - 1].max(zoom))
}

/// The next ladder step below `zoom` (or the smallest step).
pub fn zoom_out(zoom: f64) -> f64 {
    ZOOM_LADDER
        .iter()
        .rev()
        .copied()
        .find(|z| *z < zoom * (1.0 - EPS))
        .unwrap_or(ZOOM_LADDER[0].min(zoom))
}

/// How fast the wheel zooms: the factor per point of scroll is
/// `exp(points × WHEEL_RATE)`.
pub const WHEEL_RATE: f64 = 0.0015;

/// The zoom after a wheel motion of `points` (positive zooms in): a
/// continuous factor that stops at exactly 100 % when it crosses it.
pub fn wheel_zoom(zoom: f64, points: f32) -> f64 {
    let next = (zoom * (f64::from(points) * WHEEL_RATE).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
    let crosses = (zoom < 1.0 - EPS && next > 1.0) || (zoom > 1.0 + EPS && next < 1.0);
    if crosses { 1.0 } else { next }
}

/// The mapping between image pixels, physical pixels and points for one
/// frame of one viewport in one window. Build it with [`Mapping::new`];
/// every conversion of the viewport (drawing, tiles, hit tests, the
/// readout) goes through it.
///
/// Physical pixels are counted from the window's top left corner, as the
/// GPU draws them; a physical pixel `n` covers `n..n+1` and its centre is
/// `n + 0.5`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// The viewport, in points.
    pub rect: Rect,
    /// Physical pixels per point of the window drawing it.
    pub ppp: f32,
    /// Physical pixels per image pixel (fit resolved).
    pub zoom: f64,
    /// Where image point `(0, 0)` lands, in physical pixels: whole
    /// numbers.
    pub origin: [f64; 2],
    /// The image point at the centre of the view.
    pub center: [f64; 2],
    /// The image's size in pixels.
    pub image: [f64; 2],
    /// The viewport in physical pixels, rounded: `[x0, y0, x1, y1]`.
    pub rect_px: [f64; 4],
}

impl Mapping {
    /// The mapping of a view of an image (`image`, its size in pixels)
    /// into `rect` (points) on a window with `ppp` physical pixels per
    /// point.
    pub fn new(view: &View, image: [f64; 2], rect: Rect, ppp: f32) -> Mapping {
        let p = f64::from(ppp);
        let rect_px = [
            (f64::from(rect.min.x) * p).round(),
            (f64::from(rect.min.y) * p).round(),
            (f64::from(rect.max.x) * p).round(),
            (f64::from(rect.max.y) * p).round(),
        ];
        let size = Vec2::new(
            (rect_px[2] - rect_px[0]) as f32,
            (rect_px[3] - rect_px[1]) as f32,
        );
        let (zoom, center) = match view.zoom {
            Zoom::Fit => (fit_zoom(image, size), [image[0] / 2.0, image[1] / 2.0]),
            Zoom::Px(z) => (z.clamp(MIN_ZOOM, MAX_ZOOM), view.center),
        };
        let mid = [
            (rect_px[0] + rect_px[2]) / 2.0,
            (rect_px[1] + rect_px[3]) / 2.0,
        ];
        let origin = [
            (mid[0] - center[0] * zoom).round(),
            (mid[1] - center[1] * zoom).round(),
        ];
        Mapping {
            rect,
            ppp,
            zoom,
            origin,
            center,
            image,
            rect_px,
        }
    }

    /// An image point in physical pixels.
    pub fn image_to_px(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.origin[0] + p[0] * self.zoom,
            self.origin[1] + p[1] * self.zoom,
        ]
    }

    /// A physical position in image pixels.
    pub fn px_to_image(&self, px: [f64; 2]) -> [f64; 2] {
        [
            (px[0] - self.origin[0]) / self.zoom,
            (px[1] - self.origin[1]) / self.zoom,
        ]
    }

    /// An image point on screen, in points.
    pub fn image_to_screen(&self, p: [f64; 2]) -> Pos2 {
        let px = self.image_to_px(p);
        let s = f64::from(self.ppp);
        pos2((px[0] / s) as f32, (px[1] / s) as f32)
    }

    /// A screen position (points) in image pixels, continuous.
    pub fn screen_to_image(&self, pos: Pos2) -> [f64; 2] {
        let s = f64::from(self.ppp);
        self.px_to_image([f64::from(pos.x) * s, f64::from(pos.y) * s])
    }

    /// A rectangle of the image (`[x0, y0, x1, y1]`, image pixels) on
    /// screen, in points.
    pub fn image_rect_to_screen(&self, r: [f64; 4]) -> Rect {
        Rect::from_min_max(
            self.image_to_screen([r[0], r[1]]),
            self.image_to_screen([r[2], r[3]]),
        )
    }

    /// The image pixel drawn on the physical pixel under `pos` (points),
    /// or `None` outside the image. This is the drawing's own rule: the
    /// physical pixel's centre, mapped to the image, floored. The pixel
    /// readout and the tests use it.
    pub fn pixel_at(&self, pos: Pos2) -> Option<[i64; 2]> {
        let s = f64::from(self.ppp);
        let n = [
            (f64::from(pos.x) * s).floor() + 0.5,
            (f64::from(pos.y) * s).floor() + 0.5,
        ];
        let p = self.px_to_image(n);
        let (x, y) = (p[0].floor(), p[1].floor());
        (x >= 0.0 && y >= 0.0 && x < self.image[0] && y < self.image[1])
            .then_some([x as i64, y as i64])
    }

    /// The part of the image in view, clipped to the image:
    /// `[x0, y0, x1, y1]` in image pixels, or `None` when nothing of it is
    /// visible.
    pub fn visible(&self) -> Option<[f64; 4]> {
        let a = self.px_to_image([self.rect_px[0], self.rect_px[1]]);
        let b = self.px_to_image([self.rect_px[2], self.rect_px[3]]);
        let r = [
            a[0].max(0.0),
            a[1].max(0.0),
            b[0].min(self.image[0]),
            b[1].min(self.image[1]),
        ];
        (r[2] > r[0] && r[3] > r[1]).then_some(r)
    }

    /// The view centred so that image point `at` stays under the screen
    /// position `pos` at another zoom (zooming around the pointer).
    pub fn zoom_around(&self, pos: Pos2, zoom: f64) -> View {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let s = f64::from(self.ppp);
        let at = self.screen_to_image(pos);
        let mid = [
            (self.rect_px[0] + self.rect_px[2]) / 2.0,
            (self.rect_px[1] + self.rect_px[3]) / 2.0,
        ];
        let offset = [f64::from(pos.x) * s - mid[0], f64::from(pos.y) * s - mid[1]];
        View::at(zoom, [at[0] - offset[0] / zoom, at[1] - offset[1] / zoom])
    }

    /// The view moved by a drag of `delta` points (the image follows the
    /// pointer).
    pub fn panned(&self, delta: Vec2) -> View {
        let s = f64::from(self.ppp);
        View::at(
            self.zoom,
            [
                self.center[0] - f64::from(delta.x) * s / self.zoom,
                self.center[1] - f64::from(delta.y) * s / self.zoom,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

    fn rect() -> Rect {
        Rect::from_min_size(pos2(13.0, 7.0), Vec2::new(301.0, 203.0))
    }

    #[test]
    fn hundred_percent_maps_each_physical_column_to_one_image_column() {
        for ppp in SCALES {
            for center in [[100.0, 80.0], [100.5, 80.25], [100.37, 79.91]] {
                let m = Mapping::new(&View::actual(center), [400.0, 300.0], rect(), ppp);
                assert_eq!(m.zoom, 1.0);
                assert_eq!(m.origin, [m.origin[0].round(), m.origin[1].round()]);
                let mut last = None;
                for n in (m.rect_px[0] as i64)..(m.rect_px[2] as i64) {
                    let pos = pos2(((n as f64 + 0.5) / f64::from(ppp)) as f32, 100.0);
                    let Some([x, _]) = m.pixel_at(pos) else {
                        continue;
                    };
                    if let Some(l) = last {
                        assert_eq!(x, l + 1, "ppp {ppp}, column {n}");
                    }
                    last = Some(x);
                }
                assert!(last.is_some());
            }
        }
    }

    #[test]
    fn whole_zooms_repeat_each_image_column_exactly_n_times() {
        for ppp in SCALES {
            for zoom in [2.0, 3.0, 4.0] {
                let m = Mapping::new(&View::at(zoom, [50.3, 40.7]), [400.0, 300.0], rect(), ppp);
                let mut runs: Vec<(i64, u32)> = Vec::new();
                for n in (m.rect_px[0] as i64)..(m.rect_px[2] as i64) {
                    let pos = pos2(((n as f64 + 0.5) / f64::from(ppp)) as f32, 100.0);
                    let Some([x, _]) = m.pixel_at(pos) else {
                        continue;
                    };
                    match runs.last_mut() {
                        Some((l, c)) if *l == x => *c += 1,
                        _ => runs.push((x, 1)),
                    }
                }
                // The first and last runs may be cut by the view's edges.
                for (_, c) in &runs[1..runs.len() - 1] {
                    assert_eq!(f64::from(*c), zoom, "ppp {ppp}");
                }
            }
        }
    }

    #[test]
    fn image_and_screen_round_trip() {
        for ppp in SCALES {
            let m = Mapping::new(&View::at(0.37, [123.0, 45.0]), [800.0, 600.0], rect(), ppp);
            let p = [321.25, 456.5];
            let back = m.screen_to_image(m.image_to_screen(p));
            assert!((back[0] - p[0]).abs() < 1e-3 && (back[1] - p[1]).abs() < 1e-3);
        }
    }

    #[test]
    fn fit_shows_the_whole_image_centred() {
        let m = Mapping::new(&View::fit(), [1000.0, 500.0], rect(), 2.0);
        assert!((m.zoom - 602.0 / 1000.0).abs() < 1e-9);
        let v = m.visible().unwrap();
        assert_eq!(v, [0.0, 0.0, 1000.0, 500.0]);
    }

    #[test]
    fn the_ladder_walks_whole_ratios_and_the_wheel_stops_at_100() {
        assert_eq!(zoom_in(1.0), 2.0);
        assert_eq!(zoom_out(1.0), 2.0 / 3.0);
        assert_eq!(zoom_in(0.9), 1.0);
        assert_eq!(zoom_out(1.1), 1.0);
        assert_eq!(zoom_in(32.0), 32.0);
        assert_eq!(zoom_out(1.0 / 16.0), 1.0 / 16.0);
        // Crossing 100 % stops on it, from either side.
        assert_eq!(wheel_zoom(0.9, 200.0), 1.0);
        assert_eq!(wheel_zoom(1.2, -200.0), 1.0);
        // From 100 %, the wheel leaves it.
        assert!(wheel_zoom(1.0, 50.0) > 1.0);
        assert!(wheel_zoom(1.0, -50.0) < 1.0);
    }

    #[test]
    fn zooming_around_a_point_keeps_it_under_the_pointer() {
        for ppp in SCALES {
            let m = Mapping::new(&View::at(0.5, [200.0, 150.0]), [400.0, 300.0], rect(), ppp);
            let pos = pos2(100.0, 60.0);
            let at = m.screen_to_image(pos);
            let v = m.zoom_around(pos, 2.0);
            let m2 = Mapping::new(&v, [400.0, 300.0], rect(), ppp);
            let back = m2.screen_to_image(pos);
            // Within the origin's snap of half a physical pixel.
            assert!((back[0] - at[0]).abs() <= 0.5 / 2.0 + 1e-9);
            assert!((back[1] - at[1]).abs() <= 0.5 / 2.0 + 1e-9);
        }
    }
}
