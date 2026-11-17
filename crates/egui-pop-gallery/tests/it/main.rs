//! The gallery's integration tests, one test executable: each module was
//! a test target of its own, each linking egui, wgpu and the whole gallery.
//! Linked once, they build in a fraction of the time and the disk.
//!
//! One module alone: `cargo nextest run -p egui-pop-gallery --test it -E
//! 'test(/^keyboard_walk::/)'`, or `cargo test -p egui-pop-gallery --test it
//! keyboard_walk::`. Under plain `cargo test` every module's tests share one
//! process (docs/testing.md, "One test executable").

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
