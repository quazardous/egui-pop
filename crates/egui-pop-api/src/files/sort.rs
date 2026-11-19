//! The order of the in-app picker's entries: by which column, up or down,
//! folders first or among the files, names read naturally.
//!
//! ```
//! use std::cmp::Ordering;
//! use egui_pop_api::files::sort::{PickerSort, SortColumn, natural_cmp};
//!
//! assert_eq!(natural_cmp("IMG_9.cr3", "IMG_10.cr3"), Ordering::Less);
//! let newest = PickerSort::default().click(SortColumn::Modified);
//! assert_eq!(newest.as_text(), "-modified");
//! assert_eq!(PickerSort::parse("-modified"), Some(newest));
//! ```

use std::cmp::Ordering;
use std::time::SystemTime;

use super::extension_dot;
#[cfg(test)]
use super::{Entry, fs::Meta};

/// What the picker's and the file browser's entries are sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SortColumn {
    /// The name, read naturally ([`natural_cmp`]).
    #[default]
    Name,
    /// A file's size in bytes.
    Size,
    /// When it last changed.
    Modified,
    /// A file's extension.
    Type,
    /// When a photo was taken, as an application's source says
    /// ([`SourceEntry::taken`](super::browser::SourceEntry::taken)); the
    /// time it last changed when the source does not say.
    Taken,
}

impl SortColumn {
    /// Its name in the store and to agents.
    pub fn as_str(self) -> &'static str {
        match self {
            SortColumn::Name => "name",
            SortColumn::Size => "size",
            SortColumn::Modified => "modified",
            SortColumn::Type => "type",
            SortColumn::Taken => "taken",
        }
    }

    /// The column of a name in the store.
    pub fn parse(name: &str) -> Option<SortColumn> {
        match name {
            "name" => Some(SortColumn::Name),
            "size" => Some(SortColumn::Size),
            "modified" => Some(SortColumn::Modified),
            "type" => Some(SortColumn::Type),
            "taken" => Some(SortColumn::Taken),
            _ => None,
        }
    }

    /// Every column, in the order a sort menu lists them.
    pub const ALL: [SortColumn; 5] = [
        SortColumn::Name,
        SortColumn::Modified,
        SortColumn::Taken,
        SortColumn::Size,
        SortColumn::Type,
    ];

    /// Which way a first click sorts: the newest and the largest first, as
    /// Finder and Nautilus do; those are what people look for in these
    /// columns.
    pub fn starts_descending(self) -> bool {
        matches!(
            self,
            SortColumn::Size | SortColumn::Modified | SortColumn::Taken
        )
    }
}

/// The order of the picker's entries. A listing always has one: unlike the
/// data table's, a header's click goes up and down, never to no order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PickerSort {
    pub column: SortColumn,
    pub descending: bool,
}

impl PickerSort {
    /// What a click on `column`'s header asks for.
    pub fn click(self, column: SortColumn) -> PickerSort {
        PickerSort {
            column,
            descending: if self.column == column {
                !self.descending
            } else {
                column.starts_descending()
            },
        }
    }

    /// As the store keeps it: `name`, `-name` (descending), as the data
    /// table does.
    pub fn as_text(self) -> String {
        let column = self.column.as_str();
        if self.descending {
            format!("-{column}")
        } else {
            column.to_string()
        }
    }

    /// The order of a text in the store.
    pub fn parse(text: &str) -> Option<PickerSort> {
        let (column, descending) = match text.strip_prefix('-') {
            Some(column) => (column, true),
            None => (text, false),
        };
        SortColumn::parse(column).map(|column| PickerSort { column, descending })
    }
}

