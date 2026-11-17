//! Slot sizes, counted in twelfths of the base unit.

use core::fmt;

/// Twelfths in one base unit. 12 is the least common multiple of 2, 3 and 4,
/// so every allowed size is a whole number of twelfths.
pub const TWELFTHS_PER_UNIT: u32 = 12;

/// The allowed sizes below one unit, in twelfths: ¼, ⅓, ½, ⅔, ¾.
const FRACTIONS: [u32; 5] = [3, 4, 6, 8, 9];

/// Half a unit, in twelfths: the hysteresis band.
pub(crate) const HALF_UNIT: u32 = TWELFTHS_PER_UNIT / 2;

/// A standard slot size along one axis: ¼, ⅓, ½, ⅔, ¾ of the base unit, or a
/// whole number of units.
///
/// The value is stored in twelfths of the unit, so sums stay exact. A `Size`
/// can only hold an allowed value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "u32", into = "u32")
)]
pub struct Size(u32);

impl Size {
    /// A quarter of the unit.
    pub const QUARTER: Size = Size(3);
    /// A third of the unit.
    pub const THIRD: Size = Size(4);
    /// Half the unit.
    pub const HALF: Size = Size(6);
    /// Two thirds of the unit.
    pub const TWO_THIRDS: Size = Size(8);
    /// Three quarters of the unit.
    pub const THREE_QUARTERS: Size = Size(9);
    /// One whole unit.
    pub const ONE: Size = Size(12);

    /// `n` whole units. `n` is raised to 1 if zero.
    pub const fn units(n: u32) -> Size {
        let n = if n == 0 { 1 } else { n };
        Size(n.saturating_mul(TWELFTHS_PER_UNIT) / TWELFTHS_PER_UNIT * TWELFTHS_PER_UNIT)
    }

    /// The size of `twelfths` twelfths, if it is an allowed size.
    pub fn from_twelfths(twelfths: u32) -> Option<Size> {
        is_allowed(twelfths).then_some(Size(twelfths))
    }

    /// The size in twelfths of the unit.
    pub const fn twelfths(self) -> u32 {
        self.0
    }

    /// The next larger allowed size.
    pub fn next_step(self) -> Size {
        next_step(self.0).map_or(self, Size)
    }

    /// The next smaller allowed size, if any.
    pub fn prev_step(self) -> Option<Size> {
        prev_step(self.0).map(Size)
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            3 => f.write_str("1/4"),
            4 => f.write_str("1/3"),
            6 => f.write_str("1/2"),
            8 => f.write_str("2/3"),
            9 => f.write_str("3/4"),
            t => write!(f, "{}", t / TWELFTHS_PER_UNIT),
        }
    }
}

/// The error of converting a number of twelfths that is not an allowed size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSize(pub u32);

impl fmt::Display for InvalidSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} twelfths is not an allowed slot size", self.0)
    }
}

impl std::error::Error for InvalidSize {}

impl TryFrom<u32> for Size {
    type Error = InvalidSize;
    fn try_from(twelfths: u32) -> Result<Size, InvalidSize> {
        Size::from_twelfths(twelfths).ok_or(InvalidSize(twelfths))
    }
}

impl From<Size> for u32 {
    fn from(size: Size) -> u32 {
        size.0
    }
}

/// Whether `t` twelfths is an allowed size.
pub(crate) fn is_allowed(t: u32) -> bool {
    FRACTIONS.contains(&t) || (t > 0 && t.is_multiple_of(TWELFTHS_PER_UNIT))
}

/// The smallest allowed size above `t`.
pub(crate) fn next_step(t: u32) -> Option<u32> {
    if t < TWELFTHS_PER_UNIT {
        FRACTIONS
            .iter()
            .copied()
            .find(|&f| f > t)
            .or(Some(TWELFTHS_PER_UNIT))
    } else {
        (t / TWELFTHS_PER_UNIT + 1).checked_mul(TWELFTHS_PER_UNIT)
    }
}

/// The largest allowed size below `t`.
pub(crate) fn prev_step(t: u32) -> Option<u32> {
    floor_step(t.checked_sub(1)?)
}

/// The largest allowed size at most `t`.
pub(crate) fn floor_step(t: u32) -> Option<u32> {
    if t >= TWELFTHS_PER_UNIT {
        Some(t / TWELFTHS_PER_UNIT * TWELFTHS_PER_UNIT)
    } else {
        FRACTIONS.iter().copied().rev().find(|&f| f <= t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder() {
        let mut s = Size::QUARTER;
        let mut seen = vec![s.twelfths()];
        for _ in 0..7 {
            s = s.next_step();
            seen.push(s.twelfths());
        }
        assert_eq!(seen, [3, 4, 6, 8, 9, 12, 24, 36]);
        assert_eq!(Size::units(2).prev_step(), Some(Size::ONE));
        assert_eq!(Size::ONE.prev_step(), Some(Size::THREE_QUARTERS));
        assert_eq!(Size::QUARTER.prev_step(), None);
    }

    #[test]
    fn allowed_only() {
        assert!(Size::from_twelfths(5).is_none());
        assert!(Size::from_twelfths(13).is_none());
        assert!(Size::from_twelfths(0).is_none());
        assert_eq!(Size::from_twelfths(36), Some(Size::units(3)));
        assert_eq!(Size::units(0), Size::ONE);
        assert_eq!(floor_step(23), Some(12));
        assert_eq!(floor_step(7), Some(6));
        assert_eq!(floor_step(2), None);
        assert_eq!(Size::TWO_THIRDS.to_string(), "2/3");
    }
}
