//! The `inspect` method: egui's [`egui_inspection`] protocol carried over
//! the agent socket, to an [`InspectionPlugin`] in the application's
//! process.
//!
//! `egui_inspection` is by Lucas Meurer and Emil Ernerfeldt
//! ([emilk/egui](https://github.com/emilk/egui)): the accessibility tree,
//! raw input events, a screenshot, a resize and a wait for the frame to
//! settle. eframe serves it on an unauthenticated loopback TCP port; this
//! module serves it on the socket instead, through
//! [`InspectionPlugin::submit`], which takes requests from any transport
//! (the idea of tunnelling it, and the tools that use it, come from Rerun's
//! `egui_mcp` and `egui_inspection_tools`, by the same authors,
//! [rerun-io/egui_crates](https://github.com/rerun-io/egui_crates)). No
//! port is opened, and every request is held like the socket's other
//! methods:
//!
//! - the bridge on ([`BridgeMode::On`](egui_pop_api::core::debug::BridgeMode));
//!   read-only or off refuse it, since a screenshot or the tree shows
//!   whatever the window shows;
//! - the agent's grant allows `ui` and private data;
//! - the input lock: while the user answers an agent's request
//!   (a destructive call's confirmation, a pairing), every request is
//!   refused, checked again just before it is handed to egui.
//!
//! Params: `{ "request": <Request> }`, the protocol's [`Request`] in
//! serde's JSON form (`"GetTree"`, `{ "GetScreenshot": {
//! "pixels_per_point": 1.0 } }`, `{ "ApplyEvents": { "events": [..] } }`,
//! `{ "Resize": { "width", "height" } }`, `{ "Settle": { "max_steps" } }`,
//! `"GetInfo"`). Result: `{ "response": <Response> }`, the same way, but a
//! screenshot is `{ "Screenshot": { "size": [w, h], "png_base64" } }`.
//! [`request_to_json`] and [`response_from_json`] are the client's side.
//!
//! The plugin reads and acts on the main window; `ui` (with `window`)
//! reaches the application's other windows.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use egui_inspection::InspectionPlugin;
/// The protocol's types and version, as `egui_inspection` 0.37 has them
/// (its version follows egui's).
pub use egui_inspection::protocol::{EncodedPng, PROTOCOL_VERSION, Request, Response};
use serde_json::{Value as Json, json};

use crate::Reply;

/// The request in `inspect`'s params.
pub(crate) fn request_from_params(params: &Json) -> Result<Request, String> {
    let request = params
        .get("request")
        .ok_or("`request` is missing: an egui_inspection request, such as \"GetTree\"")?;
    serde_json::from_value(request.clone())
        .map_err(|e| format!("`request` is not an egui_inspection request: {e}"))
}

/// The request's name, for the journal.
pub(crate) fn name(request: &Request) -> &'static str {
    match request {
        Request::GetInfo => "GetInfo",
        Request::GetTree => "GetTree",
        Request::GetScreenshot { .. } => "GetScreenshot",
        Request::ApplyEvents { .. } => "ApplyEvents",
        Request::Resize { .. } => "Resize",
        Request::Settle { .. } => "Settle",
    }
}

/// `request` as `inspect`'s params.
pub fn request_to_json(request: &Request) -> Json {
    json!({ "request": serde_json::to_value(request).unwrap_or(Json::Null) })
}

/// `response` as `inspect`'s result.
pub fn response_to_json(response: &Response) -> Json {
    let response = match response {
        Response::Screenshot(png) => json!({ "Screenshot": {
            "size": png.size,
            "png_base64": BASE64.encode(&png.bytes),
        }}),
        other => serde_json::to_value(other).unwrap_or_else(
            |e| json!({ "Error": { "message": format!("the response does not serialise: {e}") } }),
        ),
    };
    json!({ "response": response })
}

/// `inspect`'s result as the protocol's [`Response`].
///
/// # Errors
///
/// When `result` is not one.
pub fn response_from_json(result: &Json) -> Result<Response, String> {
    let response = result.get("response").ok_or("no `response`")?;
    if let Some(shot) = response.get("Screenshot") {
        let size: [u32; 2] = serde_json::from_value(shot["size"].clone())
            .map_err(|e| format!("a screenshot's `size`: {e}"))?;
        let bytes = BASE64
            .decode(shot["png_base64"].as_str().ok_or("no `png_base64`")?)
            .map_err(|e| format!("a screenshot's `png_base64`: {e}"))?;
        return Ok(Response::Screenshot(EncodedPng { size, bytes }));
    }
    serde_json::from_value(response.clone()).map_err(|e| format!("not a response: {e}"))
}

/// Hands `request` to the application's [`InspectionPlugin`] (added on
/// first use), and asks for the frames that serve it, hidden window
/// included; the reply goes to the connection when the plugin gives it.
pub(crate) fn submit(ctx: &egui::Context, request: Request, reply: Reply) {
    if ctx.with_plugin::<InspectionPlugin, _>(|_| ()).is_none() {
        ctx.add_plugin(InspectionPlugin::new(None));
    }
    ctx.with_plugin::<InspectionPlugin, _>(move |p| {
        p.submit(request, move |response| {
            let _ = reply.send(Ok(response_to_json(&response)));
        });
    });
    ctx.send_viewport_cmd_to(
        egui::ViewportId::ROOT,
        egui::ViewportCommand::RequestPaintWhileHidden,
    );
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_responses_round_trip_as_json() {
        let r = request_to_json(&Request::GetScreenshot {
            pixels_per_point: Some(1.0),
        });
        assert!(matches!(
            request_from_params(&r),
            Ok(Request::GetScreenshot {
                pixels_per_point: Some(_)
            })
        ));
        assert!(matches!(
            request_from_params(&json!({ "request": "GetTree" })),
            Ok(Request::GetTree)
        ));
        assert!(request_from_params(&json!({})).is_err());
        assert!(request_from_params(&json!({ "request": "Nope" })).is_err());

        let shot = Response::Screenshot(EncodedPng {
            size: [2, 1],
            bytes: vec![0x89, b'P', b'N', b'G'],
        });
        let j = response_to_json(&shot);
        assert_eq!(j["response"]["Screenshot"]["png_base64"], "iVBORw==");
        match response_from_json(&j) {
            Ok(Response::Screenshot(png)) => {
                assert_eq!(png.size, [2, 1]);
                assert_eq!(png.bytes, [0x89, b'P', b'N', b'G']);
            }
            other => panic!("{other:?}"),
        }
        let done = response_to_json(&Response::Done);
        assert!(matches!(response_from_json(&done), Ok(Response::Done)));
    }
}
