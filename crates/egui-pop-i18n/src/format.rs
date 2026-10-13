//! Locale-aware numbers and dates, for Rust code and for catalogues.
//!
//! In a catalogue, numbers go through Fluent's `NUMBER` function (or are
//! placed as they are) and are written with the locale's separators;
//! plurals select on them by the locale's rules. Dates go through
//! `DATETIME`, which takes an ISO date (`2026-10-12`), date and time
//! (`2026-10-12T14:05`) or a number of seconds since 1970 (UTC):
//!
//! ```ftl
//! files = { $count ->
//!     [one] { $count } file
//!    *[other] { $count } files
//! }
//! size = { NUMBER($mib, maximumFractionDigits: 1) } MiB
//! saved = Saved on { DATETIME($when, dateStyle: "long") }
//! ```
//!
//! `dateStyle` is `short`, `medium` (the default), `long` or `full`;
//! `timeStyle` is `short` or `medium`. Month and weekday names are known
//! for English and French; other languages get numeric dates
//! (`2026-10-12`).
//!
//! ```
//! use egui_pop_i18n::format::{self, DateStyle};
//!
//! assert_eq!(format::number("fr", 1234.5, Some(1)), "1\u{202f}234,5");
//! assert_eq!(format::number("en", 1234.5, Some(1)), "1,234.5");
//! let d = format::Date::parse("2026-10-12").unwrap();
//! assert_eq!(format::date("fr", &d, DateStyle::Long), "12 octobre 2026");
//! assert_eq!(format::date("en", &d, DateStyle::Medium), "Oct 12, 2026");
//! ```

use fluent_bundle::types::{FluentNumber, FluentNumberOptions, FluentNumberStyle};
use fluent_bundle::{FluentArgs, FluentValue};

use crate::locale::language;

/// How a locale writes numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Separators {
    pub decimal: &'static str,
    pub group: &'static str,
    /// What goes between a number and `%`.
    pub percent: &'static str,
    /// What goes between a number and its unit (`1,5 EV`, `12 px`, a
    /// slider's `50 %`).
    pub unit: &'static str,
}

/// The separators of a locale (English's for a language not known).
pub fn separators(locale: &str) -> Separators {
    const NNBSP: &str = "\u{202f}";
    const NBSP: &str = "\u{a0}";
    let (decimal, group, percent) = match locale {
        "de-CH" | "it-CH" => (".", "’", ""),
        "pt-PT" => (",", NBSP, ""),
        _ => match language(locale) {
            "fr" => (",", NNBSP, NNBSP),
            "de" | "nl" | "it" | "es" | "pt" | "da" | "id" | "el" | "ro" | "tr" | "sl" | "hr" => {
                (",", ".", "")
            }
            "ru" | "uk" | "pl" | "cs" | "sk" | "sv" | "nb" | "fi" | "hu" | "bg" | "lt" | "lv"
            | "et" => (",", NBSP, NBSP),
            _ => (".", ",", ""),
        },
    };
    // French typography puts a narrow no-break space before a unit;
    // other languages keep a plain space.
    let unit = if language(locale) == "fr" { NNBSP } else { " " };
    Separators {
        decimal,
        group,
        percent,
        unit,
    }
}

/// A number written for a locale: at most `max_fraction` digits after
/// the separator (3 when `None`), trailing zeros dropped, grouped by
/// thousands.
pub fn number(locale: &str, value: f64, max_fraction: Option<usize>) -> String {
    let options = FluentNumberOptions {
        maximum_fraction_digits: max_fraction,
        ..FluentNumberOptions::default()
    };
    number_with(locale, value, &options)
}

