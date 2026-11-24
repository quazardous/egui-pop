//! The panel zen mode shows, on the bus (ADR 0029): `pop.zen.panel` asks
//! the application which panel matters now, when zen mode starts without
//! a panel named and the focused panel is not a view
//! ([`AppHandle::zen_target`]).
//!
//! egui-pop has no listener of its own: an application answers with the
//! panel its own rules pick (an image viewer, the surface a double click
//! reaches), by its id; `None`, or a panel that is not open, lets the
//! centre's active tab be shown.
//!
//! [`AppHandle::zen_target`]: crate::AppHandle::zen_target

use bus_contract::{Point, PointId, PointKind};

/// Which panel zen mode shows (`pop.zen.panel`): the value is the panel's
/// id, `None` while no listener picks one.
#[derive(Debug)]
pub struct ZenPanel;
impl Point for ZenPanel {
    const KIND: PointKind = PointKind::Alter;
    const ID: PointId = PointId::new("pop.zen.panel");
    type Input = ();
    type Value = Option<String>;
}

/// The zen points, in the order [`super::order_dump`] prints them.
pub const ZEN_POINTS: [&str; 1] = [ZenPanel::ID.as_str()];
