//! The strings the viewport shows, by key with English fallbacks,
//! translated through the provider installed with
//! `egui_pop_core::strings::install` (see that module for the key and
//! placeholder conventions).

use egui_pop_core::strings::StrKey;

/// The viewport's accessible name, unless the application gives one.
pub const VIEWPORT: StrKey = StrKey::new("egui-pop-viewport-label", "Image viewport");
/// The histogram's accessible name, unless the application gives one.
pub const HISTOGRAM: StrKey = StrKey::new("egui-pop-viewport-histogram-label", "Histogram");
/// The histogram's accessible value: its channels and bin count.
pub const HISTOGRAM_VALUE: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-value",
    "{ $channels } ({ $bins } bins)",
);
/// As [`HISTOGRAM_VALUE`], on a logarithmic scale.
pub const HISTOGRAM_VALUE_LOG: StrKey = StrKey::new(
    "egui-pop-viewport-histogram-value-log",
    "{ $channels } ({ $bins } bins, logarithmic)",
);

/// Every string the viewport shows, for catalogues and tests.
pub const ALL: &[StrKey] = &[VIEWPORT, HISTOGRAM, HISTOGRAM_VALUE, HISTOGRAM_VALUE_LOG];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_fluent_ids() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-"), "{}", k.key);
            assert!(
                k.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                k.key
            );
        }
    }
}
