//! The file browser's thumbnail contract in the library's views: one
//! [`ThumbnailProvider`] serves the browser, the picker, the grid, the
//! filmstrip and the comparison.
//!
//! [`ProviderThumbs`] is a [`ThumbSource`] over a provider, for one
//! frame: made before the views are drawn (it begins the provider's
//! frame), dropped after (it ends it, so what scrolled away leaves the
//! provider's queue). The application says which file each key is.
//!
//! ```no_run
//! use egui_pop_api::files::browser::{SourceEntry, ThumbnailProvider};
//! use egui_pop_views::library::thumbs::ProviderThumbs;
//!
//! fn entry_of(key: &u64) -> Option<SourceEntry> { todo!() }
//!
//! fn ui(ui: &mut egui::Ui, provider: &dyn ThumbnailProvider) {
//!     let thumbs = ProviderThumbs::begin(ui.ctx(), provider, entry_of);
//!     // ThumbGrid::show(ui, state, items, &thumbs) …
//!     drop(thumbs); // ends the provider's frame
//! }
//! ```
//!
//! Its badges meet the browser's the same way: a [`Badge`] is a
//! [`MarkChip`] (`MarkChip::from(badge)`), both drawn as indicators.
//! So does its zoom: a grid's [`Zoom`] and the browser's and picker's
//! [`ThumbZoom`] are the same count per row, converted with `From`.

use egui::Context;
use egui_pop_api::files::browser::{
    MarkChip, Priority, SourceEntry, ThumbRequest, ThumbState, ThumbZoom, ThumbnailProvider,
};

use super::grid::{Badge, Thumb, ThumbSource, Zoom};

/// The browser's zoom as a grid's: the same count per row, or the same
/// size in points still to convert.
impl From<ThumbZoom> for Zoom {
    fn from(zoom: ThumbZoom) -> Zoom {
        match zoom {
            ThumbZoom::PerRow(n) => Zoom::PerRow(n),
            ThumbZoom::Points(p) => Zoom::Points(p),
        }
    }
}

/// A grid's zoom as the browser's.
impl From<Zoom> for ThumbZoom {
    fn from(zoom: Zoom) -> ThumbZoom {
        match zoom {
            Zoom::PerRow(n) => ThumbZoom::PerRow(n),
            Zoom::Points(p) => ThumbZoom::Points(p),
        }
    }
}

/// A thumbnail as a provider has it, as the library draws it. A file
/// with no image of its own ([`ThumbState::None`], a folder) and one
/// that failed both show the grid's warning sign: the library shows
/// images.
impl From<&ThumbState> for Thumb {
    fn from(state: &ThumbState) -> Thumb {
        match state {
            ThumbState::Ready(texture) => {
                let [w, h] = texture.size();
                Thumb::Ready {
                    texture: texture.id(),
                    size: [
                        u32::try_from(w).unwrap_or(u32::MAX),
                        u32::try_from(h).unwrap_or(u32::MAX),
                    ],
                }
            }
            ThumbState::Loading => Thumb::Pending { average: None },
            ThumbState::None | ThumbState::Failed(_) => Thumb::Failed,
        }
    }
}

/// A library badge as the file browser's chip: its text, named by its
/// text, of its tone, in the bottom right corner, where the library
/// draws its badges.
impl From<Badge> for MarkChip {
    fn from(badge: Badge) -> MarkChip {
        let name = badge.said().to_owned();
        match badge.icon {
            Some(icon) => MarkChip::icon(icon, name),
            None => MarkChip::text(badge.text, name),
        }
        .tone(badge.tone)
    }
}

/// Which file a key is ([`ProviderThumbs::begin`]).
type EntryOf<'a, K> = dyn Fn(&K) -> Option<SourceEntry> + 'a;

/// A [`ThumbSource`] over a [`ThumbnailProvider`] for one frame: made
/// with [`ProviderThumbs::begin`], which begins the provider's frame,
/// and dropped once the views are drawn, which ends it.
pub struct ProviderThumbs<'a, K> {
    ctx: Context,
    provider: &'a dyn ThumbnailProvider,
    entry: Box<EntryOf<'a, K>>,
}

impl<K> std::fmt::Debug for ProviderThumbs<'_, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderThumbs").finish_non_exhaustive()
    }
}

