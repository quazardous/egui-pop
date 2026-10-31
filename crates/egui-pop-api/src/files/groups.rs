//! Grouped files: files that go together show as one entry of the in-app
//! picker (a RAW with its sidecars, Lightroom's and digiKam's habit), and
//! choosing it gives the main file, with the others beside it.
//!
//! egui-pop has the mechanism and no rule of its own: nothing groups until an
//! application declares a [`FileGrouping`], with [`FilesPlugin::grouping`] or
//! [`Groupings::register`]. A [`SidecarRule`] covers the usual case, by
//! extension; a grouping of its own reads the names as it likes.
//!
//! ```
//! use egui_pop_api::files::groups::{Groupings, SidecarRule};
//! use std::path::Path;
//! use std::sync::Arc;
//!
//! let photos = SidecarRule::new("my_app.photos", &["cr3", "nef"], &["xmp", "jpg"])
//!     .badge("jpg", "JPEG");
//! let groupings = Groupings::default();
//! groupings.register(Arc::new(photos));
//!
//! let names = ["a.cr3", "a.cr3.xmp", "a.jpg", "b.jpg", "c.nef", "c.xmp"];
//! let groups = groupings.in_folder(Path::new("/shoot"), &names);
//! assert_eq!(groups[0].main, Path::new("/shoot/a.cr3"));
//! assert_eq!(groups[0].badges, ["XMP", "JPEG"]);
//! assert_eq!(groups[1].main, Path::new("/shoot/c.nef"));
//! assert_eq!(groups.len(), 2, "b.jpg has no RAW: it stays a file of its own");
//! ```
//!
//! [`FilesPlugin::grouping`]: super::FilesPlugin::grouping

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui_pop_core::lock;
use serde_json::{Value as Json, json};

use super::extension_dot;
use super::fs::SharedFs;

/// One group as a [`FileGrouping`] finds it, by the names of a folder's files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameGroup {
    /// The file the entry stands for.
    pub main: String,
    /// The files that go with it.
    pub members: Vec<String>,
}

/// Which of a folder's files go together: declared by the application, asked
/// once per listing on the interface thread (the picker reads a folder there
/// too), with names and nothing else, so that it reads no file.
pub trait FileGrouping: fmt::Debug + Send + Sync {
    /// Its id: registering another with this id replaces it.
    fn id(&self) -> &str;

    /// The groups among these names: the folder's files, without folders or
    /// hidden files. What makes no sense is ignored, and the file stays an
    /// entry of its own: a name that is not there, a member that is its own
    /// main, or a file an earlier group or grouping took.
    fn groups(&self, names: &[&str]) -> Vec<NameGroup>;

    /// What the entry's badge says of a member (the picker adds the `+`): its
    /// extension in capitals, by default.
    fn badge(&self, member: &str) -> String {
        extension_dot(member).map_or_else(String::new, |dot| member[dot + 1..].to_uppercase())
    }
}

/// The usual grouping, by extension: `x.jpg`, `x.xmp` and `x.cr3.xmp` belong to
/// `x.cr3`. The stem has to be the same, whatever the case of the extensions.
///
/// Main extensions come in the order they win: with `["cr3", "jpg"]`, `x.jpg`
/// belongs to `x.cr3` when there is one and is a main of its own otherwise, for
/// `x.xmp`. A sidecar goes to the first main that claims it, and members come
/// in the order of `sidecars`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidecarRule {
    id: String,
    mains: Vec<String>,
    sidecars: Vec<String>,
    badges: Vec<(String, String)>,
}

impl SidecarRule {
    /// Files with one of the `mains` extensions head a group of the files with
    /// the same stem or the whole name before one of the `sidecars` extensions.
    pub fn new(id: &str, mains: &[&str], sidecars: &[&str]) -> Self {
        let lower = |list: &[&str]| list.iter().map(|e| e.to_lowercase()).collect();
        Self {
            id: id.into(),
            mains: lower(mains),
            sidecars: lower(sidecars),
            badges: Vec::new(),
        }
    }

    /// What the badge says of a sidecar with this extension, in place of the
    /// extension in capitals: `.badge("jpg", "JPEG")`.
    pub fn badge(mut self, extension: &str, badge: &str) -> Self {
        self.badges.push((extension.to_lowercase(), badge.into()));
        self
    }
}

/// A name with its extension in lower case, as a [`SidecarRule`] compares them.
fn folded(name: &str) -> String {
    match extension_dot(name) {
        Some(dot) => format!("{}{}", &name[..=dot], name[dot + 1..].to_lowercase()),
        None => name.to_string(),
    }
}