/// A number written for a locale with Fluent's options
/// (`minimumFractionDigits`, `maximumFractionDigits`,
/// `minimumIntegerDigits`, `useGrouping`, `style: "percent"`).
pub fn number_with(locale: &str, value: f64, options: &FluentNumberOptions) -> String {
    let sep = separators(locale);
    if !value.is_finite() {
        return value.to_string();
    }
    let percent = options.style == FluentNumberStyle::Percent;
    let value = if percent { value * 100.0 } else { value };
    let min = options.minimum_fraction_digits.unwrap_or(0);
    let max = options
        .maximum_fraction_digits
        .unwrap_or(if percent { 0 } else { 3 })
        .max(min);
    let fixed = format!("{:.*}", max, value.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut frac = frac.trim_end_matches('0').to_string();
    while frac.len() < min {
        frac.push('0');
    }
    let mut int = int.to_string();
    if let Some(digits) = options.minimum_integer_digits {
        while int.len() < digits {
            int.insert(0, '0');
        }
    }
    let int = if options.use_grouping && int.len() > 3 {
        let mut out = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i) % 3 == 0 {
                out.push_str(sep.group);
            }
            out.push(c);
        }
        out
    } else {
        int
    };
    let negative =
        value < 0.0 && (int.chars().any(|c| c.is_ascii_digit() && c != '0') || !frac.is_empty());
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&int);
    if !frac.is_empty() {
        out.push_str(sep.decimal);
        out.push_str(&frac);
    }
    if percent {
        out.push_str(sep.percent);
        out.push('%');
    }
    out
}

/// A number already written followed by its unit, with the locale's
/// space between them (the number alone when `unit` is empty).
///
/// ```
/// use egui_pop_i18n::format;
///
/// assert_eq!(format::quantity("fr", "1,5", "EV"), "1,5\u{202f}EV");
/// assert_eq!(format::quantity("en", "50", "%"), "50 %");
/// ```
pub fn quantity(locale: &str, number: &str, unit: &str) -> String {
    if unit.is_empty() {
        number.to_string()
    } else {
        format!("{number}{}{unit}", separators(locale).unit)
    }
}

/// A calendar date, with an optional time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub time: Option<(u32, u32, u32)>,
}

impl Date {
    /// From `YYYY-MM-DD`, optionally followed by `THH:MM[:SS]` (a
    /// trailing zone or fraction is ignored).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (date, time) = match s.split_once(['T', ' ']) {
            Some((d, t)) => (d, Some(t)),
            None => (s, None),
        };
        let mut parts = date.splitn(3, '-');
        let year = parts.next()?.parse().ok()?;
        let month: u32 = parts.next()?.parse().ok()?;
        let day: u32 = parts.next()?.parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        let time = match time {
            None => None,
            Some(t) => {
                let t: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == ':')
                    .collect();
                let mut p = t.split(':');
                let h: u32 = p.next()?.parse().ok()?;
                let m: u32 = p.next()?.parse().ok()?;
                let sec: u32 = p.next().map_or(Some(0), |x| x.parse().ok())?;
                (h < 24 && m < 60 && sec < 61).then_some((h, m, sec))?;
                Some((h, m, sec))
            }
        };
        Some(Self {
            year,
            month,
            day,
            time,
        })
    }

    /// From seconds since 1970-01-01 (UTC).
    pub fn from_unix(seconds: i64) -> Self {
        let days = seconds.div_euclid(86_400);
        let secs = seconds.rem_euclid(86_400) as u32;
        // Howard Hinnant's days-to-civil.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (if month <= 2 { y + 1 } else { y }) as i32;
        Self {
            year,
            month,
            day,
            time: Some((secs / 3600, secs / 60 % 60, secs % 60)),
        }
    }

    /// The day of the week, 0 for Monday.
    pub fn weekday(&self) -> u32 {
        // Days since 1970-01-01 (a Thursday), by civil-to-days.
        let (y, m) = if self.month <= 2 {
            (i64::from(self.year) - 1, i64::from(self.month) + 9)
        } else {
            (i64::from(self.year), i64::from(self.month) - 3)
        };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let doy = (153 * m + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        (days + 3).rem_euclid(7) as u32
    }
}

/// How long a date is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateStyle {
    Short,
    Medium,
    Long,
    Full,
}

