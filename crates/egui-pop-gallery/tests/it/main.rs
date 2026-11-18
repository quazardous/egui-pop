//! The gallery's integration tests, one test executable: each module was
//! a test target of its own, each linking egui, wgpu and the whole gallery.
//! Linked once, they build in a fraction of the time and the disk.
//!
//! One module alone: `cargo nextest run -p egui-pop-gallery --test it -E
//! 'test(/^keyboard_walk::/)'`, or `cargo test -p egui-pop-gallery --test it
//! keyboard_walk::`. Under plain `cargo test` every module's tests share one
//! process (docs/testing.md, "One test executable").

// `for_each_page!`: the pages of `PAGES`, written by build.rs.
include!(concat!(env!("OUT_DIR"), "/pages.rs"));

/// A module of tests, one per page of `PAGES`, each calling `$check(page)`:
/// `for_each_page!(per_page! { module, check })`. Each page's test runs
/// in a process of its own under nextest, so a slow page shows by name and
/// no test grows with the gallery; a page added to `PAGES` gets its test.
macro_rules! per_page {
    ({ $module:ident, $check:path } $($id:ident $page:literal;)*) => {
        mod $module {
            $(
                #[test]
                fn $id() {
                    $check($page);
                }
            )*
        }
    };
}

mod agent_actions;
mod default_fonts;
mod drag_cursors;
mod gallery;
mod gallery_doc;
mod hosts_doc;
mod inspect;
mod inspection;
mod keyboard_walk;
mod mounts;
mod preview;
mod rerun_like;
mod scenarios;
mod shot;
mod viewer_video;
mod youtube_referrer;