impl FileGrouping for SidecarRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn groups(&self, names: &[&str]) -> Vec<NameGroup> {
        let by_folded: HashMap<String, &str> = names.iter().map(|n| (folded(n), *n)).collect();
        let mut taken: HashSet<&str> = HashSet::new();
        let mut out = Vec::new();
        for main_extension in &self.mains {
            for name in names {
                let Some(dot) = extension_dot(name) else {
                    continue;
                };
                if name[dot + 1..].to_lowercase() != *main_extension || taken.contains(name) {
                    continue;
                }
                let mut members = Vec::new();
                for sidecar in &self.sidecars {
                    for form in [
                        format!("{}.{sidecar}", &name[..dot]),
                        format!("{name}.{sidecar}"),
                    ] {
                        if let Some(member) = by_folded.get(&form)
                            && member != name
                            && taken.insert(member)
                        {
                            members.push((*member).to_string());
                        }
                    }
                }
                taken.insert(name);
                if !members.is_empty() {
                    out.push(NameGroup {
                        main: (*name).to_string(),
                        members,
                    });
                }
            }
        }
        out
    }

    fn badge(&self, member: &str) -> String {
        let Some(dot) = extension_dot(member) else {
            return String::new();
        };
        let extension = &member[dot + 1..];
        self.badges
            .iter()
            .find(|(e, _)| e.eq_ignore_ascii_case(extension))
            .map_or_else(|| extension.to_uppercase(), |(_, badge)| badge.clone())
    }
}

/// A group, as the picker shows it and as a choice gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileGroup {
    /// The file the entry stands for: what choosing it gives.
    pub main: PathBuf,
    /// The files that go with it.
    pub members: Vec<PathBuf>,
    /// What the entry's badges say, each once, without the `+`.
    pub badges: Vec<String>,
}

impl FileGroup {
    /// The names of its members.
    pub fn member_names(&self) -> Vec<String> {
        self.members
            .iter()
            .filter_map(|m| m.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect()
    }

    fn to_json(&self) -> Json {
        json!({
            "main": self.main.display().to_string(),
            "members": self.members.iter().map(|m| m.display().to_string()).collect::<Vec<_>>(),
            "badges": self.badges,
        })
    }

    fn from_json(json: &Json) -> Option<FileGroup> {
        let strings = |key: &str| {
            json.get(key)?
                .as_array()
                .map(|a| a.iter().filter_map(Json::as_str).map(String::from))
                .map(Iterator::collect::<Vec<_>>)
        };
        Some(FileGroup {
            main: PathBuf::from(json.get("main")?.as_str()?),
            members: strings("members")?.into_iter().map(PathBuf::from).collect(),
            badges: strings("badges").unwrap_or_default(),
        })
    }
}

/// Groups as a choice's result and the `file.chosen` step say them.
pub(super) fn to_json(groups: &[FileGroup]) -> Json {
    Json::Array(groups.iter().map(FileGroup::to_json).collect())
}

/// The groups a choice's result or the `file.chosen` step's arguments hold.
pub(super) fn from_json(json: &Json) -> Vec<FileGroup> {
    json.get("groups")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(FileGroup::from_json).collect())
        .unwrap_or_default()
}

/// The groupings declared, as a service of [`Files`](super::Files).
#[derive(Clone, Debug, Default)]
pub struct Groupings(Arc<Mutex<Vec<Arc<dyn FileGrouping>>>>);

impl Groupings {
    /// Adds a grouping, or replaces the one with its id. Earlier ones win a
    /// file both claim. Pickers open now take it at their next listing. A
    /// plugin registering one takes it back in its `disconnect`.
    pub fn register(&self, grouping: Arc<dyn FileGrouping>) {
        let mut list = lock(&self.0);
        list.retain(|g| g.id() != grouping.id());
        list.push(grouping);
    }

    /// Takes a grouping away; says whether it was there.
    pub fn unregister(&self, id: &str) -> bool {
        let mut list = lock(&self.0);
        let before = list.len();
        list.retain(|g| g.id() != id);
        list.len() != before
    }

    /// The ids of the groupings, in the order they apply.
    pub fn ids(&self) -> Vec<String> {
        lock(&self.0).iter().map(|g| g.id().to_string()).collect()
    }

    /// Whether none is declared: nothing groups.
    pub fn is_empty(&self) -> bool {
        lock(&self.0).is_empty()
    }

