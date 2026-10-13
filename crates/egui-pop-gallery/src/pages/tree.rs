//! The tree page: a live tree over a simulated drive the page owns,
//! answered from a thread after a delay; edits made while branches are
//! open; drag and drop with a veto; rename in place, from a name apart
//! from a decorated label; icons and per-row toggles; a context menu
//! for the row right-clicked; a branch of 100 000 files.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::Ui;
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::icon::{Icon, Tone};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, MenuId, PluginId, StoryId};
use egui_pop::views::Mode;
use egui_pop::views::steps::TREE_DROP;
use egui_pop::views::tree::{Node, Position, Signal, Sink, TreeSource, TreeState, TreeView};

use super::section;
use crate::shell::SHELL;

pub const TREE_PAGE: PluginId = PluginId::from_static("gallery.tree");

/// A row's context menu, with its commands: they act on the row
/// right-clicked (`args.key`), not on the selection.
pub const ROW_MENU: MenuId = MenuId::from_static("gallery.tree.row");
pub const CMD_HIDE: CommandId = CommandId::from_static("gallery.tree.hide");
pub const CMD_LOCK: CommandId = CommandId::from_static("gallery.tree.lock");
pub const CMD_RENAME: CommandId = CommandId::from_static("gallery.tree.rename");

/// The context key the drop veto reads.
const VETO: &str = "galleryVetoArchive";

/// How many files the big branch holds.
pub const MANY: usize = 100_000;

/// The drive: each branch's children, by parent (`None`: the roots).
/// The application's data; the tree only shows it.
pub type Drive = Arc<Mutex<HashMap<Option<String>, Vec<Node<String>>>>>;

fn drive() -> Drive {
    let mut d: HashMap<Option<String>, Vec<Node<String>>> = HashMap::new();
    let b = |k: &str, l: &str| Node::branch(k.to_string(), l);
    let f = |k: &str, l: &str| Node::leaf(k.to_string(), l);
    d.insert(
        None,
        vec![
            b("photos", "Photos"),
            b("documents", "Documents"),
            b("archive", "Archive"),
            b("many", "Many files"),
            b("offline", "Offline drive"),
        ],
    );
    d.insert(
        Some("photos".into()),
        vec![
            b("2023", "2023"),
            b("2024", "2024"),
            f("cover.jpg", "cover.jpg"),
        ],
    );
    for (year, n) in [("2023", 12), ("2024", 8)] {
        d.insert(
            Some(year.into()),
            (1..=n)
                .map(|i| {
                    f(
                        &format!("{year}/{i:04}.raw"),
                        &format!("IMG_{year}_{i:04}.raw"),
                    )
                })
                .collect(),
        );
    }
    // Labels decorated with a size, named apart: a rename starts from
    // the name.
    d.insert(
        Some("documents".into()),
        vec![
            b("letters", "Letters"),
            f("notes.txt", "notes.txt · 2 KB").with_name("notes.txt"),
            f("report.pdf", "report.pdf · 18 pages").with_name("report.pdf"),
        ],
    );
    d.insert(
        Some("letters".into()),
        (1..=3)
            .map(|i| f(&format!("letters/{i}"), &format!("letter-{i}.odt")))
            .collect(),
    );
    d.insert(Some("archive".into()), vec![f("old.zip", "old.zip")]);
    Arc::new(Mutex::new(d))
}

/// Answers from a thread after a delay, as a disk or a network would.
#[derive(Debug, Clone)]
pub struct Demo {
    pub drive: Drive,
    pub delay: Duration,
}

impl TreeSource<String> for Demo {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let drive = self.drive.clone();
        let parent = parent.cloned();
        let answer = move || match parent.as_deref() {
            Some("offline") => sink.fail("the drive is not connected"),
            Some("many") => sink.deliver(
                (0..MANY)
                    .map(|i| Node::leaf(format!("many/{i}"), format!("file-{i:06}.dat")))
                    .collect(),
            ),
            _ => {
                let children = drive
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&parent)
                    .cloned()
                    .unwrap_or_default();
                sink.deliver(children);
            }
        };
        if cfg!(target_arch = "wasm32") || self.delay.is_zero() {
            answer();
        } else {
            let delay = self.delay;
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                answer();
            });
        }
    }
}