/// Compares names as people read them: a run of digits is a number
/// (`IMG_9` before `IMG_10`), and case does not matter. Names that read the
/// same (`a` and `A`, `01` and `1`) take the order of their characters, so
/// that this is a total order. It compares characters, not a language's
/// collation: `é` comes after `z`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a, b);
    loop {
        let (Some(c), Some(d)) = (x.chars().next(), y.chars().next()) else {
            return x.len().cmp(&y.len()).then_with(|| a.cmp(b));
        };
        let order = if c.is_ascii_digit() && d.is_ascii_digit() {
            let (m, rest_x) = digits(x);
            let (n, rest_y) = digits(y);
            x = rest_x;
            y = rest_y;
            m.len().cmp(&n.len()).then_with(|| m.cmp(n))
        } else {
            x = &x[c.len_utf8()..];
            y = &y[d.len_utf8()..];
            c.to_lowercase().cmp(d.to_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
}

/// The number a text starts with, without its leading zeros, and what
/// follows it.
fn digits(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, rest) = text.split_at(end);
    (number.trim_start_matches('0'), rest)
}

/// What an entry is sorted by, in the column sorted. All of one kind.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Size(u64),
    Time(SystemTime),
    Extension(String),
}

/// A file's extension in lower case, empty without one.
fn extension(name: &str) -> String {
    extension_dot(name).map_or_else(String::new, |dot| name[dot + 1..].to_lowercase())
}

/// What [`sort_by`] asks of each item it sorts.
pub(super) struct SortFacts<'a> {
    pub name: &'a str,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub taken: Option<SystemTime>,
}

/// Puts entries in `sort`'s order ([`sort_by`]), `facts` asked only when the
/// column needs them: the tests' way to read the order.
#[cfg(test)]
pub(super) fn sort_entries(
    entries: Vec<Entry>,
    sort: PickerSort,
    folders_first: bool,
    facts: impl Fn(&Entry) -> Option<Meta>,
) -> Vec<Entry> {
    let needs_facts = matches!(
        sort.column,
        SortColumn::Size | SortColumn::Modified | SortColumn::Taken
    );
    let mut keyed: Vec<(Option<Meta>, Entry)> = entries
        .into_iter()
        .map(|e| (needs_facts.then(|| facts(&e)).flatten(), e))
        .collect();
    sort_by(&mut keyed, sort, folders_first, |(m, e)| SortFacts {
        name: &e.name,
        is_dir: e.is_dir,
        size: m.as_ref().filter(|m| !m.is_dir).map(|m| m.len),
        modified: m.as_ref().and_then(|m| m.modified),
        // A file system knows no capture time.
        taken: None,
    });
    keyed.into_iter().map(|(_, e)| e).collect()
}

/// Sorts `items` in `sort`'s order. Folders stay first when `folders_first`,
/// in the same direction among themselves. What has no value in the column (a
/// folder's size and type, a time the file system does not say) goes last either
/// way, and the name settles what ties: upwards, except in the name's own column.
pub(super) fn sort_by<T>(
    items: &mut [T],
    sort: PickerSort,
    folders_first: bool,
    facts: impl Fn(&T) -> SortFacts<'_>,
) {
    let value = |f: &SortFacts<'_>| match sort.column {
        SortColumn::Name => None,
        SortColumn::Size => f.size.filter(|_| !f.is_dir).map(Value::Size),
        SortColumn::Modified => f.modified.map(Value::Time),
        SortColumn::Taken => f.taken.or(f.modified).map(Value::Time),
        SortColumn::Type => (!f.is_dir).then(|| Value::Extension(extension(f.name))),
    };
    let mut order: Vec<(Option<Value>, usize)> = items
        .iter()
        .enumerate()
        .map(|(i, t)| (value(&facts(t)), i))
        .collect();
    order.sort_by(|(v, i), (w, j)| {
        let (a, b) = (facts(&items[*i]), facts(&items[*j]));
        let group = if folders_first {
            b.is_dir.cmp(&a.is_dir)
        } else {
            Ordering::Equal
        };
        let values = || match (v, w) {
            (Some(v), Some(w)) if sort.descending => w.cmp(v),
            (Some(v), Some(w)) => v.cmp(w),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        };
        let names = || {
            let order = natural_cmp(a.name, b.name);
            if sort.descending && sort.column == SortColumn::Name {
                order.reverse()
            } else {
                order
            }
        };
        group.then_with(values).then_with(names)
    });
    apply_order(items, order.into_iter().map(|(_, i)| i).collect());
}

