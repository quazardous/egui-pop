//! The mount contract in the application (ADR 0013): the hosts this
//! crate builds and what each offers, the components of this crate and
//! of `egui-pop-core` and what each needs, the components and hosts an
//! application declares ([`Ctx::mountable`], [`Ctx::host_offer`]), and
//! the questions agents and code ask:
//!
//! - [`AppHandle::where_can`]: every host and the verdict for a
//!   component;
//! - [`AppHandle::mount`]: a component put in a host, recorded with its
//!   conditions, or refused with its rule (reported, ADR 0003);
//! - the debug bridge's `mounts` collector: the declarations, what is
//!   mounted where, and the `where_can` and `check` actions.
//!
//! ```
//! use egui_pop_api::{App, mounts};
//!
//! let app = App::builder().build(&egui::Context::default()).unwrap();
//! let verdicts = app.handle().where_can(mounts::JOBS_CENTRE).unwrap();
//! let status_bar = verdicts.iter().find(|v| v.host == mounts::STATUS_BAR).unwrap();
//! assert_eq!(status_bar.fit.cell(), "D1");
//! ```

use egui_pop_model::diagnostics::{Diagnostic, Level};
use egui_pop_model::ids::OwnerId;
use egui_pop_model::mount::{
    self, ClaimKey, Dismiss, Fit, FocusPolicy, HostDisplay, HostOffer, HostSize, Keep, Keyboard,
    Lifetime, Mountable, Window,
};
use egui_pop_model::panels::{PanelHost, Place};
use serde::Serialize;
use serde_json::{Value as Json, json};
use std::cell::RefCell;

use crate::app::{AppHandle, WeakApp};
use crate::ctx::Ctx;

/// A docked panel: a tab in an area.
pub const PANEL: &str = "panel";
/// A floating palette.
pub const PALETTE: &str = "palette";
/// A panel detached into an auxiliary window.
pub const WINDOW: &str = "window";
/// A dialog of the four kinds (`Confirm`, `Form`, `Choose`, `Info`).
pub const DIALOG: &str = "dialog";
/// A custom dialog: a body of its own, the dialog rules kept
/// ([`crate::dialogs::Custom`]).
pub const CUSTOM_DIALOG: &str = "dialog.custom";
/// A raw modal overlay, drawn from a block of the `OVERLAYS` zone.
pub const MODAL: &str = "modal";
/// A popover anchored to something.
pub const POPOVER: &str = "popover";
/// A block of a line zone (the top bar, the caption, the activity bar).
pub const LINE: &str = "line";
/// The status bar.
pub const STATUS_BAR: &str = "status-bar";
/// A custom page of the settings window
/// ([`crate::settings_pages::SettingsPage`]).
pub const SETTINGS_PAGE: &str = "settings.page";
/// A gallery story.
pub const STORY: &str = "story";

/// The value-descriptor sliders (`egui_pop_core::slider`).
pub const SLIDERS: &str = "sliders";
/// Forms of value-descriptor fields.
pub const FORMS: &str = "forms";
/// The progress bar and spinner (`egui_pop_core::progress`).
pub const PROGRESS: &str = "progress";
/// The jobs centre ([`crate::jobs_centre`]).
pub const JOBS_CENTRE: &str = "jobs-centre";
/// The theme editor ([`crate::theme_editor`]).
pub const THEME_EDITOR: &str = "theme-editor";
/// The tip of the day and hints ([`crate::tips`]).
pub const TIPS: &str = "tips";

/// A docked panel: the area's whole rect, resized by the user.
pub fn panel() -> HostOffer {
    HostOffer::new(PANEL, "P", "Docked panel")
        .source("Ctx::panel, PanelSpec, workbench")
        .resizable(true)
        .keeps(&[Keep::Place])
        .display(HostDisplay::Wide)
        .hides_content(true)
}

/// A floating palette: a bounded width, the content's height.
pub fn palette() -> HostOffer {
    HostOffer::new(PALETTE, "F", "Floating palette")
        .source("palettes")
        .size(HostSize::Content)
        .focus(FocusPolicy::ClickInto)
        .keeps(&[Keep::Place])
        .hides_content(true)
}

/// An auxiliary window: a detachable panel in a window of its own.
pub fn window() -> HostOffer {
    HostOffer::new(WINDOW, "W", "Auxiliary window")
        .source("workbench::windows, show_viewport_immediate")
        .resizable(true)
        .window(Window::Auxiliary)
        .keeps(&[Keep::Place, Keep::Size])
        .cursor_images(false)
        .headless(false)
}

