//! Reads kit folders and prints what egui-pop makes of them: name,
//! theme, faces, licence and every problem (`kit::check`). Exits with 1
//! when a kit has problems.
//!
//! ```sh
//! cargo run -p egui-pop-tokens --example kit-check -- path/to/kit [more/kits...]
//! ```

use egui_pop_tokens::kit::{self, KitData};

fn main() {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: kit-check <kit folder>...");
        std::process::exit(2);
    }
    let mut failed = false;
    for dir in dirs {
        let kit = KitData::load(&dir);
        println!("{dir}: kit `{}` ({})", kit.name, kit.display_name());
        if let Some(d) = &kit.theme.description {
            println!("  {d}");
        }
        println!(
            "  extends `{}`, {} tokens",
            kit.theme.extends.as_deref().unwrap_or("-"),
            kit.theme.tokens.len()
        );
        for f in &kit.fonts {
            println!("  font {} {} ({})", f.family, f.weight, f.file);
        }
        println!(
            "  licence: {}",
            if kit.license.is_some() { "yes" } else { "none" }
        );
        let problems = kit::check(&kit);
        for p in &problems {
            println!("  problem: {p}");
        }
        if problems.is_empty() {
            println!("  ok");
        }
        failed |= !problems.is_empty();
    }
    std::process::exit(i32::from(failed));
}
