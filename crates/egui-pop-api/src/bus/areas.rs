//! The interactive areas on the bus (ADR 0029): `pop.ui.areas` lists every
//! area the last pass of each window registered
//! (`egui_pop_core::areas`): its kind (display, navigate, edit), for an
//! edit area the control it writes, its value and path, its panel,
//! whether it is inert and its grips. Agents and tests read which areas
//! edit without a screenshot.
//!
//! The registry's listener ([`AREA_REGISTRY`]) fills the list; a listener
//! after it may add to it (areas egui does not draw).

use std::sync::{Arc, Mutex};

use bus_contract::{Bus, ListenOpts, Point, PointId, PointKind};
use egui_pop_core::areas::{self, AreaRecord};
use egui_pop_core::lock;

use super::OWNER;

/// The interactive areas are listed (`pop.ui.areas`): the value is the list,
/// each window's last pass, the main window's first.
#[derive(Debug)]
pub struct UiAreas;
impl Point for UiAreas {
    const KIND: PointKind = PointKind::Alter;
    const ID: PointId = PointId::new("pop.ui.areas");
    type Input = ();
    type Value = Vec<AreaRecord>;
}

/// The area points, in the order [`super::order_dump`] prints them.
pub const AREA_POINTS: [&str; 1] = [UiAreas::ID.as_str()];

/// The registry's listener of `pop.ui.areas`: lists the areas registered.
pub const AREA_REGISTRY: &str = "pop.ui.area_registry";

/// Registers the registry's listener of `pop.ui.areas`; it reads the context
/// the interface draws with.
pub(super) fn install(kernel: &dyn Bus, ctx: Arc<Mutex<Option<egui::Context>>>) {
    let listened = kernel.listen::<UiAreas>(
        AREA_REGISTRY,
        ListenOpts::new().owner(OWNER),
        move |_, value| {
            if let Some(ctx) = lock(&ctx).clone() {
                value.extend(areas::list(&ctx));
            }
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}
