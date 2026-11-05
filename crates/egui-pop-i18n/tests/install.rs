//! Catalogues installed in an egui context without `App` or
//! `I18nPlugin`: the context-level lookups, `tr!` and egui-pop's own
//! strings read them.

use egui_pop_i18n::{I18n, args, install, t, text, tr};

const FR: &str = "hello = Bonjour, { $name } !\ntitle = Fichiers\n";
const EN: &str = "hello = Hello, { $name }!\ntitle = Files\n";

#[test]
fn installed_catalogues_answer_without_the_plugin() {
    let i18n = I18n::new("fr");
    i18n.add("fr", FR).unwrap();
    i18n.add("en", EN).unwrap();
    let ctx = egui::Context::default();
    assert!(egui_pop_i18n::plugin::of(&ctx).is_none());

    install(&ctx, &i18n);

    assert!(egui_pop_i18n::plugin::of(&ctx).is_some());
    assert_eq!(t(&ctx, "hello", &args![name = "Ada"]), "Bonjour, Ada !");
    assert_eq!(text(&ctx, "missing", "Missing", &args![]), "Missing");
    assert_eq!(tr!("hello", name = "Ada"), "Bonjour, Ada !");
    // The strings provider: what egui-pop's own widgets look up.
    assert_eq!(
        egui_pop_core::strings::lookup(&ctx, "title", "Files"),
        "Fichiers"
    );

    // The I18n is shared: a locale switch shows without installing again.
    i18n.set_locale("en");
    assert_eq!(t(&ctx, "hello", &args![name = "Ada"]), "Hello, Ada!");
    assert_eq!(tr!("hello", name = "Ada"), "Hello, Ada!");
    assert_eq!(egui_pop_core::strings::lookup(&ctx, "title", ""), "Files");
}
