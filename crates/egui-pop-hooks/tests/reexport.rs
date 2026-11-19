//! The kernel moved to `bus-kernel`: this crate re-exports it, so the
//! old paths name the very same items.

use std::any::TypeId;
use std::sync::Arc;

use egui_pop_hooks::bus_contract::Bus;
use egui_pop_hooks::{Kernel, ListenOpts, Point, PointId, PointSpec};

struct Trail;
impl Point for Trail {
    const ID: PointId = PointId::new("test.trail");
    type Input = ();
    type Value = Vec<String>;
}

#[test]
fn the_old_paths_are_bus_kernel_items() {
    assert_eq!(TypeId::of::<Kernel>(), TypeId::of::<bus_kernel::Kernel>());
    assert_eq!(
        TypeId::of::<ListenOpts>(),
        TypeId::of::<bus_kernel::bus_contract::ListenOpts>()
    );
    let bus: Arc<dyn Bus> = Arc::new(egui_pop_hooks::bus_kernel::Kernel::new());
    bus.declare::<Trail>(PointSpec::new("A trail.")).unwrap();
    bus.listen::<Trail>("test.a", ListenOpts::new(), |_, v| {
        v.push("a".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(bus.alter::<Trail>((), Vec::new()).value, ["a"]);
}