    /// The groups among the names of the files of a folder, from every
    /// grouping. A grouping that panics counts for nothing, and the log says so.
    pub fn in_folder(&self, dir: &Path, names: &[&str]) -> Vec<FileGroup> {
        let list = lock(&self.0).clone();
        let present: HashSet<&str> = names.iter().copied().collect();
        let mut members_taken: HashSet<String> = HashSet::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<FileGroup> = Vec::new();
        for grouping in list {
            let found = catch_unwind(AssertUnwindSafe(|| grouping.groups(names)));
            let Ok(found) = found else {
                log::warn!(target: "egui_pop", "files: the grouping {} panicked", grouping.id());
                continue;
            };
            for group in found {
                if !present.contains(group.main.as_str()) || members_taken.contains(&group.main) {
                    continue;
                }
                let members: Vec<String> = group
                    .members
                    .into_iter()
                    .filter(|m| {
                        present.contains(m.as_str())
                            && *m != group.main
                            && !at.contains_key(m)
                            && !members_taken.contains(m)
                    })
                    .collect();
                let mut kept = Vec::new();
                for member in members {
                    if members_taken.insert(member.clone()) {
                        kept.push(member);
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                let n = *at.entry(group.main.clone()).or_insert_with(|| {
                    out.push(FileGroup {
                        main: dir.join(&group.main),
                        ..FileGroup::default()
                    });
                    out.len() - 1
                });
                for member in kept {
                    let badge = grouping.badge(&member);
                    if !badge.is_empty() && !out[n].badges.contains(&badge) {
                        out[n].badges.push(badge);
                    }
                    out[n].members.push(dir.join(member));
                }
            }
        }
        out
    }

    /// The groups these paths head, each path's folder read once: what a choice
    /// gives, whoever answered it. A path heading none has no group, and a path
    /// that is a group's member is no more than that file.
    pub fn of_paths(&self, fs: &SharedFs, paths: &[PathBuf]) -> Vec<FileGroup> {
        if self.is_empty() {
            return Vec::new();
        }
        let mut folders: HashMap<&Path, Vec<FileGroup>> = HashMap::new();
        let mut out = Vec::new();
        for path in paths {
            let Some(dir) = path.parent() else {
                continue;
            };
            let groups = folders.entry(dir).or_insert_with(|| {
                let items = fs.read_dir(dir).unwrap_or_default();
                let names: Vec<&str> = items
                    .iter()
                    .filter(|e| !e.is_dir && !e.name.starts_with('.'))
                    .map(|e| e.name.as_str())
                    .collect();
                self.in_folder(dir, &names)
            });
            if let Some(group) = groups.iter().find(|g| g.main == *path)
                && !out.contains(group)
            {
                out.push(group.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the rule gives them: main, then members.
    fn found(rule: &SidecarRule, names: &[&str]) -> Vec<Vec<String>> {
        rule.groups(names)
            .into_iter()
            .map(|g| {
                let mut line = vec![g.main];
                line.extend(g.members);
                line
            })
            .collect()
    }

    fn photos() -> SidecarRule {
        SidecarRule::new("photos", &["cr3", "nef"], &["xmp", "jpg"])
    }

    #[test]
    fn a_rule_finds_both_forms_of_a_sidecar_and_the_same_stem_only() {
        let names = [
            "a.cr3",
            "a.xmp",
            "a.cr3.xmp",
            "a.jpg",
            "ab.xmp",
            "a.b.cr3",
            "a.b.xmp",
            "lone.jpg",
            "lone.xmp",
            "x.txt",
        ];
        assert_eq!(
            found(&photos(), &names),
            [
                vec!["a.cr3", "a.xmp", "a.cr3.xmp", "a.jpg"],
                vec!["a.b.cr3", "a.b.xmp"],
            ]
        );
    }

    #[test]
    fn extensions_match_in_any_case_and_stems_exactly() {
        let names = ["IMG_1.CR3", "IMG_1.xmp", "IMG_1.CR3.XMP", "img_1.JPG"];
        assert_eq!(
            found(&photos(), &names),
            [vec!["IMG_1.CR3", "IMG_1.xmp", "IMG_1.CR3.XMP"]]
        );
    }

    #[test]
    fn a_sidecar_goes_to_the_first_main_and_the_order_of_mains_wins() {
        let names = ["a.nef", "a.cr3", "a.xmp", "a.nef.xmp"];
        assert_eq!(
            found(&photos(), &names),
            [vec!["a.cr3", "a.xmp"], vec!["a.nef", "a.nef.xmp"]]
        );
        // A JPEG heads its own group only without its RAW.
        let rule = SidecarRule::new("r", &["cr3", "jpg"], &["jpg", "xmp"]);
        assert_eq!(
            found(&rule, &["a.cr3", "a.jpg", "a.xmp", "b.jpg", "b.xmp"]),
            [vec!["a.cr3", "a.jpg", "a.xmp"], vec!["b.jpg", "b.xmp"]]
        );
    }

    #[test]
    fn a_rule_says_its_badges() {
        let rule = photos().badge("JPG", "JPEG");
        assert_eq!(FileGrouping::badge(&rule, "a.jpg"), "JPEG");
        assert_eq!(FileGrouping::badge(&rule, "a.JPG"), "JPEG");
        assert_eq!(FileGrouping::badge(&rule, "a.cr3.xmp"), "XMP");
        assert_eq!(FileGrouping::badge(&rule, "none"), "");
    }

    /// Gives what it is told, whatever the names.
    #[derive(Debug)]
    struct Fixed(&'static str, Vec<(&'static str, Vec<&'static str>)>);

    impl FileGrouping for Fixed {
        fn id(&self) -> &str {
            self.0
        }
        fn groups(&self, _: &[&str]) -> Vec<NameGroup> {
            self.1
                .iter()
                .map(|(main, members)| NameGroup {
                    main: (*main).into(),
                    members: members.iter().map(|m| (*m).into()).collect(),
                })
                .collect()
        }
    }

    #[derive(Debug)]
    struct Panics;

    impl FileGrouping for Panics {
        fn id(&self) -> &str {
            "panics"
        }
        fn groups(&self, _: &[&str]) -> Vec<NameGroup> {
            panic!("this grouping cannot read");
        }
    }

    fn in_d(groupings: &Groupings, names: &[&str]) -> Vec<(String, Vec<String>)> {
        groupings
            .in_folder(Path::new("/d"), names)
            .into_iter()
            .map(|g| {
                let main = g.main.file_name().unwrap().to_string_lossy().into_owned();
                (main, g.member_names())
            })
            .collect()
    }

    #[test]
    fn what_makes_no_sense_stays_a_file_of_its_own() {
        let g = Groupings::default();
        g.register(Arc::new(Fixed(
            "odd",
            vec![
                ("gone", vec!["b"]),
                ("a", vec!["a", "gone", "b"]),
                ("c", vec!["b"]),
                ("b", vec!["c"]),
            ],
        )));
        assert_eq!(in_d(&g, &["a", "b", "c"]), [("a".into(), vec!["b".into()])]);
    }

    #[test]
    fn earlier_groupings_win_and_a_main_gathers_what_later_ones_add() {
        let g = Groupings::default();
        g.register(Arc::new(Fixed("one", vec![("a", vec!["b"])])));
        g.register(Arc::new(Fixed(
            "two",
            vec![("c", vec!["b", "a"]), ("a", vec!["d"]), ("b", vec!["e"])],
        )));
        assert_eq!(
            in_d(&g, &["a", "b", "c", "d", "e"]),
            [("a".into(), vec!["b".into(), "d".into()])]
        );
    }

    #[test]
    fn a_grouping_that_panics_counts_for_nothing() {
        let g = Groupings::default();
        g.register(Arc::new(Panics));
        g.register(Arc::new(Fixed("fine", vec![("a", vec!["b"])])));
        assert_eq!(in_d(&g, &["a", "b"]), [("a".into(), vec!["b".into()])]);
    }

    #[test]
    fn the_same_id_replaces_and_unregistering_takes_it_away() {
        let g = Groupings::default();
        assert!(g.is_empty());
        g.register(Arc::new(Fixed("one", vec![("a", vec!["b"])])));
        g.register(Arc::new(Fixed("one", vec![("b", vec!["a"])])));
        assert_eq!(g.ids(), ["one"]);
        assert_eq!(in_d(&g, &["a", "b"]), [("b".into(), vec!["a".into()])]);
        assert!(g.unregister("one"));
        assert!(!g.unregister("one"));
        assert!(in_d(&g, &["a", "b"]).is_empty());
    }

    #[test]
    fn badges_say_each_thing_once() {
        let g = Groupings::default();
        g.register(Arc::new(photos().badge("jpg", "JPEG")));
        let groups = g.in_folder(Path::new("/d"), &["a.cr3", "a.xmp", "a.cr3.xmp", "a.jpg"]);
        assert_eq!(groups[0].badges, ["XMP", "JPEG"]);
        assert_eq!(from_json(&json!({ "groups": to_json(&groups) })), groups);
        assert!(from_json(&json!({ "paths": [] })).is_empty());
    }
}
