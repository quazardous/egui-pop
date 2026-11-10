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

/// Text placed in a string stays as written, even when it looks like a
/// number (a folder named "2026" is not "2,026"); a count passed as a
/// number is written as the locale writes numbers.
#[test]
fn names_stay_verbatim_and_counts_follow_the_locale() {
    use egui_pop_api::strings::{FILES_CRUMB_OPEN, JOBS_ACTIVE};
    use egui_pop_core::strings::{tr_args, tr_with};

    let i18n = I18n::with_egui_pop("en");
    let ctx = egui::Context::default();
    install(&ctx, &i18n);
    for (locale, open, count) in [
        ("en", "Open folder ", "Jobs: 1,234"),
        ("fr", "Ouvrir le dossier ", "Tâches : 1\u{202f}234"),
    ] {
        i18n.set_locale(locale);
        for name in ["2026", "007", "1e3", "3.14"] {
            assert_eq!(
                tr_args(&ctx, &FILES_CRUMB_OPEN, &[("name", name)]),
                format!("{open}{name}"),
                "{locale}"
            );
            assert_eq!(
                tr_with(&ctx, &FILES_CRUMB_OPEN, &[("name", name.into())]),
                format!("{open}{name}"),
                "{locale}"
            );
        }
        let jobs = tr_with(&ctx, &JOBS_ACTIVE, &[("count", 1234.into())]);
        assert_eq!(jobs, count, "{locale}");
    }
}
