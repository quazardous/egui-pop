//! Settings export and import, in the Settings app: General › Advanced
//! holds "Export settings…" and "Import settings…", each a wizard of the
//! wizard kit, and both are commands an agent or a test runs alone.
//!
//! - **Export** (`settings.export`): the settings the person changed (the
//!   `app` value scope), or a library's or workspace's values (`layer`),
//!   every setting or those of one scope or section (`place`); with the
//!   person's shortcuts (`shortcuts`) and the settings custom pages edit
//!   (`pages`, on by default). A sensitive setting
//!   ([`Descriptor::sensitive`](egui_pop_model::values::Descriptor::sensitive))
//!   is never written: it is listed under `leftOut`. The file is the
//!   `settings-export` document ([`egui_pop_model::admin::transfer`]).
//!   `dryRun` answers the document without writing it.
//! - **Import** (`settings.import`): reads a file (`file`, or the
//!   document itself as `document`), judges each value against the
//!   setting declared here (a change, the same value, unknown, obsolete,
//!   invalid, sensitive; [`transfer::plan`]),
//!   and applies the changes chosen (`keys`; `null`: every change) into a
//!   value scope (`scope`, `app` by default) as **one undo step** of the
//!   settings' history (`Ctrl+Z` in the Settings app). A value of the
//!   wrong type or out of range is never applied: it is skipped with its
//!   reason. `dryRun` (and the read-only `settings.importPreview`) answer
//!   the plan without applying it. `settings.undoImport` takes the last
//!   import back, a library's or workspace's layer and the shortcuts
//!   included.
//! - **Obsolete settings**: an application names the settings it retired
//!   ([`Admin::obsolete`]), so an old file's values for them are flagged,
//!   not applied.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::admin::transfer::{self, Declared, Row, SHORTCUTS, SettingsFile, Verdict};
use egui_pop_model::admin::{
    Action, FORMAT, Field, FieldKind, Item, Manifest, Records, Section, Tree, ValueScope, Wizard,
    cell_text,
};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::history::SETTINGS;
use egui_pop_model::ids::{CommandId, OwnerId, SettingId};
use egui_pop_model::keymap::{KeymapDiff, KeymapDoc, Layer};
use egui_pop_model::values::{Change, Set, enter_history};
use serde_json::{Value as Json, json};

use super::{ADMIN, Admin, scope_named};
use crate::app::AppHandle;

/// Writes the chosen settings to a file.
pub const CMD_EXPORT: CommandId = CommandId::from_static("settings.export");
/// Reads a settings file and applies the changes chosen (or, `dryRun`,
/// says what it would do).
pub const CMD_IMPORT: CommandId = CommandId::from_static("settings.import");
/// What an import would do: read-only, for the import wizard's table.
pub const CMD_IMPORT_PREVIEW: CommandId = CommandId::from_static("settings.importPreview");
/// Takes the last import back.
pub const CMD_UNDO_IMPORT: CommandId = CommandId::from_static("settings.undoImport");

/// The export wizard's id.
pub const EXPORT_WIZARD: &str = "settings-export";
/// The import wizard's id.
pub const IMPORT_WIZARD: &str = "settings-import";

/// What an export takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// The value scope read: `app` (the settings the person changed), or
    /// a library's or workspace's layer.
    pub layer: ValueScope,
    /// A scope (`interface`) or a section (`interface/theme`) of the
    /// Settings app; `None`: every setting.
    pub place: Option<String>,
    /// The person's shortcuts too.
    pub shortcuts: bool,
    /// The settings custom pages edit (registered widget kinds).
    pub pages: bool,
}

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            layer: ValueScope::App,
            place: None,
            shortcuts: false,
            pages: true,
        }
    }
}

/// The last import, for its undo.
#[derive(Clone, Debug)]
struct LastImport {
    scope: ValueScope,
    /// The undo step of the settings' history it made (`app`).
    step: Option<u64>,
    /// The layer as it was (`library`, `workspace`).
    layer: Option<BTreeMap<String, Json>>,
    /// The person's shortcuts as they were.
    keymap: Option<KeymapDoc>,
}

