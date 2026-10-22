//! Pointer gestures in an application, owned by egui-pop: the pointer
//! profile setting (`input.pointer_profile`: Document, darktable,
//! Photoshop, Lightroom Classic, Blender / node editors, Trackpad;
//! Document unless the application picks another with
//! [`AppBuilder::pointer_profile`]; a retired id such as `adobe` names
//! its successor, `lightroom`), the options
//! each profile gives its default for (`input.slider_wheel`,
//! `input.image_double_click`), the wheel device (`input.wheel_device`),
//! and the gesture commands (`gesture.image.zoom`...) the profile binds
//! to pointer chords in the keymap's default layer
//! (`docs/pointer-gestures.md`).
//!
//! The user's changes are ordinary user-layer bindings, made in the
//! shortcuts editor: they stay over the profile. When the profile
//! changes and some exist, a dialog asks whether to keep them on the
//! new profile or drop them (`pointer.overrides`, `{ "value": "keep" |
//! "drop" }`).
//!
//! The application publishes what is in force to its context
//! (`egui_pop_core::pointer::set_map`) after each change of the keymap
//! or of these settings: the canvases read it.
//!
//! [`AppBuilder::pointer_profile`]: crate::AppBuilder::pointer_profile

use std::sync::Arc;

use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, SettingId};
use egui_pop_model::keymap::{Keymap, Layer};
use egui_pop_model::pointer::{
    self, GESTURES, ImageDoubleClick, PROFILE_OWNER, PointerMap, Profile, SliderWheel, WheelDevice,
};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::json;

use crate::app::AppHandle;
use crate::ctx::{Ctx, CtxError};
use crate::dialogs::{Choose, ChooseOption, DIALOGS};
use crate::strings as txt;

/// The setting: the pointer profile, by id.
pub const SETTING_PROFILE: SettingId = SettingId::from_static("input.pointer_profile");
/// The setting: the wheel over sliders (`profile`, `guarded`, `value`).
pub const SETTING_SLIDER_WHEEL: SettingId = SettingId::from_static("input.slider_wheel");
/// The setting: a double click on the image (`profile`, `fit`, `leave`,
/// `nothing`).
pub const SETTING_IMAGE_DOUBLE_CLICK: SettingId =
    SettingId::from_static("input.image_double_click");
/// The setting: what sends wheel events (`auto`, `mouse`, `trackpad`).
pub const SETTING_WHEEL_DEVICE: SettingId = SettingId::from_static("input.wheel_device");

/// Keeps or drops the user's pointer gesture changes (`{ "value":
/// "keep" | "drop" }`): what the dialog shown on a change of profile
/// runs.
pub const CMD_OVERRIDES: CommandId = CommandId::from_static("pointer.overrides");

/// The category gesture commands are listed under.
pub const CATEGORY: &str = "Pointer";

/// An option's choice that follows the profile.
pub const FROM_PROFILE: &str = "profile";

fn path(id: &SettingId) -> String {
    format!("settings/{id}")
}

fn choice(app: &AppHandle, id: &SettingId) -> Option<String> {
    lock(&app.store())
        .get(&path(id))
        .and_then(|e| e.value.as_choice().map(str::to_string))
}

/// The profile the setting asks for (its id; it may name no shipped
/// profile; an alias is given as the profile it names).
pub fn profile_id(app: &AppHandle) -> String {
    choice(app, &SETTING_PROFILE)
        .map(|id| pointer::canonical_id(&id).to_string())
        .unwrap_or_else(|| pointer::DEFAULT_PROFILE.into())
}

/// The profile in force: the setting's, else the default.
pub fn profile(app: &AppHandle) -> &'static Profile {
    pointer::profile(&profile_id(app)).unwrap_or_else(pointer::default_profile)
}

