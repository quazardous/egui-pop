//! What the gallery names itself to YouTube: its id, as the `Referer` of the
//! viewer's YouTube player (YouTube's terms ask an application to identify
//! itself so).

use egui_pop::views::viewer::referrer_for;
use egui_pop_gallery::Options;
use egui_pop_gallery::pages::platform::APP_ID;

#[test]
fn the_gallery_sends_its_id_as_youtubes_referer() {
    let app = egui_pop_gallery::builder(&Options::default())
        .build(&egui::Context::default())
        .expect("the gallery builds");
    assert_eq!(app.handle().app_id(), APP_ID);
    assert_eq!(
        referrer_for(&app.handle().app_id()).as_deref(),
        Some("https://org.egui_pop.gallery/")
    );
}