/// A dialog of the four kinds: descriptors, no free content.
pub fn dialog() -> HostOffer {
    HostOffer::new(DIALOG, "D", "Dialog (four kinds)")
        .source("dialogs: Confirm, Form, Choose, Info")
        .size(HostSize::Content)
        .focus(FocusPolicy::ModalExclusive)
        .owns_keys(&[ClaimKey::Enter, ClaimKey::Escape])
        .dismissed_by(&[Dismiss::Answer, Dismiss::Escape])
        .lifetime(Lifetime::Transient)
        .keeps(&[Keep::Place])
        .gives_app(false)
        .scope(false)
        .fault_boundary(false)
        .free_content(false)
}

/// A raw modal overlay: any `Ui`, none of the dialog rules.
pub fn modal() -> HostOffer {
    HostOffer::new(MODAL, "M", "Raw modal")
        .source("overlay::show with Kind::Modal")
        .size(HostSize::Content)
        .focus(FocusPolicy::ModalExclusive)
        .owns_keys(&[ClaimKey::Escape])
        .dismissed_by(&[Dismiss::Escape])
        .lifetime(Lifetime::Transient)
        .scope(false)
}

/// A popover: next to its anchor, closed by Escape or a click outside.
pub fn popover() -> HostOffer {
    HostOffer::new(POPOVER, "O", "Popover")
        .source("overlay::show with Kind::Popover, egui::Popup")
        .size(HostSize::Content)
        .owns_keys(&[ClaimKey::Escape])
        .dismissed_by(&[Dismiss::Escape, Dismiss::OutsideClick])
        .lifetime(Lifetime::Transient)
        .scope(false)
        .fault_boundary(false)
}

/// A block of a line zone.
pub fn line() -> HostOffer {
    HostOffer::new(LINE, "L", "Line zone")
        .source("zones: TOP_BAR, TITLE_BAR, ACTIVITY_BAR")
        .size(HostSize::Row)
}

/// The status bar.
pub fn status_bar() -> HostOffer {
    HostOffer::new(STATUS_BAR, "S", "Status bar")
        .source("zones: STATUS_BAR, strips")
        .size(HostSize::Row)
}

/// A gallery story: drawn by the gallery in its main zone.
pub fn story() -> HostOffer {
    HostOffer::new(STORY, "G", "Gallery story")
        .source("Ctx::story, Story")
        .resizable(true)
        .display(HostDisplay::Wide)
}

/// The hosts of this crate, in the order of `docs/hosts.md`'s columns
/// (the viewport's overlays are `egui-pop-viewport`'s).
pub fn hosts() -> Vec<HostOffer> {
    vec![
        panel(),
        palette(),
        window(),
        dialog(),
        crate::dialogs::custom_host(),
        modal(),
        popover(),
        line(),
        status_bar(),
        crate::settings_pages::host(),
        story(),
    ]
}

/// A panel's host for a place: the panel's offer in an area, the
/// palette's floating, the window's detached.
pub fn panel_host(place: Place) -> PanelHost {
    let offer = match place {
        Place::Area(_) => panel(),
        Place::Floating => palette(),
        Place::Window => window(),
    };
    PanelHost { place, offer }
}

/// Sliders: a value dragged, typed or stepped.
pub fn sliders() -> Mountable {
    Mountable::new(SLIDERS, "Sliders")
        .crate_name("egui-pop-core")
        .claims(&[ClaimKey::Enter, ClaimKey::Escape])
        .drag()
        .min(120.0, 20.0)
        .role("slider")
        .descriptor_field()
}

/// Forms: one row per value-descriptor field.
pub fn forms() -> Mountable {
    Mountable::new(FORMS, "Forms")
        .crate_name("egui-pop-core")
        .claims(&[ClaimKey::Enter])
        .min(200.0, 48.0)
        .popups()
        .role("form")
        .descriptor_field()
}

/// The progress bar and spinner: not interactive.
pub fn progress() -> Mountable {
    Mountable::new(PROGRESS, "Progress")
        .crate_name("egui-pop-core")
        .keyboard(Keyboard::None)
        .min(40.0, 4.0)
        .preferred(200.0, 8.0)
        .role("progressbar")
        .preview()
}

/// The jobs centre: one per application, from the host's journal.
pub fn jobs_centre() -> Mountable {
    Mountable::new(JOBS_CENTRE, "Jobs centre")
        .crate_name("egui-pop-api")
        .keymap()
        .min(240.0, 120.0)
        .fills()
        .single()
        .needs_app()
        .role("list")
}

/// The theme editor: one draft per application.
pub fn theme_editor() -> Mountable {
    Mountable::new(THEME_EDITOR, "Theme editor")
        .crate_name("egui-pop-api")
        .keymap()
        .min(520.0, 320.0)
        .fills()
        .single()
        .needs_app()
        .session()
        .popups()
        .role("form")
}