/// Asks for a profile (the setting), applied before the next frame. An
/// alias (`adobe`) asks for the profile it names (`lightroom`).
pub fn set_profile(app: &AppHandle, id: &str) -> Result<(), String> {
    if pointer::profile(id).is_none() {
        return Err(format!("`{id}` is not a pointer profile"));
    }
    let id = pointer::canonical_id(id);
    lock(&app.store())
        .set(
            Set::new(
                &path(&SETTING_PROFILE),
                Change::To(Value::Choice(id.into())),
                Origin::User,
            )
            .by(&egui_pop_model::ids::OwnerId::FRAMEWORK),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The options in force: the settings', or the profile's where they
/// say `profile`.
pub fn options(app: &AppHandle, profile: &Profile) -> (pointer::Options, WheelDevice) {
    let mut o = profile.options;
    if let Some(w) = choice(app, &SETTING_SLIDER_WHEEL).and_then(|c| SliderWheel::parse(&c)) {
        o.slider_wheel = w;
    }
    if let Some(d) =
        choice(app, &SETTING_IMAGE_DOUBLE_CLICK).and_then(|c| ImageDoubleClick::parse(&c))
    {
        o.image_double_click = d;
    }
    let device = choice(app, &SETTING_WHEEL_DEVICE)
        .and_then(|c| WheelDevice::parse(&c))
        .unwrap_or_default();
    (o, device)
}

/// The gesture commands whose bindings the user changed.
pub fn overrides(app: &AppHandle) -> Vec<CommandId> {
    GESTURES
        .iter()
        .filter(|g| app.rebound_in(Layer::User, &g.id))
        .map(|g| g.id.clone())
        .collect()
}

/// Puts a profile's bindings in the keymap's default layer, in place of
/// the previous profile's. `None` when no shipped profile has that id
/// (the keymap is left as it was).
pub(crate) fn load_profile(keymap: &mut Keymap, id: &str) -> Option<&'static Profile> {
    let p = pointer::profile(id)?;
    let d = keymap.load(&PROFILE_OWNER, Layer::Default, &pointer::profile_doc(p));
    for e in d.all() {
        log::warn!(target: "egui_pop", "pointer profile `{id}`: {}", e.message);
    }
    Some(p)
}

/// The map in force: the keymap's pointer bindings, the profile's
/// modifiers, the options.
pub(crate) fn map(app: &AppHandle, keymap: &Keymap) -> PointerMap {
    let p = profile(app);
    let mut m = PointerMap::from_keymap(keymap, p);
    let (options, device) = options(app, p);
    m.options = options;
    m.wheel_device = device;
    m
}

/// What the application last loaded and published.
#[derive(Debug, Default)]
pub(crate) struct Sync {
    /// The profile whose bindings are in the default layer, once the
    /// settings were read.
    pub loaded: Option<String>,
    /// The map last published, and what it was made from (the rebuild
    /// count, the settings).
    pub published: Option<(Arc<PointerMap>, u64, String)>,
}

/// The settings a published map depends on, as text.
pub(crate) fn settings_print(app: &AppHandle) -> String {
    [
        &SETTING_PROFILE,
        &SETTING_SLIDER_WHEEL,
        &SETTING_IMAGE_DOUBLE_CLICK,
        &SETTING_WHEEL_DEVICE,
    ]
    .iter()
    .map(|id| choice(app, id).unwrap_or_default())
    .collect::<Vec<_>>()
    .join("|")
}

/// After a change of profile, when the user changed gestures: asks
/// whether to keep them on the new profile or drop them. They stay
/// until answered.
pub(crate) fn ask_about_overrides(app: &AppHandle, profile: &Profile) {
    let changed = overrides(app);
    if changed.is_empty() {
        return;
    }
    let Some(dialogs) = app.service(&DIALOGS) else {
        return;
    };
    let ctx = app.egui();
    let count = changed.len().to_string();
    let title = strings::tr_args(
        &ctx,
        &txt::POINTER_OVERRIDES_TITLE,
        &[("profile", &profile.title)],
    );
    let detail = strings::tr_args(&ctx, &txt::POINTER_OVERRIDES_DETAIL, &[("count", &count)]);
    dialogs.open(
        Choose::with_options(
            &title,
            vec![
                ChooseOption::new(&strings::tr(&ctx, &txt::POINTER_KEEP), json!("keep")),
                ChooseOption::new(&strings::tr(&ctx, &txt::POINTER_DROP), json!("drop")),
            ],
            CMD_OVERRIDES,
        )
        .detail(&detail)
        .args(json!({ "profile": profile.id })),
    );
}

/// The framework's registrations: the settings, the gesture commands,
/// the default profile's bindings.
pub(crate) fn register(ctx: &mut Ctx<'_>, default_profile: &str) -> Result<(), CtxError> {
    let ids = pointer::ids();
    let default_profile = pointer::canonical_id(default_profile);
    let default = if ids.contains(&default_profile) {
        default_profile
    } else {
        log::warn!(
            target: "egui_pop",
            "`{default_profile}` is not a pointer profile: `{}` is used",
            pointer::DEFAULT_PROFILE
        );
        pointer::DEFAULT_PROFILE
    };
    ctx.setting(
        SETTING_PROFILE,
        Descriptor::choice("Pointer profile", &ids, default).description(
            "What the wheel, a pinch and the mouse buttons do on the image, the node graph and sliders, as in darktable, Photoshop, Lightroom Classic, document tools (GIMP, Inkscape, the web), Blender and node editors, or on a trackpad. The shortcuts editor shows and changes each gesture.",
        ),
    )?;
    let mut wheel = vec![FROM_PROFILE];
    wheel.extend(SliderWheel::CHOICES);
    ctx.setting(
        SETTING_SLIDER_WHEEL,
        Descriptor::choice("Wheel over sliders", &wheel, FROM_PROFILE).description(
            "Whether the wheel over a slider scrolls the panel (guarded: the slider steps only with the focus or the step keys) or changes the value (value); profile: as the pointer profile says.",
        ),
    )?;
    let mut double = vec![FROM_PROFILE];
    double.extend(ImageDoubleClick::CHOICES);
    ctx.setting(
        SETTING_IMAGE_DOUBLE_CLICK,
        Descriptor::choice("Double click on the image", &double, FROM_PROFILE).description(
            "What a double click on the image does: fit it, leave the view (back to the browser, as darktable does), or nothing; profile: as the pointer profile says.",
        ),
    )?;
    ctx.setting(
        SETTING_WHEEL_DEVICE,
        Descriptor::choice("Wheel device", &WheelDevice::CHOICES, "auto").description(
            "What sends wheel events: auto tells a mouse wheel (lines) from a trackpad (points); some mice send points too, and choosing mouse or trackpad settles it.",
        ),
    )?;
    for g in GESTURES {
        ctx.declare(
            CommandSpec::new(g.id.clone(), g.title, CATEGORY)
                .description(g.description)
                .hidden()
                .no_agent(),
        )?;
    }
    ctx.command(
        CommandSpec::new(CMD_OVERRIDES, "Keep or drop pointer gesture changes", CATEGORY)
            .description(
                "Keeps the pointer gestures the user changed on the current profile, or drops them (`value`: keep or drop).",
            )
            .hidden()
            .args(json!({ "value": "keep" })),
        |call| {
            let value = call
                .args
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("keep");
            let changed = overrides(call.app);
            match value {
                "keep" => Ok(json!({ "kept": changed.len() })),
                "drop" => {
                    for id in &changed {
                        call.app.reset_keys_in(Layer::User, id.clone());
                    }
                    Ok(json!({ "dropped": changed.len() }))
                }
                other => Err(format!("`{other}` is neither keep nor drop")),
            }
        },
    )?;
    load_profile(&mut ctx.reg.keymap, default);
    Ok(())
}