impl DateStyle {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "short" => Self::Short,
            "medium" => Self::Medium,
            "long" => Self::Long,
            "full" => Self::Full,
            _ => return None,
        })
    }
}

const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const EN_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const FR_MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
const FR_SHORT_MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];
const FR_DAYS: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];

/// English written month first (the United States, or no region).
fn month_first(locale: &str) -> bool {
    let region = locale.split('-').skip(1).find(|p| p.len() == 2);
    matches!(region, None | Some("US" | "PH"))
}

/// A date written for a locale.
pub fn date(locale: &str, d: &Date, style: DateStyle) -> String {
    let m = (d.month as usize).saturating_sub(1).min(11);
    let (y, day) = (d.year, d.day);
    match language(locale) {
        "en" => {
            let month = EN_MONTHS[m];
            let us = month_first(locale);
            match style {
                DateStyle::Short if us => format!("{}/{day}/{}", d.month, y.rem_euclid(100)),
                DateStyle::Short => format!("{day:02}/{:02}/{y}", d.month),
                DateStyle::Medium if us => format!("{} {day}, {y}", &month[..3]),
                DateStyle::Medium => format!("{day} {} {y}", &month[..3]),
                DateStyle::Long if us => format!("{month} {day}, {y}"),
                DateStyle::Long => format!("{day} {month} {y}"),
                DateStyle::Full if us => {
                    format!("{}, {month} {day}, {y}", EN_DAYS[d.weekday() as usize])
                }
                DateStyle::Full => format!("{}, {day} {month} {y}", EN_DAYS[d.weekday() as usize]),
            }
        }
        "fr" => match style {
            DateStyle::Short => format!("{day:02}/{:02}/{y}", d.month),
            DateStyle::Medium => format!("{day} {} {y}", FR_SHORT_MONTHS[m]),
            DateStyle::Long => format!("{day} {} {y}", FR_MONTHS[m]),
            DateStyle::Full => format!(
                "{} {day} {} {y}",
                FR_DAYS[d.weekday() as usize],
                FR_MONTHS[m]
            ),
        },
        _ => format!("{y:04}-{:02}-{day:02}", d.month),
    }
}

/// A time of day written for a locale; `seconds` for the medium style.
pub fn time(locale: &str, hour: u32, minute: u32, second: Option<u32>) -> String {
    let secs = second.map(|s| format!(":{s:02}")).unwrap_or_default();
    if language(locale) == "en" && month_first(locale) {
        let h12 = match hour % 12 {
            0 => 12,
            h => h,
        };
        let half = if hour < 12 { "AM" } else { "PM" };
        format!("{h12}:{minute:02}{secs}\u{202f}{half}")
    } else {
        format!("{hour:02}:{minute:02}{secs}")
    }
}

/// Number formatting as Fluent's custom formatter: called by every
/// bundle for every number it writes, with the bundle's locale.
pub(crate) fn fluent_formatter(
    value: &FluentValue,
    intls: &intl_memoizer::concurrent::IntlLangMemoizer,
) -> Option<String> {
    use fluent_bundle::memoizer::MemoizerKind;
    let FluentValue::Number(n) = value else {
        return None;
    };
    intls
        .with_try_get_threadsafe::<LocaleOf, _, _>((), |l| number_with(&l.0, n.value, &n.options))
        .ok()
}

/// The locale a memoizer is for.
struct LocaleOf(String);

impl intl_memoizer::Memoizable for LocaleOf {
    type Args = ();
    type Error = ();

    fn construct(lang: unic_langid::LanguageIdentifier, _: ()) -> Result<Self, ()> {
        Ok(Self(lang.to_string()))
    }
}

