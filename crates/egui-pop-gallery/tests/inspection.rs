//! The gallery behind the `egui_inspection` protocol, as `egui-mcp`
//! reaches it with the `inspection` feature (docs/inspection.md): the
//! plugin eframe attaches is attached to a headless harness, served on a
//! loopback port, and a client asks for the accessibility tree over TCP.
//! No window: eframe's own attach (`EGUI_INSPECTION`) needs one, so the
//! test attaches the same plugin itself.

use std::io::{BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui_inspection::protocol::{
    PROTOCOL_VERSION, Request, Response, read_handshake, read_message, write_message,
};
use egui_kittest::Harness;
use egui_pop::theme::Density;
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;

/// Long enough for a loaded machine; the plugin itself gives up at 20 s.
const DEADLINE: Duration = Duration::from_secs(30);

fn harness() -> Harness<'static, Gallery> {
    let options = Options {
        theme: "pop-dark".to_string(),
        density: Density::Comfortable,
        page: "overview".to_string(),
        ..Options::default()
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .shared_wgpu()
        .build_eframe(move |cc| {
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        });
    h.run();
    h
}

/// Serves the plugin on a free loopback port and returns the port.
fn serve(ctx: &egui::Context) -> u16 {
    ctx.add_plugin(egui_inspection::InspectionPlugin::new(Some(
        "egui-pop gallery".into(),
    )));
    // `serve` does not say which port an `:0` got: take a free one, let
    // it go and bind it again, retrying if another process took it.
    for _ in 0..10 {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free loopback port")
            .port();
        if egui_inspection::serve(ctx, &format!("127.0.0.1:{port}")).is_ok() {
            return port;
        }
    }
    panic!("no loopback port to serve on");
}

/// Sends `requests` from a client thread, one after the other, while the
/// harness runs frames; returns the responses in order.
fn ask(h: &mut Harness<'_, Gallery>, port: u16, requests: Vec<Request>) -> Vec<Response> {
    let (tx, rx) = mpsc::channel();
    let count = requests.len();
    std::thread::spawn(move || {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("the endpoint accepts");
        stream.set_read_timeout(Some(DEADLINE)).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = BufWriter::new(stream);
        let version = read_handshake(&mut reader).expect("the handshake");
        assert_eq!(version, PROTOCOL_VERSION);
        for request in requests {
            write_message(&mut writer, &request).expect("the request goes out");
            let response: Response = read_message(&mut reader).expect("a response");
            if tx.send(response).is_err() {
                return;
            }
        }
    });
    let start = Instant::now();
    let mut responses = Vec::new();
    while responses.len() < count {
        assert!(
            start.elapsed() < DEADLINE,
            "the endpoint answered {} of {count} requests in {DEADLINE:?}",
            responses.len()
        );
        // The plugin answers inside a frame's hooks.
        h.step();
        while let Ok(r) = rx.try_recv() {
            responses.push(r);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    responses
}

#[test]
fn the_endpoint_answers_with_the_gallery_s_tree() {
    let mut h = harness();
    let port = serve(&h.ctx.clone());
    let responses = ask(&mut h, port, vec![Request::GetInfo, Request::GetTree]);

    let Response::Info { label, .. } = &responses[0] else {
        panic!("not an info: {:?}", responses[0]);
    };
    assert_eq!(label.as_deref(), Some("egui-pop gallery"));

    let Response::Tree {
        accesskit: Some(update),
        ..
    } = &responses[1]
    else {
        panic!("not a tree: {:?}", responses[1]);
    };
    // The menu bar's Help menu, which holds "Report a problem": what an
    // agent clicks by its label.
    let labels: Vec<String> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().map(str::to_string))
        .collect();
    assert!(
        labels.iter().any(|l| l == "Help"),
        "no Help menu among {} labels: {labels:?}",
        labels.len()
    );
}