/// Moves `items` into `order` (each an index of where the item is now), in
/// place.
fn apply_order<T>(items: &mut [T], mut order: Vec<usize>) {
    // Follow each cycle of the permutation once.
    for start in 0..order.len() {
        let mut at = start;
        while order[at] != start {
            let from = order[at];
            items.swap(at, from);
            order[at] = at;
            at = from;
        }
        order[at] = at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn names_sort_as_people_read_them() {
        let mut names = vec![
            "IMG_10.cr3",
            "b",
            "IMG_9.cr3",
            "A",
            "IMG_009.cr3",
            "img_2.cr3",
            "a",
            "10",
            "9",
            "été",
            "z",
            "x18446744073709551616000",
            "x18446744073709551617",
        ];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            [
                "9",
                "10",
                "A",
                "a",
                "b",
                "img_2.cr3",
                "IMG_009.cr3",
                "IMG_9.cr3",
                "IMG_10.cr3",
                "x18446744073709551617",
                "x18446744073709551616000",
                "z",
                "été",
            ]
        );
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
        // Digits among other characters: the order holds for any three.
        assert_eq!(natural_cmp("a!", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "ab"), Ordering::Less);
        assert_eq!(natural_cmp("a!", "ab"), Ordering::Less);
        // Zeros alone, and a number against its own start.
        assert_eq!(natural_cmp("0", "00"), Ordering::Less);
        assert_eq!(natural_cmp("0", "1"), Ordering::Less);
    }

    #[test]
    fn the_stored_text_round_trips_and_nonsense_is_none() {
        for column in [
            SortColumn::Name,
            SortColumn::Size,
            SortColumn::Modified,
            SortColumn::Type,
        ] {
            for descending in [false, true] {
                let sort = PickerSort { column, descending };
                assert_eq!(PickerSort::parse(&sort.as_text()), Some(sort));
            }
        }
        assert_eq!(PickerSort::parse("colour"), None);
        assert_eq!(PickerSort::parse(""), None);
        assert_eq!(PickerSort::parse("-"), None);
    }

    #[test]
    fn a_click_turns_the_column_or_starts_it_its_own_way() {
        let name = PickerSort::default();
        assert_eq!(name.click(SortColumn::Name).as_text(), "-name");
        assert_eq!(name.click(SortColumn::Name).click(SortColumn::Name), name);
        assert_eq!(name.click(SortColumn::Size).as_text(), "-size");
        assert_eq!(name.click(SortColumn::Modified).as_text(), "-modified");
        assert_eq!(name.click(SortColumn::Type).as_text(), "type");
        let down = name.click(SortColumn::Name);
        assert_eq!(down.click(SortColumn::Type).as_text(), "type");
    }

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry {
            path: PathBuf::from("/d").join(name),
            name: name.into(),
            is_dir,
        }
    }

    fn sorted(text: &str, folders_first: bool) -> Vec<String> {
        let entries = vec![
            entry("b.png", false),
            entry("IMG_10.jpg", false),
            entry("IMG_9.png", false),
            entry("old", true),
            entry("New 2", true),
            entry("New 10", true),
            entry("README", false),
        ];
        let facts = |e: &Entry| {
            let (len, secs) = match e.name.as_str() {
                "b.png" => (5, Some(30)),
                "IMG_10.jpg" => (5, Some(10)),
                "IMG_9.png" => (9, Some(20)),
                "README" => (1, None),
                "old" => (0, Some(5)),
                "New 2" => (0, Some(50)),
                _ => (0, None),
            };
            Some(Meta {
                is_dir: e.is_dir,
                len,
                modified: secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
            })
        };
        let sort = PickerSort::parse(text).unwrap();
        sort_entries(entries, sort, folders_first, facts)
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    #[test]
    fn folders_stay_first_in_every_order_unless_asked_otherwise() {
        assert_eq!(
            sorted("name", true),
            [
                "New 2",
                "New 10",
                "old",
                "b.png",
                "IMG_9.png",
                "IMG_10.jpg",
                "README"
            ]
        );
        assert_eq!(
            sorted("-name", true),
            [
                "old",
                "New 10",
                "New 2",
                "README",
                "IMG_10.jpg",
                "IMG_9.png",
                "b.png"
            ]
        );
        assert_eq!(
            sorted("name", false),
            [
                "b.png",
                "IMG_9.png",
                "IMG_10.jpg",
                "New 2",
                "New 10",
                "old",
                "README"
            ]
        );
    }

    #[test]
    fn size_and_time_order_and_what_has_none_goes_last_either_way() {
        // Ties go up by name in both directions.
        assert_eq!(
            sorted("size", true),
            [
                "New 2",
                "New 10",
                "old",
                "README",
                "b.png",
                "IMG_10.jpg",
                "IMG_9.png"
            ]
        );
        assert_eq!(
            sorted("-size", true),
            [
                "New 2",
                "New 10",
                "old",
                "IMG_9.png",
                "b.png",
                "IMG_10.jpg",
                "README"
            ]
        );
        // Mixed with the files, a folder has no size.
        assert_eq!(
            sorted("-size", false),
            [
                "IMG_9.png",
                "b.png",
                "IMG_10.jpg",
                "README",
                "New 2",
                "New 10",
                "old"
            ]
        );
        assert_eq!(
            sorted("modified", false),
            [
                "old",
                "IMG_10.jpg",
                "IMG_9.png",
                "b.png",
                "New 2",
                "New 10",
                "README"
            ]
        );
        assert_eq!(
            sorted("-modified", false),
            [
                "New 2",
                "b.png",
                "IMG_9.png",
                "IMG_10.jpg",
                "old",
                "New 10",
                "README"
            ]
        );
    }

    #[test]
    fn the_type_is_the_extension_whatever_its_case() {
        assert_eq!(extension("a.PNG"), "png");
        assert_eq!(extension("README"), "");
        assert_eq!(extension(".config"), "");
        assert_eq!(
            sorted("type", true),
            [
                "New 2",
                "New 10",
                "old",
                "README",
                "IMG_10.jpg",
                "b.png",
                "IMG_9.png"
            ]
        );
        assert_eq!(
            sorted("-type", true),
            [
                "New 2",
                "New 10",
                "old",
                "b.png",
                "IMG_9.png",
                "IMG_10.jpg",
                "README"
            ]
        );
    }

    #[test]
    fn name_and_type_read_no_facts() {
        for text in ["name", "-name", "type"] {
            let sort = PickerSort::parse(text).unwrap();
            let out = sort_entries(
                vec![entry("a", false), entry("b", true)],
                sort,
                true,
                |_| panic!("no facts for {text}"),
            );
            assert_eq!(out.len(), 2);
        }
    }

    #[test]
    fn the_picker_sorts_by_capture_time_as_by_change_time() {
        assert_eq!(sorted("taken", true), sorted("modified", true));
        assert_eq!(sorted("-taken", false), sorted("-modified", false));
    }

    #[test]
    fn a_capture_time_comes_before_the_change_time_and_stands_in_for_none() {
        // (name, modified, taken)
        let mut items = vec![
            ("a", Some(1), Some(30)),
            ("b", Some(20), None),
            ("c", None, Some(10)),
            ("d", None, None),
        ];
        let at = |s: Option<u64>| s.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        let sort = PickerSort::parse("taken").unwrap();
        sort_by(&mut items, sort, true, |&(name, modified, taken)| {
            SortFacts {
                name,
                is_dir: false,
                size: None,
                modified: at(modified),
                taken: at(taken),
            }
        });
        let names: Vec<&str> = items.iter().map(|i| i.0).collect();
        assert_eq!(names, ["c", "b", "a", "d"]);
    }

    #[test]
    fn an_order_is_applied_in_place_whatever_its_cycles() {
        for order in [
            vec![0, 1, 2, 3, 4],
            vec![4, 3, 2, 1, 0],
            vec![1, 2, 3, 4, 0],
            vec![2, 0, 1, 4, 3],
        ] {
            let mut items = vec!['a', 'b', 'c', 'd', 'e'];
            let want: Vec<char> = order.iter().map(|&i| items[i]).collect();
            apply_order(&mut items, order);
            assert_eq!(items, want);
        }
    }
}