/// Applies a drop to the drive, as the tree did on screen.
fn move_in_drive(drive: &Drive, keys: &[String], target: &str, position: Position) {
    let mut d = drive.lock().unwrap_or_else(|e| e.into_inner());
    let parent_of = |d: &HashMap<Option<String>, Vec<Node<String>>>, k: &str| {
        d.iter()
            .find(|(_, v)| v.iter().any(|n| n.key == k))
            .map(|(p, _)| p.clone())
    };
    let mut moved = Vec::new();
    for k in keys {
        if let Some(p) = parent_of(&d, k)
            && let Some(list) = d.get_mut(&p)
            && let Some(i) = list.iter().position(|n| n.key == *k)
        {
            moved.push(list.remove(i));
        }
    }
    let parent = match position {
        Position::Inside => Some(target.to_string()),
        _ => parent_of(&d, target).unwrap_or(None),
    };
    let list = d.entry(parent).or_default();
    let at = match position {
        Position::Inside => list.len(),
        Position::Before => list
            .iter()
            .position(|n| n.key == target)
            .unwrap_or(list.len()),
        Position::After => list
            .iter()
            .position(|n| n.key == target)
            .map_or(list.len(), |i| i + 1),
    };
    list.splice(at..at, moved);
}

/// Renames a node in the drive, its label's decoration kept; returns
/// the node as it is now.
fn rename_in_drive(drive: &Drive, key: &str, to: &str) -> Option<Node<String>> {
    let mut d = drive.lock().unwrap_or_else(|e| e.into_inner());
    let n = d.values_mut().flatten().find(|n| n.key == key)?;
    match n.name.take() {
        Some(name) => {
            let decoration = n.label.strip_prefix(&name).unwrap_or("").to_string();
            n.label = format!("{to}{decoration}");
            n.name = Some(to.to_string());
        }
        None => n.label = to.to_string(),
    }
    Some(n.clone())
}

/// What the rows' toggles set: the page's own marks on nodes.
#[derive(Debug, Default)]
pub struct Marks {
    pub hidden: HashSet<String>,
    pub locked: HashSet<String>,
    /// A rename the row menu asked for, started at the next pass.
    pub rename: Option<String>,
}

impl Marks {
    fn toggle(set: &mut HashSet<String>, key: &str) {
        if !set.remove(key) {
            set.insert(key.to_string());
        }
    }
}

/// The page's state.
#[derive(Debug)]
pub struct PageState {
    pub tree: TreeState<String>,
    pub source: Demo,
    pub log: Vec<String>,
    pub marks: Rc<RefCell<Marks>>,
    added: usize,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            tree: TreeState::new(),
            source: Demo {
                drive: drive(),
                delay: Duration::from_millis(300),
            },
            log: Vec::new(),
            marks: Rc::default(),
            added: 0,
        }
    }
}

/// The tree page.
#[derive(Debug, Default)]
pub struct TreePage;

impl Plugin for TreePage {
    fn id(&self) -> PluginId {
        TREE_PAGE
    }

