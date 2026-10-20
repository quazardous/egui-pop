//! The gallery application, native or in a browser.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use egui_pop_gallery::{Gallery, Options, TITLE, USAGE};

    let mut options = match Options::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    options.announce = true;
    if options.platform {
        // One gallery at a time: a second launch hands its arguments
        // (a gallery://page/<name> link) to the first.
        match egui_pop::platform::instance::single(egui_pop_gallery::pages::platform::APP_ID) {
            Ok(egui_pop::platform::instance::Single::Handed) => return Ok(()),
            Ok(egui_pop::platform::instance::Single::First(first)) => {
                egui_pop_gallery::set_instance(first);
            }
            Err(e) => eprintln!("single instance: {e}"),
        }
    }
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_app_id(egui_pop_gallery::pages::platform::APP_ID)
            .with_inner_size(options.size)
            // The image surface shows through a hole in the interface.
            .with_transparent(options.wide_gamut),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        native,
        Box::new(move |cc| {
            let gallery = Gallery::from_creation_context(cc, options)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            Ok(Box::new(gallery))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    use egui_pop_gallery::{Gallery, Options};

    // The `log` facade to the browser's console (the platform page's
    // logs then keep only their tail).
    let _ = eframe::WebLogger::init(log::LevelFilter::Info);
    // A panic (std on the web panics where the platform has nothing)
    // aborts the module: say so in the page, not with a black canvas.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        failed(&format!("it panicked: {info}"));
        previous(info);
    }));

    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = eframe::web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(canvas) = document
            .get_element_by_id("the_canvas_id")
            .and_then(|e| e.dyn_into::<eframe::web_sys::HtmlCanvasElement>().ok())
        else {
            failed("the page has no canvas `the_canvas_id`");
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    let gallery = Gallery::from_creation_context(cc, Options::default())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                    Ok(Box::new(gallery))
                }),
            )
            .await;
        if let Err(e) = started {
            failed(&format!("it did not start: {e:?}"));
        }
    });
}

/// Logs why the gallery stopped to the console, and shows it in the page
/// in place of the canvas.
#[cfg(target_arch = "wasm32")]
fn failed(why: &str) {
    log::error!("egui-pop gallery: {why}");
    let Some(document) = eframe::web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if let Some(canvas) = document.get_element_by_id("the_canvas_id") {
        let _ = canvas.set_attribute("hidden", "");
    }
    if let Some(message) = document.get_element_by_id("failed") {
        message.set_text_content(Some(&format!(
            "The gallery could not run in this browser: {why}. The browser's console says more."
        )));
        let _ = message.remove_attribute("hidden");
    }
}
