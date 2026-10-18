//! The mount contract across the crates enabled (ADR 0013): every
//! component and host declared, in `docs/hosts.md`'s order, the page
//! written from them, and [`MountsPlugin`], which declares the
//! components and hosts of the component crates to the application so
//! that `AppHandle::where_can` and the `mounts` collector know them.
//!
//! ```
//! let page = egui_pop::mounts::page();
//! assert!(page.starts_with("# Hosts and components"));
//! assert!(egui_pop::mounts::components().iter().any(|m| m.id == "jobs-centre"));
//! ```

use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;
use egui_pop_model::mount::{HostOffer, Mountable, page};

/// Every component of the enabled crates: the viewport's, the views',
/// the editors', then `egui-pop-api`'s and `egui-pop-core`'s.
pub fn components() -> Vec<Mountable> {
    let mut out = crates();
    out.extend(egui_pop_api::mounts::components());
    out
}

/// The components of the component crates (not `egui-pop-api`'s, which
/// the application knows already).
fn crates() -> Vec<Mountable> {
    #[allow(unused_mut)]
    let mut out = Vec::new();
    #[cfg(feature = "viewport")]
    out.extend(egui_pop_viewport::mounts::components());
    #[cfg(feature = "views")]
    out.extend(egui_pop_views::mounts::components());
    #[cfg(feature = "editors")]
    out.extend(egui_pop_editors::mounts::components());
    out
}

/// Every host, in `docs/hosts.md`'s column order: `egui-pop-api`'s, the
/// viewport's overlays before the gallery's story.
pub fn hosts() -> Vec<HostOffer> {
    #[allow(unused_mut)]
    let mut out = egui_pop_api::mounts::hosts();
    #[cfg(feature = "viewport")]
    {
        let at = out
            .iter()
            .position(|h| h.id == egui_pop_api::mounts::STORY)
            .unwrap_or(out.len());
        out.insert(at, egui_pop_viewport::mounts::overlay_host());
    }
    out
}

/// `docs/hosts.md`, as the declarations write it.
pub fn page() -> String {
    page::render(&components(), &hosts())
}

/// Declares the components and hosts of the component crates enabled
/// (the viewport, the views, the editors) to the application.
#[derive(Debug, Default)]
pub struct MountsPlugin;

impl Plugin for MountsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.mounts")
    }

    fn title(&self) -> String {
        "Mount declarations".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for m in crates() {
            ctx.mountable(m);
        }
        #[cfg(feature = "viewport")]
        ctx.host_offer(egui_pop_viewport::mounts::overlay_host());
        Ok(())
    }
}
