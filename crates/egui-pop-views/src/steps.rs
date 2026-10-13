//! The views' steps (ADR 0011) and the plugin that declares them.
//!
//! Every moment a view acts on (a branch expanded, a selection changed,
//! a drop, a rename, a sort) runs as a step: a plugin may watch it on the
//! journal, veto or redirect it with a before-hook, do it its own way
//! with a run-hook, or follow it with an after-hook. The default (the
//! closure the view passes) changes the view's own state; the
//! application owns its data and applies the change there (from the
//! step's after-hook, or from the view's output signals).
//!
//! A view given no [`AppHandle`] runs its defaults directly: the signals
//! are still in its output, nothing is published.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `tree.load` | `tree`, `key` (`null`: the roots) | asks the source for the branch's children |
//! | `tree.expand` | `tree`, `key`, `expanded` | opens or closes the branch (loading it the first time) |
//! | `tree.select` | `tree`, `keys`, `cursor` | sets the selection |
//! | `tree.drop` | `tree`, `keys`, `target`, `position` (`before`, `after`, `inside`) | moves the nodes in the view |
//! | `tree.rename` | `tree`, `key`, `from`, `to` | renames the node in the view |
//! | `tree.activate` | `tree`, `key` | nothing: the application opens the leaf |
//! | `list.select` | `list`, `keys`, `cursor` | sets the selection |
//! | `table.select` | `table`, `keys`, `cursor` | sets the selection |
//! | `table.sort` | `table`, `column` (`null`: unsorted), `descending` | writes the sort to the store |
//! | `table.resize` | `table`, `widths` | writes the widths to the store |

use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PluginId, StepId};
use serde_json::{Value as Json, json};

pub const TREE_LOAD: StepId = StepId::from_static("tree.load");
pub const TREE_EXPAND: StepId = StepId::from_static("tree.expand");
pub const TREE_SELECT: StepId = StepId::from_static("tree.select");
pub const TREE_DROP: StepId = StepId::from_static("tree.drop");
pub const TREE_RENAME: StepId = StepId::from_static("tree.rename");
pub const TREE_ACTIVATE: StepId = StepId::from_static("tree.activate");
pub const LIST_SELECT: StepId = StepId::from_static("list.select");
pub const TABLE_SELECT: StepId = StepId::from_static("table.select");
pub const TABLE_SORT: StepId = StepId::from_static("table.sort");
pub const TABLE_RESIZE: StepId = StepId::from_static("table.resize");

/// The plugin's id.
pub const VIEWS: PluginId = PluginId::from_static("egui_pop.views");

/// Declares the views' steps. Connect it when views run with an
/// [`AppHandle`]: a step nobody declares is reported.
#[derive(Debug, Default)]
pub struct ViewsPlugin;

/// Every step the views declare, with its description and an example of
/// its arguments.
pub fn specs() -> Vec<StepSpec> {
    vec![
        StepSpec::new(TREE_LOAD, "A tree asks its source for a branch's children.")
            .args(json!({ "tree": "folders", "key": "photos" })),
        StepSpec::new(TREE_EXPAND, "A tree branch opens or closes; a veto keeps it as it is.")
            .args(json!({ "tree": "folders", "key": "photos", "expanded": true })),
        StepSpec::new(TREE_SELECT, "A tree's selection changes.")
            .args(json!({ "tree": "folders", "keys": ["photos"], "cursor": "photos" })),
        StepSpec::new(
            TREE_DROP,
            "Nodes dragged in a tree are dropped before, after or inside a node; a veto refuses the drop.",
        )
        .args(json!({ "tree": "folders", "keys": ["2024"], "target": "archive", "position": "inside" })),
        StepSpec::new(TREE_RENAME, "A tree node is renamed in place; a veto keeps its name.")
            .args(json!({ "tree": "folders", "key": "photos", "from": "Photos", "to": "Pictures" })),
        StepSpec::new(TREE_ACTIVATE, "A tree leaf is opened (Enter, double click).")
            .args(json!({ "tree": "folders", "key": "photos/1.jpg" })),
        StepSpec::new(LIST_SELECT, "A list's selection changes.")
            .args(json!({ "list": "recent", "keys": [3], "cursor": 3 })),
        StepSpec::new(TABLE_SELECT, "A table's selection changes.")
            .args(json!({ "table": "files", "keys": [3], "cursor": 3 })),
        StepSpec::new(
            TABLE_SORT,
            "A table is sorted by a column, or unsorted; the application sorts its rows.",
        )
        .args(json!({ "table": "files", "column": "name", "descending": false })),
        StepSpec::new(TABLE_RESIZE, "A table's columns are resized.")
            .args(json!({ "table": "files", "widths": [200.0, 80.0] })),
    ]
}

impl Plugin for ViewsPlugin {
    fn id(&self) -> PluginId {
        VIEWS
    }

    fn title(&self) -> String {
        "Data views".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for spec in specs() {
            ctx.step(spec)?;
        }
        Ok(())
    }
}

/// Runs a step through the application when there is one, else its
/// default directly.
pub(crate) fn run(
    app: Option<&AppHandle>,
    step: &StepId,
    args: Json,
    default: impl FnOnce(&Json) -> Result<Json, String>,
) -> Outcome {
    match app {
        Some(app) => app.step(step, args, |_, args| default(args)),
        None => Outcome::Done(default(&args)),
    }
}