/// The tip of the day and hints.
pub fn tips() -> Mountable {
    Mountable::new(TIPS, "Tips")
        .crate_name("egui-pop-api")
        .min(240.0, 80.0)
        .needs_app()
        .role("group")
}

/// The components of this crate and of `egui-pop-core`.
pub fn components() -> Vec<Mountable> {
    vec![
        sliders(),
        forms(),
        progress(),
        jobs_centre(),
        theme_editor(),
        tips(),
    ]
}

/// A host's verdict for a component.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placement {
    /// The host's id.
    pub host: String,
    /// Its column (`P`, `F`...).
    pub code: String,
    pub name: String,
    pub fit: Fit,
}

/// A component mounted in a host.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mounted {
    pub component: String,
    pub host: String,
    pub fit: Fit,
}

/// What the application declared, and what is mounted.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    components: RefCell<Vec<(OwnerId, Mountable)>>,
    hosts: RefCell<Vec<(OwnerId, HostOffer)>>,
    mounted: RefCell<Vec<Mounted>>,
}

impl AppHandle {
    fn mount_registry(&self) -> std::rc::Rc<Registry> {
        self.local::<Registry>()
    }

    /// Whether what `owner` declared still counts (the framework, or a
    /// connected plugin).
    pub(crate) fn owner_connected(&self, owner: &OwnerId) -> bool {
        *owner == OwnerId::FRAMEWORK
            || self
                .plugins()
                .iter()
                .any(|p| p.connected && OwnerId::from(&p.id) == *owner)
    }

    /// Every component declared: this crate's and `egui-pop-core`'s,
    /// then those of the connected plugins.
    pub fn mountables(&self) -> Vec<Mountable> {
        let mut out = components();
        let declared = self.mount_registry().components.borrow().clone();
        out.extend(
            declared
                .into_iter()
                .filter(|(o, _)| self.owner_connected(o))
                .map(|(_, m)| m),
        );
        out
    }

    /// Every host: this crate's, then those of the connected plugins
    /// (the viewport's overlays among them).
    pub fn host_offers(&self) -> Vec<HostOffer> {
        let mut out = hosts();
        let declared = self.mount_registry().hosts.borrow().clone();
        out.extend(
            declared
                .into_iter()
                .filter(|(o, _)| self.owner_connected(o))
                .map(|(_, h)| h),
        );
        out
    }

    /// Every host and its verdict for a component (ADR 0013): where it
    /// fits, under which conditions, and why not elsewhere.
    pub fn where_can(&self, component: &str) -> Result<Vec<Placement>, String> {
        let needs = self
            .mountables()
            .into_iter()
            .find(|m| m.id == component)
            .ok_or_else(|| format!("no component `{component}` is declared"))?;
        Ok(self
            .host_offers()
            .into_iter()
            .map(|h| Placement {
                fit: mount::check(&needs, &h),
                host: h.id,
                code: h.code,
                name: h.name,
            })
            .collect())
    }

    /// The verdict for a component in a host, without mounting it.
    pub fn check_mount(&self, component: &str, host: &str) -> Result<Fit, String> {
        self.where_can(component)?
            .into_iter()
            .find(|p| p.host == host)
            .map(|p| p.fit)
            .ok_or_else(|| format!("no host `{host}` is declared"))
    }

    /// Mounts a component in a host: recorded with its conditions (the
    /// `mounts` collector shows it), or refused with its rule, reported
    /// as a diagnostic (`mount-refused`) and returned as the error.
    pub fn mount(&self, component: &str, host: &str) -> Result<Fit, String> {
        let fit = match self.check_mount(component, host) {
            Ok(fit) => fit,
            Err(e) => {
                self.report(Diagnostic {
                    level: Level::Error,
                    code: "mount-unknown",
                    owner: None,
                    subject: format!("{component} in {host}"),
                    message: format!("{e}; nothing is mounted"),
                });
                return Err(e);
            }
        };
        self.record_mount(component, host, fit)
    }

    /// Mounts a component by its needs in a host by its offer, declared
    /// or not (a custom dialog's body): recorded, or refused, reported.
    pub(crate) fn mount_needs(&self, needs: &Mountable, host: &HostOffer) -> Result<Fit, String> {
        self.record_mount(&needs.id, &host.id, mount::check(needs, host))
    }

    fn record_mount(&self, component: &str, host: &str, fit: Fit) -> Result<Fit, String> {
        if let Some(refusal) = fit.refusal() {
            let reason = refusal.reason();
            self.report(Diagnostic {
                level: Level::Error,
                code: "mount-refused",
                owner: None,
                subject: format!("{component} in {host}"),
                message: format!("{reason}; nothing is mounted"),
            });
            return Err(reason);
        }
        let registry = self.mount_registry();
        let mut mounted = registry.mounted.borrow_mut();
        if !mounted
            .iter()
            .any(|m| m.component == component && m.host == host)
        {
            mounted.push(Mounted {
                component: component.into(),
                host: host.into(),
                fit: fit.clone(),
            });
        }
        Ok(fit)
    }