    fn title(&self) -> String {
        "Tree".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::steps::VIEWS]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(TREE_DROP, |app, args| {
            if app.context().get(VETO) == "true" && args["target"] == "archive" {
                Before::Veto("Archive is read-only on this page".into())
            } else {
                Before::Go
            }
        });
        let page = Rc::new(RefCell::new(PageState::default()));
        let marks = page.borrow().marks.clone();
        let row_command = |id: CommandId, title: &str, description: &str, order: i32| {
            CommandSpec::new(id, title, "Tree")
                .description(description)
                .args(serde_json::json!({ "key": "photos" }))
                .hidden()
                .menu(ROW_MENU, "1_row", order)
        };
        let key_of = |call: &egui_pop::api::Call<'_>| -> Result<String, String> {
            call.args["key"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "`key` names no node".to_string())
        };
        let m = marks.clone();
        ctx.command(
            row_command(
                CMD_HIDE,
                "Hide or show",
                "Hides the row's node, or shows it again.",
                0,
            ),
            move |call| {
                let key = key_of(call)?;
                Marks::toggle(&mut m.borrow_mut().hidden, &key);
                Ok(serde_json::json!({ "key": key }))
            },
        )?;
        let m = marks.clone();
        ctx.command(
            row_command(
                CMD_LOCK,
                "Lock or unlock",
                "Locks the row's node, or unlocks it.",
                1,
            ),
            move |call| {
                let key = key_of(call)?;
                Marks::toggle(&mut m.borrow_mut().locked, &key);
                Ok(serde_json::json!({ "key": key }))
            },
        )?;
        let m = marks;
        ctx.command(
            row_command(CMD_RENAME, "Rename…", "Renames the row's node in place.", 2),
            move |call| {
                let key = key_of(call)?;
                m.borrow_mut().rename = Some(key.clone());
                Ok(serde_json::json!({ "key": key }))
            },
        )?;
        ctx.story(
            Story::new(StoryId::from_static("tree"), "Tree", move |ui, app| {
                show(ui, app, &mut page.borrow_mut())
            })
            .description("A live tree over a source the application implements: branches loaded on demand from a thread, edits at any time, selection, keyboard, drag and drop with a veto, rename in place, 100 000 nodes.")
            .group("Components", 76),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, p: &mut PageState) {
    ui.label("Open a folder: its files come from a thread after 0.3 s. Drag files between folders, F2 renames, letters jump to a name. The eye and the lock at a row's end act on that row without selecting it, and so does a row's context menu.");
    ui.horizontal_wrapped(|ui| {
        let handle = p.tree.handle();
        if ui.button("Add a photo to Photos").clicked() {
            p.added += 1;
            let node = Node::leaf(format!("added/{}", p.added), format!("new-{}.jpg", p.added));
            p.source
                .drive
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(Some("photos".into()))
                .or_default()
                .insert(0, node.clone());
            handle.insert(Some("photos".into()), Some(0), node);
        }
        if ui.button("Remove the first photo").clicked() {
            let gone = {
                let mut d = p.source.drive.lock().unwrap_or_else(|e| e.into_inner());
                let list = d.entry(Some("photos".into())).or_default();
                let i = list.iter().position(|n| !n.branch);
                i.map(|i| list.remove(i))
            };
            if let Some(n) = gone {
                handle.remove(n.key);
            }
        }
        if ui.button("Reload everything").clicked() {
            handle.reload(None);
        }
    });
    let mut veto = app.context().get(VETO) == "true";
    if ui
        .checkbox(
            &mut veto,
            "Veto drops into Archive (a before-hook on tree.drop)",
        )
        .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }
    section(ui, "Files");
    let marks = p.marks.clone();
    let asked = marks.borrow_mut().rename.take();
    if let Some(key) = asked {
        p.tree.rename(&key);
    }
    let out = TreeView::<String>::new("gallery.files")
        .label("Files")
        .mode(Mode::Multiple)
        .drag_drop(true)
        .rename(true)
        .app(app)
        .height(300.0)
        .icon(|n| Some(if n.branch { Icon::Folder } else { Icon::File }.into()))
        .row_widgets(|ui, n| row_toggles(ui, &marks, &n.key, &n.label))
        .context_menu(app, ROW_MENU)
        .show(ui, &mut p.tree, &p.source);
    for s in out.signals {
        match &s {
            Signal::Drop {
                keys,
                target,
                position,
            } => move_in_drive(&p.source.drive, keys, target, *position),
            Signal::Rename { key, to, .. } => {
                if let Some(node) = rename_in_drive(&p.source.drive, key, to) {
                    p.tree.handle().update(node);
                }
            }
            _ => {}
        }
        p.log.push(describe(&s));
    }
    let sel = p.tree.selection();
    ui.label(format!(
        "Selected: {}",
        if sel.is_empty() {
            "nothing".to_string()
        } else {
            sel.keys()
                .iter()
                .filter_map(|k| p.tree.model().node(k).map(|n| n.label.clone()))
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));
    section(ui, "Signals");
    if p.log.is_empty() {
        ui.weak("Nothing yet.");
    }
    let skip = p.log.len().saturating_sub(5);
    for line in &p.log[skip..] {
        ui.monospace(line);
    }
}

/// A row's eye and lock, right to left: they change the page's marks,
/// not the selection.
fn row_toggles(ui: &mut Ui, marks: &RefCell<Marks>, key: &str, label: &str) {
    let (hidden, locked) = {
        let m = marks.borrow();
        (m.hidden.contains(key), m.locked.contains(key))
    };
    let (icon, name) = if hidden {
        (Icon::Hidden, format!("Show {label}"))
    } else {
        (Icon::Visible, format!("Hide {label}"))
    };
    let tone = if hidden { Tone::Muted } else { Tone::Default };
    if ui.add(icon.tone(tone).label(&name).sense_click()).clicked() {
        Marks::toggle(&mut marks.borrow_mut().hidden, key);
    }
    let (icon, name) = if locked {
        (Icon::Lock, format!("Unlock {label}"))
    } else {
        (Icon::Unlock, format!("Lock {label}"))
    };
    let tone = if locked { Tone::Warning } else { Tone::Muted };
    if ui.add(icon.tone(tone).label(&name).sense_click()).clicked() {
        Marks::toggle(&mut marks.borrow_mut().locked, key);
    }
}

fn describe(s: &Signal<String>) -> String {
    match s {
        Signal::Load { key } => format!("tree.load {}", key.as_deref().unwrap_or("(roots)")),
        Signal::Expand { key, expanded } => format!("tree.expand {key} {expanded}"),
        Signal::Select { keys, .. } => format!("tree.select {}", keys.join(", ")),
        Signal::Drop {
            keys,
            target,
            position,
        } => format!("tree.drop {} {position:?} {target}", keys.join(", ")),
        Signal::Rename { from, to, .. } => format!("tree.rename {from} -> {to}"),
        Signal::Activate { key } => format!("tree.activate {key}"),
        Signal::Context { key } => format!("tree.context {key}"),
        Signal::Vetoed { step, reason } => format!("{step} vetoed: {reason}"),
    }
}
