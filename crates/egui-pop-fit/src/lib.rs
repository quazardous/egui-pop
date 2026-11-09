//! # egui_gridflow
//!
//! A slot layout engine over plain boxes. It knows nothing of widgets,
//! values or drawing: it packs ordered elements into a container in
//! standard slot sizes, and says where each one goes.
//!
//! It has no egui dependency: the `egui_` prefix only marks it as part of
//! the egui-pop family. Results are plain rectangles, in twelfths of the
//! unit and in points.
//!
//! - **Base unit.** One row, scaled by density, given in points by the
//!   caller ([`Container::unit`]).
//! - **Allowed sizes.** ¼, ⅓, ½, ⅔, ¾ of the unit and whole multiples
//!   ([`Size`]), counted in twelfths with integer arithmetic only.
//! - **Elements.** Each declares forms (form 0 is the full form, the others
//!   smaller ones), each with a [`SizePolicy`]; its supported orientations,
//!   preferred first; an optional pinned orientation and pinned position.
//! - **Packing.** A grid as wide as the box, in twelfths, with rows going
//!   down. Pinned elements are placed first, then the others in their order,
//!   each at the first free position in reading order: a later element may
//!   move up into an earlier gap (like CSS `grid-auto-flow: dense`).
//! - **Growth.** Growable elements then take the leftover room one slot step
//!   at a time, in rounds, up to their maximum.
//! - **Hysteresis.** Pass the previous [`Layout`] to keep its column count,
//!   forms and orientations while the box stays within half a unit of a
//!   threshold.
//!
//! ```
//! use egui_gridflow::{layout, Axis, Container, Element, Size, Size2, SizePolicy};
//!
//! let half = Size2::new(Size::HALF, Size::ONE);
//! let row = Size2::new(Size::units(2), Size::ONE);
//! let elements = [
//!     Element::new(1, SizePolicy::fixed(half)),
//!     Element::new(2, SizePolicy::fixed(row)),
//!     Element::new(3, SizePolicy::growable(half, half, row, Axis::Width)),
//! ];
//! // Two units wide (48 points, 24-point unit), growing down. `b` does not
//! // fit beside `a`, so `c` moves up into the gap, then grows in slot steps
//! // (½, ⅔, ¾, 1): the next step, 2, would not fit, so ½ unit stays empty.
//! let result = layout(&Container::new(48.0, 24.0), &elements, None);
//! assert_eq!(
//!     result.dump(),
//!     "grid 24x24 twelfths, one char = 6x12\n\
//!      acc.\n\
//!      bbbb\n\
//!      a key=1 form=0 horizontal at 0,0 size 6x12\n\
//!      b key=2 form=0 horizontal at 0,12 size 24x12\n\
//!      c key=3 form=0 horizontal at 6,0 size 12x12\n",
//! );
//! ```
#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod dump;
mod element;
mod layout;
mod size;

pub use element::{AspectRatio, Axis, Cell, Element, Grow, Orientation, Size2, SizePolicy};
pub use layout::{layout, Container, GridRect, Layout, Placement, PointRect, Reason, Status};
pub use size::{InvalidSize, Size, TWELFTHS_PER_UNIT};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
