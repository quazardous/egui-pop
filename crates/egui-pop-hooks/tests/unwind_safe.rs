//! The kernel stays unwind safe with a host journal and relay: dropping
//! these auto traits is a breaking change for its users.

use std::panic::{RefUnwindSafe, UnwindSafe};

use egui_pop_hooks::Kernel;

fn unwind_safe<T: UnwindSafe + RefUnwindSafe>() {}

#[test]
fn the_kernel_is_unwind_safe() {
    unwind_safe::<Kernel>();
}