/// What a plan was read for: the file (path, size, time), the scope, the
/// store's sequence.
type CacheKey = (String, u64, Option<std::time::SystemTime>, ValueScope, u64);

/// A plan read, and what it was read for.
type Cached = Option<(CacheKey, Result<Vec<Row>, String>)>;

/// The transfer's state, kept by the `admin` service.
#[derive(Debug, Default)]
pub(super) struct State {
    obsolete: Mutex<BTreeMap<String, String>>,
    last: Mutex<Option<LastImport>>,
    cache: Mutex<Cached>,
}

/// Reads a settings file from disk.
pub fn read_file(path: &str) -> Result<SettingsFile, String> {
    if path.trim().is_empty() {
        return Err("no file chosen".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read `{path}`: {e}"))?;
    SettingsFile::read(&text)
}

/// The settings a section edits: its fields' and its widgets'.
fn section_settings(s: &Section) -> Vec<String> {
    s.groups
        .iter()
        .flat_map(|g| &g.items)
        .flat_map(|i| match i {
            Item::Field(f) => f.setting_id().map(str::to_string).into_iter().collect(),
            Item::Widget(w) => w.settings().map(str::to_string).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// The settings registered widget kinds edit (custom pages, fonts).
fn page_settings(tree: &Tree) -> BTreeSet<String> {
    tree.sections
        .iter()
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.items)
        .flat_map(|i| match i {
            Item::Widget(w) => w.settings().map(str::to_string).collect(),
            _ => Vec::new(),
        })
        .collect()
}

impl Admin {
    /// Names a setting the application retired, with what replaced it:
    /// an imported file's value for it is flagged obsolete, not applied.
    pub fn obsolete(&self, id: &str, note: &str) {
        lock(&self.transfer.obsolete).insert(id.into(), note.into());
    }

    /// The settings file of a selection: the values, the shortcuts, the
    /// sensitive settings left out.
    pub fn export_settings(
        &self,
        app: &AppHandle,
        selection: &Selection,
    ) -> Result<SettingsFile, String> {
        if selection.layer != ValueScope::App && !self.offered().contains(&selection.layer) {
            return Err(format!("no {} is open", selection.layer.name()));
        }
        let tree = self.tree(app);
        let mut ids: BTreeSet<String> = match selection.place.as_deref().filter(|p| !p.is_empty()) {
            None => app
                .settings()
                .iter()
                .map(|e| e.id.as_str().to_string())
                .collect(),
            Some(place) => {
                let sections: Vec<&Section> = tree
                    .sections
                    .iter()
                    .filter(|s| s.scope == place || s.path() == place)
                    .collect();
                if sections.is_empty() {
                    return Err(format!("no place `{place}` in the settings"));
                }
                sections.into_iter().flat_map(section_settings).collect()
            }
        };
        if !selection.pages {
            let pages = page_settings(&tree);
            ids.retain(|id| !pages.contains(id));
        }
        let store = app.store();
        let s = lock(&store);
        let layers = self.layers();
        let values: BTreeMap<String, Json> = match selection.layer {
            ValueScope::App => ids
                .iter()
                .filter_map(|id| {
                    let path = SettingId::new(id).ok()?.path();
                    let e = s.get(&path)?;
                    let v = layers.app_value(&path, &e.value.to_json());
                    (v != e.descriptor.default.to_json()).then(|| (id.clone(), v))
                })
                .collect(),
            scope => self
                .export_layer(scope)
                .into_iter()
                .filter(|(id, _)| ids.contains(id))
                .collect(),
        };
        let sensitive = |id: &str| {
            SettingId::new(id)
                .ok()
                .and_then(|i| s.get(&i.path()).map(|e| e.descriptor.sensitive))
                .unwrap_or(false)
        };
        let (values, left_out) = transfer::without_secrets(values, &sensitive);
        drop(s);
        let mut file = SettingsFile::new(
            values,
            selection.shortcuts.then(|| app.export_keymap(Layer::User)),
        );
        file.app = app.app_id();
        file.left_out = left_out;
        file.selection = json!({
            "layer": selection.layer.name(),
            "place": selection.place,
        });
        Ok(file)
    }

    /// What importing a file into a value scope would do, row by row.
    pub fn import_plan(&self, app: &AppHandle, file: &SettingsFile, scope: ValueScope) -> Vec<Row> {
        let ctx = app.egui();
        let store = app.store();
        let layers = self.layers();
        let obsolete = lock(&self.transfer.obsolete).clone();
        let mut rows = {
            let s = lock(&store);
            let declared = |key: &str| {
                let id = SettingId::new(key).ok()?;
                let path = id.path();
                let e = s.get(&path)?;
                let now = e.value.to_json();
                Some(Declared {
                    title: strings::setting_label(&ctx, key, &e.descriptor.label),
                    descriptor: e.descriptor.clone(),
                    current: layers.value_in(scope, &path, &now).unwrap_or(now),
                })
            };
            transfer::plan(file, &declared, &|k| obsolete.get(k).cloned())
        };
        if let Some(s) = &file.shortcuts {
            let now = app.export_keymap(Layer::User);
            let d = KeymapDiff::between(&now, &s.keymap);
            let n = d.added.len() + d.removed.len() + d.changed.len();
            let verdict = if n == 0 {
                Verdict::Same
            } else {
                Verdict::Change
            };
            rows.push(Row {
                key: SHORTCUTS.into(),
                title: "Keyboard shortcuts".into(),
                verdict,
                current: json!(format!("{} of your own", now.bindings.len())),
                imported: json!(format!("{} of your own", s.keymap.bindings.len())),
                reason: format!(
                    "{} added, {} removed, {} changed",
                    d.added.len(),
                    d.removed.len(),
                    d.changed.len()
                ),
                tone: verdict.tone().into(),
            });
        }
        rows
    }

    /// The plan of a file on disk, read again only when the file, the
    /// scope or the store changed (the import wizard asks each frame).
    fn plan_of_path(
        &self,
        app: &AppHandle,
        path: &str,
        scope: ValueScope,
    ) -> Result<Vec<Row>, String> {
        let meta = std::fs::metadata(path).ok();
        let key: CacheKey = (
            path.to_string(),
            meta.as_ref().map_or(0, std::fs::Metadata::len),
            meta.and_then(|m| m.modified().ok()),
            scope,
            lock(&app.store()).seq(),
        );
        if let Some((k, rows)) = lock(&self.transfer.cache).as_ref()
            && *k == key
        {
            return rows.clone();
        }
        let rows = read_file(path).map(|f| self.import_plan(app, &f, scope));
        *lock(&self.transfer.cache) = Some((key, rows.clone()));
        rows
    }

    /// Applies a file's changes into a value scope: those `keys` names
    /// (`None`: every change), as one undo step. What is not applied is
    /// said, with its reason.
    pub fn import_settings(
        &self,
        app: &AppHandle,
        file: &SettingsFile,
        scope: ValueScope,
        keys: Option<&[String]>,
    ) -> Result<Json, String> {
        if !scope.editable() {
            return Err("the command line's values are not set by an import".into());
        }
        if scope != ValueScope::App && !self.offered().contains(&scope) {
            return Err(format!("no {} is open", scope.name()));
        }
        let rows = self.import_plan(app, file, scope);
        let chosen = |k: &str| keys.is_none_or(|ks| ks.iter().any(|x| x == k));
        let mut skipped: Vec<Json> = Vec::new();
        let mut apply: Vec<&Row> = Vec::new();
        for r in &rows {
            match (r.verdict, chosen(&r.key)) {
                (Verdict::Change, true) => apply.push(r),
                (Verdict::Change, false) => {
                    skipped.push(json!({ "key": r.key, "reason": "not chosen" }));
                }
                (Verdict::Same, _) => {}
                _ => skipped.push(json!({ "key": r.key, "reason": r.reason })),
            }
        }
        for k in keys.unwrap_or_default() {
            if !rows.iter().any(|r| r.key == *k) {
                skipped.push(json!({ "key": k, "reason": "not in the file" }));
            }
        }
        let layer_before = (scope != ValueScope::App).then(|| self.export_layer(scope));
        let store = app.store();
        let layers = self.layers();
        let group = {
            let _h = enter_history(&SETTINGS);
            lock(&store).begin_group(Origin::User, "Import settings")
        };
        let mut applied = Vec::new();
        let mut keymap_before = None;
        for r in apply {
            if r.key == SHORTCUTS {
                if let Some(s) = &file.shortcuts {
                    keymap_before = Some(app.export_keymap(Layer::User));
                    app.load_keymap(s.keymap.clone(), Layer::User);
                    applied.push(r.key.clone());
                }
                continue;
            }
            let Ok(id) = SettingId::new(&r.key) else {
                continue;
            };
            let path = id.path();
            let plain = scope == ValueScope::App && layers.origin(&path) == ValueScope::App;
            let result = if plain {
                let mut s = lock(&store);
                let value = s
                    .get(&path)
                    .ok_or_else(|| format!("no setting at `{path}`"))
                    .and_then(|e| e.descriptor.accept(&r.imported));
                value.and_then(|v| {
                    s.set(
                        Set::new(&path, Change::To(v), Origin::User)
                            .by(&OwnerId::FRAMEWORK)
                            .gesture(group),
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
                })
            } else {
                self.set_in(app, scope, &id, r.imported.clone())
            };
            match result {
                Ok(()) => applied.push(r.key.clone()),
                Err(reason) => skipped.push(json!({ "key": r.key, "reason": reason })),
            }
        }
        let step = {
            let mut s = lock(&store);
            s.end(group);
            s.undo_steps_in(&SETTINGS)
                .last()
                .filter(|x| x.group == Some(group.0))
                .map(|x| x.id)
        };
        if !applied.is_empty() {
            *lock(&self.transfer.last) = Some(LastImport {
                scope,
                step,
                layer: layer_before,
                keymap: keymap_before,
            });
        }
        self.refresh();
        Ok(json!({
            "scope": scope.name(),
            "applied": applied,
            "skipped": skipped,
            "undo": (!applied.is_empty()).then_some(CMD_UNDO_IMPORT.as_str()),
        }))
    }

    /// Takes the last import back: its undo step (refused when the
    /// settings changed since: undo those first), the layer and the
    /// shortcuts as they were.
    pub fn undo_import(&self, app: &AppHandle) -> Result<Json, String> {
        let last = lock(&self.transfer.last)
            .clone()
            .ok_or("no import to undo")?;
        if let Some(step) = last.step {
            let store = app.store();
            let mut s = lock(&store);
            if s.undo_steps_in(&SETTINGS).last().map(|x| x.id) != Some(step) {
                return Err(
                    "the settings changed since the import: undo those changes first".into(),
                );
            }
            s.undo_in(&SETTINGS, &Origin::User)
                .map_err(|e| e.to_string())?;
        }
        if let Some(layer) = last.layer {
            self.load_layer(app, last.scope, layer);
        }
        if let Some(keymap) = last.keymap {
            app.load_keymap(keymap, Layer::User);
        }
        lock(&self.transfer.last).take();
        self.refresh();
        Ok(json!({ "undone": true, "scope": last.scope.name() }))
    }

    /// Whether an import can be undone.
    pub fn can_undo_import(&self) -> bool {
        lock(&self.transfer.last).is_some()
    }
}

fn admin(call: &crate::app::Call<'_>) -> Result<std::sync::Arc<Admin>, String> {
    call.app.service(&ADMIN).ok_or_else(|| "no admin".into())
}

fn text_arg<'a>(args: &'a Json, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
}

fn bool_arg(args: &Json, key: &str, default: bool) -> bool {
    args.get(key).and_then(Json::as_bool).unwrap_or(default)
}

fn scope_of(args: &Json, key: &str) -> Result<ValueScope, String> {
    let name = text_arg(args, key).unwrap_or("app");
    scope_named(name).ok_or_else(|| format!("no value scope `{name}`"))
}

/// The file an import reads: `document` (the JSON itself), else `file`.
fn source(args: &Json) -> Result<SettingsFile, String> {
    match args.get("document").filter(|d| d.is_object()) {
        Some(d) => SettingsFile::from_json(d.clone()),
        None => read_file(text_arg(args, "file").ok_or("`file` is missing")?),
    }
}

fn keys_of(args: &Json) -> Option<Vec<String>> {
    args.get("keys").and_then(Json::as_array).map(|a| {
        a.iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// An import's plan as its commands answer it.
fn plan_answer(rows: &[Row], scope: ValueScope) -> Json {
    let (changes, rest): (Vec<&Row>, Vec<&Row>) = rows.iter().partition(|r| r.verdict.applies());
    json!({
        "scope": scope.name(),
        "rows": rows,
        "changes": changes,
        "leftOut": rest,
    })
}

/// The commands.
pub(super) fn commands(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::PluginError> {
    ctx.command(
        CommandSpec::new(CMD_EXPORT, "Export settings", "File")
            .hidden()
            .description("Writes the settings you changed to a file; secrets are left out.")
            .args(json!({ "file": "", "layer": "app", "place": "", "shortcuts": false, "pages": true, "dryRun": false })),
        |call| {
            let a = admin(call)?;
            let args = call.args;
            let selection = Selection {
                layer: scope_of(args, "layer")?,
                place: text_arg(args, "place").map(str::to_string),
                shortcuts: bool_arg(args, "shortcuts", false),
                pages: bool_arg(args, "pages", true),
            };
            let file = a.export_settings(call.app, &selection)?;
            let text = file.write()?;
            let dry = bool_arg(args, "dryRun", false);
            let path = text_arg(args, "file");
            if !dry {
                let path = path.ok_or("`file` is missing")?;
                std::fs::write(path, &text).map_err(|e| format!("cannot write `{path}`: {e}"))?;
            }
            let document: Json = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            Ok(json!({
                "file": path,
                "written": !dry,
                "keys": file.settings.values.keys().collect::<Vec<_>>(),
                "shortcuts": file.shortcuts.is_some(),
                "leftOut": file.left_out,
                "document": document,
            }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_IMPORT, "Import settings", "File")
            .hidden()
            .description("Reads a settings file: what it would change, then the changes chosen, as one undo step.")
            .args(json!({ "file": "", "document": null, "scope": "app", "keys": null, "dryRun": false })),
        |call| {
            let a = admin(call)?;
            let args = call.args;
            let file = source(args)?;
            let scope = scope_of(args, "scope")?;
            if bool_arg(args, "dryRun", false) {
                return Ok(plan_answer(&a.import_plan(call.app, &file, scope), scope));
            }
            let keys = keys_of(args);
            a.import_settings(call.app, &file, scope, keys.as_deref())
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_IMPORT_PREVIEW, "Preview a settings import", "File")
            .hidden()
            .read_only()
            .args(json!({ "file": "", "document": null, "scope": "app" })),
        |call| {
            let a = admin(call)?;
            let scope = scope_of(call.args, "scope")?;
            let rows = match (call.args.get("document"), text_arg(call.args, "file")) {
                (Some(d), _) if d.is_object() => {
                    a.import_plan(call.app, &SettingsFile::from_json(d.clone())?, scope)
                }
                (_, Some(path)) => a.plan_of_path(call.app, path, scope)?,
                _ => return Err("`file` is missing".into()),
            };
            Ok(plan_answer(&rows, scope))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_UNDO_IMPORT, "Undo the last settings import", "File").hidden(),
        |call| admin(call)?.undo_import(call.app),
    )?;
    Ok(())
}

/// The hooks the wizards name.
pub(super) fn hooks(admin: &Admin) {
    admin.hook(
        "settings.importFile",
        egui_pop_model::admin::hooks::Hook::Validate(Box::new(file_check())),
    );
}

/// The import wizard's file check: the file must read as a settings
/// file. Read again only when the path, the size or the time change.
fn file_check()
-> impl Fn(&egui_pop_model::admin::hooks::Values) -> Vec<egui_pop_model::admin::hooks::Problem>
+ Send
+ Sync {
    use egui_pop_model::admin::Tone;
    use egui_pop_model::admin::hooks::Problem;
    type Key = (String, u64, Option<std::time::SystemTime>);
    let seen: Mutex<Option<(Key, Option<String>)>> = Mutex::new(None);
    move |values| {
        let path = values.get("file").and_then(Json::as_str).unwrap_or("");
        if path.trim().is_empty() {
            return Vec::new();
        }
        let meta = std::fs::metadata(path).ok();
        let key: Key = (
            path.to_string(),
            meta.as_ref().map_or(0, std::fs::Metadata::len),
            meta.and_then(|m| m.modified().ok()),
        );
        let mut seen = lock(&seen);
        let error = match seen.as_ref() {
            Some((k, e)) if *k == key => e.clone(),
            _ => {
                let e = read_file(path).err();
                *seen = Some((key, e.clone()));
                e
            }
        };
        error
            .map(|message| Problem {
                field: "file".into(),
                tone: Tone::Danger,
                message,
            })
            .into_iter()
            .collect()
    }
}

/// The Settings app's part: the two wizards and their entries in General
/// › Advanced. Built with the tree (the places an export offers) and the
/// import running (the changes its file holds, as the choices of what to
/// apply).
pub(super) fn manifest(admin: &Admin, app: &AppHandle, tree: &Tree) -> Manifest {
    let offered = admin.offered();
    let layers = offered.len() > 1;
    let draft = |task: &str, key: &str, title: &str, kind| Field::draft(task, key, title, kind);
    // Export: what, then where.
    let mut of = draft(EXPORT_WIZARD, "of", "Export", FieldKind::Choice)
        .choice(json!("all"), "Every setting you changed")
        .default_value(json!("all"));
    if layers {
        of = of.choice(json!("layer"), "The values of a library or workspace");
    }
    of = of.choice(json!("place"), "One family or section of the settings");
    let mut layer =
        draft(EXPORT_WIZARD, "layer", "Values of", FieldKind::Choice).default_value(json!("app"));
    for s in &offered {
        layer = layer.choice(json!(s.name()), scope_words(*s));
    }
    layer.meta.when = Some("form.of == 'layer'".into());
    let mut place = draft(
        EXPORT_WIZARD,
        "place",
        "Family or section",
        FieldKind::Choice,
    );
    let mut first = None;
    for scope in tree.shown_scopes() {
        let sections: Vec<&Section> = tree
            .sections_of(&scope.meta.id)
            .into_iter()
            .filter(|s| !section_settings(s).is_empty())
            .collect();
        if sections.is_empty() {
            continue;
        }
        first.get_or_insert_with(|| scope.meta.id.clone());
        place = place.choice(json!(scope.meta.id), &scope.meta.title);
        for s in sections {
            place = place.choice(
                json!(s.path()),
                &format!("{} › {}", scope.meta.title, s.meta.title),
            );
        }
    }
    place = place.default_value(first.map_or(Json::Null, Json::String));
    place.meta.when = Some("form.of == 'place'".into());
    let export = Wizard::new(EXPORT_WIZARD, "Export settings", CMD_EXPORT.as_str())
        .description("Writes your settings to a file, to keep or to bring to another computer. Secrets are never written.")
        .icon("upload")
        .step("what", "What", |s| {
            s.field(of.clone()).field(layer.clone()).field(place.clone()).field(
                draft(EXPORT_WIZARD, "shortcuts", "Your keyboard shortcuts too", FieldKind::Toggle)
                    .default_value(json!(true)),
            )
            .field(
                draft(EXPORT_WIZARD, "pages", "The settings of custom pages", FieldKind::Toggle)
                    .default_value(json!(true))
                    .help("Pages an application or a plugin draws itself (fonts, previews)."),
            );
        })
        .step("file", "File", |s| {
            s.field(
                draft(EXPORT_WIZARD, "file", "File", FieldKind::Path)
                    .save_file("settings.json", &["json"])
                    .required(),
            );
        })
        .args(json!({
            "file": "${file}",
            "layer": "${layer}",
            "place": "${place}",
            "shortcuts": "${shortcuts}",
            "pages": "${pages}",
        }))
        .verb("Export");
    // Import: the file and the scope, then what changes.
    let flow = admin.wizard_flow().filter(|f| f.wizard == IMPORT_WIZARD);
    let (path, scope) = flow.as_ref().map_or((String::new(), ValueScope::App), |f| {
        (
            f.values
                .get("file")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            f.values
                .get("scope")
                .and_then(Json::as_str)
                .and_then(scope_named)
                .unwrap_or(ValueScope::App),
        )
    });
    let mut keys = draft(IMPORT_WIZARD, "keys", "Apply", FieldKind::Multi)
        .help("The changes the file holds: untick those to keep as they are.");
    if !path.trim().is_empty()
        && let Ok(rows) = admin.plan_of_path(app, &path, scope)
    {
        for r in rows.iter().filter(|r| r.verdict.applies()) {
            let label = if r.key == SHORTCUTS {
                format!("{}: {}", r.title, r.reason)
            } else {
                format!(
                    "{}: {} → {}",
                    r.title,
                    cell_text(&r.current),
                    cell_text(&r.imported)
                )
            };
            keys = keys.choice(json!(r.key), &label);
        }
    }
    let mut scope_field =
        draft(IMPORT_WIZARD, "scope", "Into", FieldKind::Choice).default_value(json!("app"));
    for s in &offered {
        scope_field = scope_field.choice(json!(s.name()), scope_words(*s));
    }
    let mut args = json!({ "file": "${file}", "keys": "${keys}" });
    let mut preview = json!({ "file": "${file}" });
    if layers {
        args["scope"] = json!("${scope}");
        preview["scope"] = json!("${scope}");
    }
    let import = Wizard::new(IMPORT_WIZARD, "Import settings", CMD_IMPORT.as_str())
        .description("Reads settings from a file: you see what would change before anything does, and the import is one undo step.")
        .icon("download")
        .step("file", "File", |s| {
            s.field(
                draft(IMPORT_WIZARD, "file", "File", FieldKind::Path)
                    .open_file(&["json"])
                    .required()
                    .validate("settings.importFile"),
            );
            if layers {
                s.field(scope_field.clone());
            }
        })
        .step("changes", "Changes", |s| {
            s.group("apply", "What changes").field(keys.clone());
            s.group("left", "Left out").table(
                Records::command(
                    "leftOut",
                    "",
                    CMD_IMPORT_PREVIEW.as_str(),
                    "/leftOut",
                    "key",
                )
                .args(preview.clone())
                .column("title", "Setting")
                .column("imported", "In the file")
                .column("current", "Now")
                .column("reason", "Why")
                .tone("tone")
                .empty("Nothing is left out.", None),
            );
        })
        .args(args)
        .verb("Import");
    let mut m = Manifest::new(FORMAT).wizard(export).wizard(import).section(
        "general/advanced",
        "Advanced",
        |s| {
            let open = |id: &str, title: &str, wizard: &str| {
                let mut a = Action::command(super::CMD_OPEN.as_str())
                    .title(title)
                    .args(json!({ "wizard": wizard }));
                // Two actions of one command: each its own id.
                a.meta.id = id.into();
                a
            };
            s.group("transfer", "Settings file")
                .action(open("export", "Export settings…", EXPORT_WIZARD))
                .action(open("import", "Import settings…", IMPORT_WIZARD));
            if admin.can_undo_import() {
                s.action(Action::command(CMD_UNDO_IMPORT.as_str()).title("Undo the last import"));
            }
        },
    );
    for s in &mut m.sections {
        s.meta.order = 2000;
    }
    m
}

/// A value scope as the wizards say it.
fn scope_words(s: ValueScope) -> &'static str {
    match s {
        ValueScope::App => "Your settings",
        ValueScope::Library => "This library",
        ValueScope::Workspace => "This workspace",
        ValueScope::Run => "The command line",
    }
}
