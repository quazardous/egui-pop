//! Snapshots cut to what they test (docs/testing.md, "Snapshots").
//!
//! The gallery draws a page inside its bars: the top bar, the side bar,
//! the status bar. A shot of the whole window changes with any of them, so
//! a new label in the top bar would rewrite every page's picture. A shot is
//! cut instead:
//!
//! - [`Shots::page_shot`]: the page's region ([`Nav::region`]: the page and
//!   its playground), for a page's own content;
//! - [`Shots::body_shot`]: everything under the top bar, for a page that
//!   drives the side bar, the bottom area or the status bar;
//! - [`Shots::window_shot`]: the whole window, for what fills it or draws
//!   its frame, and for the bars' own suite;
//! - [`Shots::shot_of`]: a rectangle the test names.
//!
//! What floats over the window (a dialog, a menu, a tip, a toast, a
//! palette) and reaches out of the cut widens it: the picture holds all of
//! it. A sheet covering the whole window does not count.

use egui::{Order, Rect};
use egui_kittest::{Harness, SnapshotError, SnapshotResult};
use egui_pop_gallery::Gallery;
use egui_pop_gallery::shell::{NAV, Nav};

/// The cuts of a gallery window's picture.
pub trait Shots {
    /// The page's region, compared with the snapshot `name`.
    fn try_page_shot(&mut self, name: impl Into<String>) -> SnapshotResult;

    /// Everything under the top bar, compared with the snapshot `name`.
    fn try_body_shot(&mut self, name: impl Into<String>) -> SnapshotResult;

    /// The whole window, compared with the snapshot `name`.
    fn try_window_shot(&mut self, name: impl Into<String>) -> SnapshotResult;

    /// `rect` of the window, in points, compared with the snapshot `name`;
    /// nothing widens it.
    fn try_shot_of(&mut self, name: impl Into<String>, rect: Rect) -> SnapshotResult;

    /// As [`Shots::try_page_shot`], failing the test at once.
    #[track_caller]
    fn page_shot(&mut self, name: impl Into<String>) {
        passed(self.try_page_shot(name));
    }

    /// As [`Shots::try_body_shot`], failing the test at once.
    #[track_caller]
    fn body_shot(&mut self, name: impl Into<String>) {
        passed(self.try_body_shot(name));
    }

    /// As [`Shots::try_window_shot`], failing the test at once.
    #[track_caller]
    fn window_shot(&mut self, name: impl Into<String>) {
        passed(self.try_window_shot(name));
    }
}

/// Fails the test on a shot that differs.
#[track_caller]
pub fn passed(result: SnapshotResult) {
    if let Err(e) = result {
        panic!("{e}");
    }
}

impl Shots for Harness<'_, Gallery> {
    fn try_page_shot(&mut self, name: impl Into<String>) -> SnapshotResult {
        let cut = widened(&self.ctx, region(self));
        self.try_shot_of(name, cut)
    }

    fn try_body_shot(&mut self, name: impl Into<String>) -> SnapshotResult {
        let cut = widened(&self.ctx, bars(self).body);
        self.try_shot_of(name, cut)
    }

    fn try_window_shot(&mut self, name: impl Into<String>) -> SnapshotResult {
        let window = self.ctx.content_rect();
        self.try_shot_of(name, window)
    }

    fn try_shot_of(&mut self, name: impl Into<String>, rect: Rect) -> SnapshotResult {
        let image = self
            .render()
            .map_err(|err| SnapshotError::RenderError { err })?;
        let ppp = self.ctx.pixels_per_point();
        // Whole pixels, the same on every run: a layout's edge a hair
        // under or over a pixel's lands on that pixel.
        let px = |v: f32, most: u32| ((v * ppp).round().max(0.0) as u32).min(most);
        let (left, right) = (
            px(rect.left(), image.width()),
            px(rect.right(), image.width()),
        );
        let (top, bottom) = (
            px(rect.top(), image.height()),
            px(rect.bottom(), image.height()),
        );
        assert!(
            left < right && top < bottom,
            "nothing to show in {rect:?} of a {}x{} picture",
            image.width(),
            image.height()
        );
        let cut =
            image::imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image();
        egui_kittest::try_image_snapshot_options(&cut, name.into(), self.options())
    }
}

fn nav(h: &Harness<'_, Gallery>) -> std::sync::Arc<Nav> {
    h.state()
        .app
        .handle()
        .service(&NAV)
        .expect("the gallery's navigation")
}

/// The page's region ([`Nav::region`]).
pub fn region(h: &Harness<'_, Gallery>) -> Rect {
    nav(h)
        .region()
        .expect("the page is drawn: a shot comes after a frame")
}

/// The window around the page's region.
#[derive(Clone, Copy, Debug)]
pub struct Bars {
    /// Above the region, all of the window's width: the top bar.
    pub top: Rect,
    /// Left of the region, between the two: the activity bar and the side
    /// bar.
    pub side: Rect,
    /// Under the region, all of the window's width: the status bar.
    pub status: Rect,
    /// Everything under the top bar.
    pub body: Rect,
}

/// The gallery's bars, from where the page's region lies.
pub fn bars(h: &Harness<'_, Gallery>) -> Bars {
    let window = h.ctx.content_rect();
    let page = region(h);
    Bars {
        top: Rect::from_min_max(window.min, egui::pos2(window.right(), page.top())),
        side: Rect::from_min_max(
            egui::pos2(window.left(), page.top()),
            egui::pos2(page.left(), page.bottom()),
        ),
        status: Rect::from_min_max(egui::pos2(window.left(), page.bottom()), window.max),
        body: Rect::from_min_max(egui::pos2(window.left(), page.top()), window.max),
    }
}

/// `cut`, widened to hold what floats over the window this frame: every
/// area above the panels' layer, but one covering the whole window.
fn widened(ctx: &egui::Context, cut: Rect) -> Rect {
    let window = ctx.content_rect();
    let floating = ctx.memory(|m| {
        m.layer_ids()
            .filter(|layer| layer.order != Order::Background && m.areas().is_visible(layer))
            .filter_map(|layer| m.area_rect(layer.id))
            .filter(|rect| rect.is_positive() && !rect.contains_rect(window.shrink(1.0)))
            .reduce(Rect::union)
    });
    floating.map_or(cut, |f| cut.union(f)).intersect(window)
}