    /// Takes a component out of a host; `false` when it was not mounted
    /// there.
    pub fn unmount(&self, component: &str, host: &str) -> bool {
        let registry = self.mount_registry();
        let mut mounted = registry.mounted.borrow_mut();
        let before = mounted.len();
        mounted.retain(|m| !(m.component == component && m.host == host));
        mounted.len() != before
    }

    /// What is mounted where, in mounting order.
    pub fn mounts(&self) -> Vec<Mounted> {
        self.mount_registry().mounted.borrow().clone()
    }
}

impl Ctx<'_> {
    /// Declares a component of the plugin's (ADR 0013): what it needs.
    /// A component of the same id declared by another owner wins: this
    /// one is ignored, with a diagnostic; declared again by the same
    /// plugin (connected again), it replaces its own.
    pub fn mountable(&mut self, needs: Mountable) -> &mut Self {
        let registry = self.app.local::<Registry>();
        let builtin = components().iter().any(|m| m.id == needs.id);
        let ok = declare(
            &mut registry.components.borrow_mut(),
            &self.owner,
            builtin,
            needs.id.clone(),
            needs,
            |m| m.id.as_str(),
        );
        if let Err(id) = ok {
            self.app.report(duplicate(&self.owner, id, "component"));
        }
        self
    }

    /// Declares a host of the plugin's (ADR 0013): what it offers, with
    /// the same rule for a second declaration of an id.
    pub fn host_offer(&mut self, offer: HostOffer) -> &mut Self {
        let registry = self.app.local::<Registry>();
        let builtin = hosts().iter().any(|h| h.id == offer.id);
        let ok = declare(
            &mut registry.hosts.borrow_mut(),
            &self.owner,
            builtin,
            offer.id.clone(),
            offer,
            |h| h.id.as_str(),
        );
        if let Err(id) = ok {
            self.app.report(duplicate(&self.owner, id, "host"));
        }
        self
    }
}

/// Adds a declaration, or replaces the owner's own of the same id;
/// `Err(id)` when the id is another owner's (or built in).
fn declare<T>(
    list: &mut Vec<(OwnerId, T)>,
    owner: &OwnerId,
    builtin: bool,
    id: String,
    item: T,
    id_of: impl Fn(&T) -> &str,
) -> Result<(), String> {
    if builtin {
        return Err(id);
    }
    match list.iter().position(|(_, t)| id_of(t) == id) {
        Some(i) if list[i].0 == *owner => {
            list[i].1 = item;
            Ok(())
        }
        Some(_) => Err(id),
        None => {
            list.push((owner.clone(), item));
            Ok(())
        }
    }
}

fn duplicate(owner: &OwnerId, id: String, what: &str) -> Diagnostic {
    Diagnostic {
        level: Level::Warning,
        code: "duplicate-id",
        owner: Some(owner.clone()),
        subject: id,
        message: format!("a {what} of this id is declared already; this declaration is ignored"),
    }
}

/// The `mounts` collector.
pub(crate) fn collector(app: WeakApp) -> egui_pop_core::debug::FnCollector {
    let (a, b, c) = (app.clone(), app.clone(), app);
    egui_pop_core::debug::FnCollector::new(
        "mounts",
        "The mount contract (ADR 0013): every component and host declared, what is mounted where with its conditions; `where_can` and `check` give the verdicts.",
    )
    .snapshot(move |_| {
        let Ok(app) = a.get() else { return Json::Null };
        json!({
            "components": app.mountables(),
            "hosts": app.host_offers(),
            "mounted": app.mounts(),
        })
    })
    .action(
        "where_can",
        "Every host and its verdict for a component (`component`).",
        false,
        move |_, args| {
            let app = b.get()?;
            let id = args
                .get("component")
                .and_then(Json::as_str)
                .ok_or("`component` is missing")?;
            Ok(json!(app.where_can(id)?))
        },
    )
    .args(json!({ "component": "viewport" }))
    .action(
        "check",
        "The verdict for a component in a host (`component`, `host`), with the reason when it does not fit.",
        false,
        move |_, args| {
            let app = c.get()?;
            let get = |k: &str| {
                args.get(k)
                    .and_then(Json::as_str)
                    .ok_or_else(|| format!("`{k}` is missing"))
            };
            let fit = app.check_mount(get("component")?, get("host")?)?;
            let reason = fit.refusal().map(mount::Refusal::reason);
            Ok(json!({ "fit": fit, "cell": fit.cell(), "reason": reason }))
        },
    )
    .args(json!({ "component": "viewport", "host": "dialog.custom" }))
}
