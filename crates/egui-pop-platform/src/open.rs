//! The links service's system opener (ADR 0017): local files and folders
//! opened with the system's default application, and Reveal, the file
//! manager with the file selected. Web pages and mail still go through
//! egui's `OpenUrl` (the browser, `$BROWSER` honoured; a new tab on the
//! web build).
//!
//! | | Open a file or folder | Reveal |
//! |---|---|---|
//! | Linux | `xdg-open` (inside Flatpak, the runtime's forwards to the OpenURI portal, which opens what the sandbox can read) | the freedesktop file manager interface (`org.freedesktop.FileManager1.ShowItems`), else the OpenURI portal's `OpenDirectory` |
//! | macOS | `open` | `open -R` |
//! | Windows | `ShellExecuteW` (programs never reach it: the policy refuses them first) | Explorer's selection (`SHOpenFolderAndSelectItems`) |
//! | The web build | refused with the reason | refused |
//!
//! The work is done on a thread (a D-Bus call may take a moment): the
//! `link.open` step ends `opening`, then the journal says `link.opened`,
//! or `link.failed` with a warning toast.
//!
//! ```no_run
//! use egui_pop_api::App;
//! use egui_pop_platform::open::OpenPlugin;
//!
//! let app = App::builder()
//!     .plugins(egui_pop_api::DefaultPlugins)
//!     .plugin(OpenPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! ```

use std::sync::Arc;

use egui_pop_api::links::{self, Caps, EguiOpener, How, OpenCall, Opener, Opening};
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::PluginId;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.open");

/// Opens local files and folders, and reveals them, with the system;
/// web pages and mail through egui's opener.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn name(&self) -> &str {
        "system"
    }

    fn caps(&self) -> Caps {
        if cfg!(target_arch = "wasm32") {
            Caps::WEB
        } else {
            Caps {
                web: true,
                files: true,
                reveal: true,
                inside: false,
            }
        }
    }

    fn open(&self, call: &OpenCall<'_>) -> Opening {
        let local = call.target.local_path();
        match (call.how, local) {
            (How::Outside, None) => EguiOpener.open(call),
            (how, Some(path)) => system(how, path),
            (_, None) => Opening::Done(Err(
                "only a local file or folder can be shown in its folder".into(),
            )),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn system(how: How, path: String) -> Opening {
    let (pending, resolver) = egui_pop_api::host::pending();
    let spawned = std::thread::Builder::new()
        .name("egui-pop-open".into())
        .spawn(move || {
            let result = match how {
                How::Reveal => opener::reveal(&path),
                _ => opener::open(&path),
            };
            resolver.resolve(
                result
                    .map(|()| serde_json::Value::Null)
                    .map_err(|e| e.to_string()),
            );
        });
    match spawned {
        Ok(_) => Opening::Later(pending),
        Err(e) => Opening::Done(Err(format!("no thread to open it from: {e}"))),
    }
}

#[cfg(target_arch = "wasm32")]
fn system(_: How, _: String) -> Opening {
    Opening::Done(Err("local files cannot be opened from the web".into()))
}

/// Installs [`SystemOpener`] in the links service.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenPlugin;

impl Plugin for OpenPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "System opener".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![links::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let l = links::links(&ctx.app())
            .ok_or_else(|| PluginError::Failed("the links service is not there".into()))?;
        l.set_opener(Arc::new(SystemOpener));
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        if let Some(l) = links::links(&ctx.app()) {
            l.set_opener(Arc::new(EguiOpener));
        }
    }
}