/// The `DATETIME` function of a bundle in `locale`.
pub(crate) fn datetime_function(
    locale: String,
) -> impl for<'a> Fn(&[FluentValue<'a>], &FluentArgs) -> FluentValue<'a> + Send + Sync + 'static {
    move |positional, named| {
        let d = match positional.first() {
            Some(FluentValue::String(s)) => Date::parse(s),
            Some(FluentValue::Number(n)) => Some(Date::from_unix(n.value as i64)),
            _ => None,
        };
        let Some(d) = d else {
            return FluentValue::Error;
        };
        let option = |name: &str| match named.get(name) {
            Some(FluentValue::String(s)) => Some(s.to_string()),
            _ => None,
        };
        let date_style = option("dateStyle").and_then(|s| DateStyle::parse(&s));
        let time_style = option("timeStyle");
        let mut parts = Vec::new();
        if date_style.is_some() || time_style.is_none() {
            parts.push(date(&locale, &d, date_style.unwrap_or(DateStyle::Medium)));
        }
        if let (Some(ts), Some((h, m, s))) = (time_style, d.time) {
            parts.push(time(&locale, h, m, (ts == "medium").then_some(s)));
        }
        FluentValue::String(parts.join(" ").into())
    }
}

/// A Fluent number from Rust, for arguments.
pub fn fluent_number(value: f64) -> FluentValue<'static> {
    FluentValue::Number(FluentNumber::new(value, FluentNumberOptions::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_the_locale() {
        assert_eq!(number("en", 1_234_567.891, None), "1,234,567.891");
        assert_eq!(
            number("fr", 1_234_567.891, Some(2)),
            "1\u{202f}234\u{202f}567,89"
        );
        assert_eq!(number("de", 1234.0, None), "1.234");
        assert_eq!(number("en", 3.0, None), "3");
        assert_eq!(number("en", -0.0001, Some(2)), "0");
        assert_eq!(number("en", -2.5, None), "-2.5");
        let pct = FluentNumberOptions {
            style: FluentNumberStyle::Percent,
            ..Default::default()
        };
        assert_eq!(number_with("fr", 0.25, &pct), "25\u{202f}%");
        assert_eq!(number_with("en", 0.25, &pct), "25%");
        let min = FluentNumberOptions {
            minimum_fraction_digits: Some(2),
            use_grouping: false,
            ..Default::default()
        };
        assert_eq!(number_with("en", 1234.5, &min), "1234.50");
    }

    #[test]
    fn a_unit_is_spaced_as_the_locale_does() {
        assert_eq!(quantity("fr", "1,50", "EV"), "1,50\u{202f}EV");
        assert_eq!(quantity("fr-CA", "50", "%"), "50\u{202f}%");
        assert_eq!(quantity("en", "1.50", "EV"), "1.50 EV");
        assert_eq!(quantity("en-US", "50", "%"), "50 %");
        assert_eq!(quantity("de", "12", "px"), "12 px");
        assert_eq!(quantity("fr", "3", ""), "3");
    }

    #[test]
    fn dates_parse_and_print() {
        let d = Date::parse("2026-10-12T14:05:09Z").unwrap();
        assert_eq!(d.time, Some((14, 5, 9)));
        assert_eq!(d.weekday(), 0, "a Monday");
        assert_eq!(
            Date::from_unix(0),
            Date::parse("1970-01-01T00:00:00").unwrap()
        );
        assert_eq!(
            Date::from_unix(1_791_806_752),
            Date::parse("2026-10-12T12:05:52").unwrap()
        );
        assert_eq!(date("en", &d, DateStyle::Full), "Monday, October 12, 2026");
        assert_eq!(date("en-GB", &d, DateStyle::Medium), "12 Oct 2026");
        assert_eq!(date("en", &d, DateStyle::Short), "10/12/26");
        assert_eq!(date("fr", &d, DateStyle::Full), "lundi 12 octobre 2026");
        assert_eq!(date("fr", &d, DateStyle::Short), "12/10/2026");
        assert_eq!(date("ja", &d, DateStyle::Long), "2026-10-12");
        assert_eq!(time("en", 14, 5, None), "2:05\u{202f}PM");
        assert_eq!(time("fr", 14, 5, Some(9)), "14:05:09");
        assert!(Date::parse("2026-13-01").is_none());
    }
}