impl<'a, K> ProviderThumbs<'a, K> {
    /// Begins `provider`'s frame. `entry` says which file a key is; a
    /// key it does not know shows the warning sign.
    pub fn begin(
        ctx: &Context,
        provider: &'a dyn ThumbnailProvider,
        entry: impl Fn(&K) -> Option<SourceEntry> + 'a,
    ) -> Self {
        provider.begin_frame(ctx);
        Self {
            ctx: ctx.clone(),
            provider,
            entry: Box::new(entry),
        }
    }

    fn ask(&self, key: &K, size: u32, priority: Priority) -> Thumb {
        match (self.entry)(key) {
            Some(entry) => Thumb::from(&self.provider.request(
                &self.ctx,
                &ThumbRequest {
                    entry: &entry,
                    size,
                    priority,
                },
            )),
            None => Thumb::Failed,
        }
    }
}

impl<K> ThumbSource<K> for ProviderThumbs<'_, K> {
    /// Asks the provider, as on screen now.
    fn thumb(&self, key: &K, size: u32) -> Thumb {
        self.ask(key, size, Priority::Visible)
    }

    /// The images in view, already asked for by [`Self::thumb`]: asked
    /// again, so that a view telling only what it wants keeps them in
    /// the provider's queue this frame.
    fn wanted(&self, keys: &[K], size: u32) {
        for key in keys {
            let _ = self.ask(key, size, Priority::Visible);
        }
    }
}

impl<K> Drop for ProviderThumbs<'_, K> {
    fn drop(&mut self) {
        self.provider.end_frame(&self.ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::grid;
    use egui_pop_api::files::browser::EntryKind;
    use egui_pop_core::kit::Tone;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);

    impl ThumbnailProvider for Log {
        fn begin_frame(&self, _ctx: &Context) {
            self.0.lock().unwrap().push("begin".into());
        }
        fn request(&self, _ctx: &Context, request: &ThumbRequest<'_>) -> ThumbState {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} {}", request.entry.name, request.size));
            if request.entry.name == "broken.png" {
                ThumbState::Failed("no".into())
            } else {
                ThumbState::Loading
            }
        }
        fn end_frame(&self, _ctx: &Context) {
            self.0.lock().unwrap().push("end".into());
        }
        fn cancel(&self, _path: &Path) {}
    }

    fn entry(name: &str) -> SourceEntry {
        SourceEntry {
            path: PathBuf::from("/shoot").join(name),
            name: name.into(),
            kind: EntryKind::File,
            size: Some(1),
            modified: None,
            taken: None,
            app_id: None,
        }
    }

    #[test]
    fn a_frame_asks_the_provider_between_its_begin_and_end() {
        let ctx = Context::default();
        let log = Log::default();
        let names = ["a.png", "broken.png"];
        {
            let thumbs =
                ProviderThumbs::begin(&ctx, &log, |k: &usize| names.get(*k).map(|n| entry(n)));
            assert_eq!(thumbs.thumb(&0, 96), Thumb::Pending { average: None });
            assert_eq!(thumbs.thumb(&1, 96), Thumb::Failed);
            // A key the application does not know.
            assert_eq!(thumbs.thumb(&7, 96), Thumb::Failed);
        }
        assert_eq!(
            *log.0.lock().unwrap(),
            ["begin", "a.png 96", "broken.png 96", "end"]
        );
    }

    #[test]
    fn a_badge_is_a_bottom_right_chip_of_its_tone() {
        let chip = MarkChip::from(Badge::new("RAW").tone(Tone::Info));
        assert_eq!(chip, MarkChip::text("RAW", "RAW").tone(Tone::Info));
    }

    #[test]
    fn an_icon_badge_is_a_top_right_chip_under_its_name() {
        let chip = MarkChip::from(Badge::icon("phosphor:plugs", "offline").tone(Tone::Warning));
        assert_eq!(
            chip,
            MarkChip::icon("phosphor:plugs", "offline").tone(Tone::Warning)
        );
        let pair = MarkChip::from(Badge::new("R+J").named("RAW+JPG"));
        assert_eq!(pair, MarkChip::text("R+J", "RAW+JPG"));
    }

    #[test]
    fn the_browser_zoom_and_the_grid_zoom_are_one_count_per_row() {
        assert_eq!(Zoom::from(ThumbZoom::PerRow(1)), Zoom::PerRow(1));
        assert_eq!(ThumbZoom::from(Zoom::PerRow(8)), ThumbZoom::PerRow(8));
        assert_eq!(Zoom::from(ThumbZoom::Points(128.0)), Zoom::Points(128.0));
        assert_eq!(
            ThumbZoom::from(Zoom::default()),
            ThumbZoom::PerRow(grid::DEFAULT_PER_ROW)
        );
    }
}
