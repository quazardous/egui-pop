//! What the caller declares: elements, their forms and size policies.

use crate::size::Size;
use core::fmt;

/// An orientation an element can be laid out in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Orientation {
    /// As declared: the policy's width is horizontal.
    Horizontal,
    /// Turned: the policy's width runs down, its height across.
    Vertical,
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Orientation::Horizontal => "horizontal",
            Orientation::Vertical => "vertical",
        })
    }
}

/// An axis of a size policy, in the policy's own (horizontal) terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Axis {
    /// Along the width.
    Width,
    /// Along the height.
    Height,
}

impl Axis {
    fn other(self) -> Axis {
        match self {
            Axis::Width => Axis::Height,
            Axis::Height => Axis::Width,
        }
    }
}

/// Whether an element keeps its size or takes leftover room.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Grow {
    /// The element keeps its preferred size.
    #[default]
    Fixed,
    /// The element grows along this axis, one slot step at a time, up to its
    /// maximum.
    Along(Axis),
}

/// A width-to-height ratio. A ratio with a zero term is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AspectRatio {
    /// The width term.
    pub width: u32,
    /// The height term.
    pub height: u32,
}

/// A size on both axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Size2 {
    /// Across.
    pub width: Size,
    /// Down.
    pub height: Size,
}

impl Size2 {
    /// A size from its width and height.
    pub const fn new(width: Size, height: Size) -> Size2 {
        Size2 { width, height }
    }

    pub(crate) fn get(self, axis: Axis) -> Size {
        match axis {
            Axis::Width => self.width,
            Axis::Height => self.height,
        }
    }

    fn transposed(self) -> Size2 {
        Size2::new(self.height, self.width)
    }
}

/// How an element is sized, written for its horizontal orientation (the
/// vertical orientation swaps the axes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SizePolicy {
    /// The smallest size the element accepts. Below it, the element steps
    /// down to its next form.
    pub min_size: Size2,
    /// The preferred size, the one the element is placed at.
    pub size: Size2,
    /// The largest size growth may reach.
    pub max_size: Size2,
    /// A width-to-height ratio the element keeps, if any.
    pub aspect_ratio: Option<AspectRatio>,
    /// Whether, and along which axis, the element grows.
    pub grow: Grow,
}

impl SizePolicy {
    /// A fixed element of one size.
    pub const fn fixed(size: Size2) -> SizePolicy {
        SizePolicy {
            min_size: size,
            size,
            max_size: size,
            aspect_ratio: None,
            grow: Grow::Fixed,
        }
    }

    /// A growable element: placed at `size`, accepting down to `min_size`,
    /// growing along `axis` up to `max_size`.
    pub const fn growable(min_size: Size2, size: Size2, max_size: Size2, axis: Axis) -> SizePolicy {
        SizePolicy {
            min_size,
            size,
            max_size,
            aspect_ratio: None,
            grow: Grow::Along(axis),
        }
    }

    /// The same policy keeping an aspect ratio.
    pub const fn with_aspect_ratio(mut self, width: u32, height: u32) -> SizePolicy {
        self.aspect_ratio = Some(AspectRatio { width, height });
        self
    }

    /// The policy made consistent (min ≤ size ≤ max on each axis, no zero
    /// ratio), then turned to `orientation`.
    pub(crate) fn resolved(&self, orientation: Orientation) -> SizePolicy {
        let mut p = *self;
        for axis in [Axis::Width, Axis::Height] {
            let min = p.min_size.get(axis);
            let max = p.max_size.get(axis).max(min);
            let size = p.size.get(axis).clamp(min, max);
            set(&mut p.max_size, axis, max);
            set(&mut p.size, axis, size);
        }
        if matches!(p.aspect_ratio, Some(r) if r.width == 0 || r.height == 0) {
            p.aspect_ratio = None;
        }
        if orientation == Orientation::Vertical {
            p.min_size = p.min_size.transposed();
            p.size = p.size.transposed();
            p.max_size = p.max_size.transposed();
            p.aspect_ratio = p.aspect_ratio.map(|r| AspectRatio {
                width: r.height,
                height: r.width,
            });
            if let Grow::Along(axis) = p.grow {
                p.grow = Grow::Along(axis.other());
            }
        }
        p
    }
}

fn set(s: &mut Size2, axis: Axis, value: Size) {
    match axis {
        Axis::Width => s.width = value,
        Axis::Height => s.height = value,
    }
}

/// A grid position, in twelfths of the unit from the container's top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cell {
    /// Twelfths from the left edge.
    pub col: u32,
    /// Twelfths from the top edge.
    pub row: u32,
}

/// One element of a container, in its logical order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Element {
    /// The caller's stable key. Hysteresis finds an element's previous
    /// choices by it, so keep it across layouts; keys should be unique.
    pub key: u64,
    /// The forms, largest first. Form 0 is the full form; the others are
    /// smaller forms, known to the engine only by index.
    pub forms: Vec<SizePolicy>,
    /// The supported orientations, preferred first. Empty means horizontal.
    pub orientations: Vec<Orientation>,
    /// An orientation chosen by the person, used instead of the preference
    /// when it is supported.
    pub pinned_orientation: Option<Orientation>,
    /// A position stored from the person's layout, placed before the flow.
    pub pin: Option<Cell>,
}

impl Element {
    /// A horizontal element with one form.
    pub fn new(key: u64, full: SizePolicy) -> Element {
        Element {
            key,
            forms: vec![full],
            orientations: vec![Orientation::Horizontal],
            pinned_orientation: None,
            pin: None,
        }
    }

    /// Adds a smaller form, after the existing ones.
    pub fn with_form(mut self, form: SizePolicy) -> Element {
        self.forms.push(form);
        self
    }

    /// Sets the supported orientations, preferred first.
    pub fn with_orientations(mut self, orientations: &[Orientation]) -> Element {
        self.orientations = orientations.to_vec();
        self
    }

    /// Pins the orientation.
    pub fn with_pinned_orientation(mut self, orientation: Orientation) -> Element {
        self.pinned_orientation = Some(orientation);
        self
    }

    /// Pins the position, in twelfths.
    pub fn with_pin(mut self, col: u32, row: u32) -> Element {
        self.pin = Some(Cell { col, row });
        self
    }

    /// The orientations to try, in order.
    pub(crate) fn orientation_order(&self) -> Vec<Orientation> {
        let supported: &[Orientation] = if self.orientations.is_empty() {
            &[Orientation::Horizontal]
        } else {
            &self.orientations
        };
        match self.pinned_orientation {
            Some(o) if supported.contains(&o) => vec![o],
            _ => {
                let mut order = Vec::with_capacity(2);
                for &o in supported {
                    if !order.contains(&o) {
                        order.push(o);
                    }
                }
                order
            }
        }
    }
}
