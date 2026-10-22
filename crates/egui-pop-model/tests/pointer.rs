//! Pointer chords in the keymap, and the shipped pointer profiles.

use egui_pop_model::ids::{CommandId, OwnerId};
use egui_pop_model::keymap::{
    Action, BindOptions, Button, Chord, Keymap, KeymapDoc, Keys, Layer, Pointer,
};
use egui_pop_model::pointer::{
    self, GESTURES, ImageDoubleClick, PROFILE_OWNER, PointerMap, SCOPE_GRAPH, SCOPE_IMAGE,
    SCOPE_VALUE, SCOPES, SliderWheel, ValueKeys, gesture,
};
use egui_pop_model::values::{SettingsDoc, settings_schema};

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

#[test]
fn pointer_chords_parse_print_and_stand_alone() {
    let c = chord("ctrl+wheel");
    assert_eq!(c.pointer(), Some(Pointer::Wheel));
    assert!(c.ctrl && !c.shift);
    assert_eq!(c.to_string(), "Ctrl+Wheel");
    assert_eq!(
        chord("Shift+MiddleDrag").pointer(),
        Some(Pointer::Drag(Button::Middle))
    );
    assert_eq!(chord("Shift+MiddleDrag").to_string(), "Shift+MiddleDrag");
    assert_eq!(
        chord("PrimaryDoubleClick").to_string(),
        "PrimaryDoubleClick"
    );
    assert_eq!(chord("Pinch").pointer(), Some(Pointer::Pinch));
    assert_eq!(chord("Scroll").pointer(), Some(Pointer::Scroll));
    // A key stays a key.
    assert_eq!(chord("Ctrl+K").pointer(), None);
    // Every gesture round-trips by name.
    for p in Pointer::all() {
        assert_eq!(Pointer::parse(&p.name()), Some(p));
        assert_eq!(Chord::pointer_gesture(p).to_string(), p.name());
    }
    // Never in a sequence.
    assert!(Keys::parse("Ctrl+K Wheel").is_err());
    assert!(Keys::parse("Wheel Ctrl+K").is_err());
    assert_eq!(
        Keys::parse("Alt+Wheel").unwrap().pointer().map(|(_, p)| p),
        Some(Pointer::Wheel)
    );
}

#[test]
fn pointer_bindings_keep_the_winner_per_scope_and_gesture() {
    let mut k = Keymap::new();
    let fw = OwnerId::FRAMEWORK;
    let image = BindOptions::surface(SCOPE_IMAGE);
    let graph = BindOptions::surface(SCOPE_GRAPH);
    k.bind_with(
        &fw,
        Layer::Default,
        "Wheel",
        Action::Run(gesture::IMAGE_PAN),
        "",
        image.clone(),
    )
    .unwrap();
    k.bind_with(
        &fw,
        Layer::Default,
        "Wheel",
        Action::Run(gesture::GRAPH_ZOOM),
        "",
        graph,
    )
    .unwrap();
    k.bind_with(
        &OwnerId::USER,
        Layer::User,
        "Wheel",
        Action::Run(gesture::IMAGE_ZOOM),
        "",
        image,
    )
    .unwrap();
    // A key binding is not a pointer one.
    k.bind(
        &fw,
        Layer::Default,
        "Ctrl+K",
        Action::run("keys.editor"),
        "",
    )
    .unwrap();
    let b = k.pointer_bindings();
    assert_eq!(b.len(), 2, "{b:?}");
    let image: Vec<_> = b
        .iter()
        .filter(|b| b.options.scope.to_string() == "surface:canvas.image")
        .collect();
    assert_eq!(image.len(), 1);
    assert_eq!(image[0].action, Action::Run(gesture::IMAGE_ZOOM));
    assert_eq!(image[0].layer, Layer::User);
}

#[test]
fn rebinding_all_keeps_a_gestures_scope_and_every_trigger() {
    let mut k = Keymap::new();
    let doc = pointer::profile_doc(pointer::default_profile());
    assert!(k.load(&PROFILE_OWNER, Layer::Default, &doc).is_empty());
    k.rebind_all(
        &OwnerId::USER,
        Layer::User,
        &gesture::IMAGE_PAN,
        &["Wheel", "Alt+PrimaryDrag"],
    )
    .unwrap();
    let map = PointerMap::from_keymap(&k, pointer::default_profile());
    let pans: Vec<String> = map
        .chords(SCOPE_IMAGE, &gesture::IMAGE_PAN)
        .map(Chord::to_string)
        .collect();
    assert_eq!(pans, ["Wheel", "Alt+PrimaryDrag"]);
    // The graph's pan is another command, untouched.
    assert!(map.chords(SCOPE_GRAPH, &gesture::GRAPH_PAN).count() > 1);
    // Reset: the profile again.
    k.reset(&OwnerId::USER, Layer::User, &gesture::IMAGE_PAN);
    let map = PointerMap::from_keymap(&k, pointer::default_profile());
    assert!(
        map.command(SCOPE_IMAGE, &chord("PrimaryDrag")) == Some(&gesture::IMAGE_PAN),
        "{map:?}"
    );
}

#[test]
fn the_six_profiles_ship_with_known_gestures_in_their_scopes() {
    let ids = pointer::ids();
    assert_eq!(
        ids,
        [
            "document",
            "darktable",
            "photoshop",
            "lightroom",
            "blender",
            "trackpad"
        ]
    );
    assert_eq!(pointer::default_profile().id, pointer::DEFAULT_PROFILE);
    for p in pointer::builtin() {
        assert!(!p.title.is_empty() && !p.description.is_empty(), "{}", p.id);
        let mut k = Keymap::new();
        let d = k.load(&PROFILE_OWNER, Layer::Default, &pointer::profile_doc(p));
        assert!(d.is_empty(), "{}: {d:?}", p.id);
        assert!(k.conflicts().is_empty(), "{}: {:?}", p.id, k.conflicts());
        for b in &p.bindings {
            let keys = Keys::parse(&b.keys).unwrap();
            assert!(keys.pointer().is_some(), "{}: {}", p.id, b.keys);
            let id = CommandId::new(b.command.as_deref().unwrap()).unwrap();
            let spec = pointer::gesture_spec(&id)
                .unwrap_or_else(|| panic!("{}: `{id}` is not a gesture", p.id));
            assert_eq!(b.scope, format!("surface:{}", spec.scope), "{}: {id}", p.id);
            assert!(SCOPES.contains(&spec.scope));
        }
        // Every profile zooms with a pinch and pans with two fingers.
        let map = PointerMap::from_profile(p);
        for (scope, zoom, pan) in [
            (SCOPE_IMAGE, &gesture::IMAGE_ZOOM, &gesture::IMAGE_PAN),
            (SCOPE_GRAPH, &gesture::GRAPH_ZOOM, &gesture::GRAPH_PAN),
        ] {
            assert_eq!(map.command(scope, &chord("Pinch")), Some(zoom), "{}", p.id);
            assert_eq!(map.command(scope, &chord("Scroll")), Some(pan), "{}", p.id);
        }
    }
}

#[test]
fn profiles_give_their_options_and_the_wheel_its_meaning() {
    let darktable = PointerMap::from_profile(pointer::profile("darktable").unwrap());
    assert_eq!(darktable.options.slider_wheel, SliderWheel::Value);
    assert_eq!(
        darktable.options.image_double_click,
        ImageDoubleClick::Leave
    );
    assert_eq!(
        darktable.command(SCOPE_IMAGE, &chord("MiddleClick")),
        Some(&gesture::IMAGE_ZOOM_CYCLE)
    );
    assert_eq!(
        darktable.command(SCOPE_IMAGE, &chord("Ctrl+Wheel")),
        Some(&gesture::IMAGE_ZOOM_FREE)
    );
    assert_eq!(
        darktable.command(SCOPE_VALUE, &chord("Ctrl+Alt+Wheel")),
        Some(&gesture::VALUE_PANEL)
    );
    let document = PointerMap::default();
    assert_eq!(document.profile, "document");
    assert_eq!(document.options.slider_wheel, SliderWheel::Guarded);
    assert_eq!(document.options.image_double_click, ImageDoubleClick::Fit);
    assert_eq!(
        document.command(SCOPE_IMAGE, &chord("Ctrl+Wheel")),
        Some(&gesture::IMAGE_ZOOM)
    );
    // Every gesture is declared once.
    for (i, g) in GESTURES.iter().enumerate() {
        assert!(GESTURES[i + 1..].iter().all(|o| o.id != g.id), "{}", g.id);
    }
}

fn chords_of(map: &PointerMap, scope: &str, command: &CommandId) -> Vec<String> {
    map.chords(scope, command).map(|c| c.to_string()).collect()
}

#[test]
fn photoshops_profile_scrolls_zooms_with_alt_and_is_shift_coarser_alt_finer() {
    let ps = PointerMap::from_profile(pointer::profile("photoshop").unwrap());
    assert_eq!(ps.options.image_double_click, ImageDoubleClick::Nothing);
    for (keys, command) in [
        ("Wheel", Some(&gesture::IMAGE_PAN)),
        ("Ctrl+Wheel", Some(&gesture::IMAGE_PAN_X)),
        ("Alt+Wheel", Some(&gesture::IMAGE_ZOOM)),
        ("Shift+Wheel", None),
        ("PrimaryDrag", Some(&gesture::IMAGE_PAN)),
        // The Hand tool's click does nothing; only the Zoom tool zooms.
        ("PrimaryClick", None),
        ("MiddleDrag", None),
    ] {
        assert_eq!(ps.command(SCOPE_IMAGE, &chord(keys)), command, "{keys}");
    }
    assert_eq!(
        chords_of(&ps, SCOPE_VALUE, &gesture::VALUE_FINE),
        ["Alt+PrimaryDrag"]
    );
    assert_eq!(
        chords_of(&ps, SCOPE_VALUE, &gesture::VALUE_COARSE),
        ["Shift+PrimaryDrag"]
    );
    assert_eq!(ps.command(SCOPE_VALUE, &chord("Ctrl+PrimaryDrag")), None);
    // The arrows follow the drag.
    assert_eq!(ps.value_keys, ValueKeys::default());
}

#[test]
fn lightrooms_profile_clicks_to_zoom_and_has_its_own_arrow_modifiers() {
    let lr = PointerMap::from_profile(pointer::profile("lightroom").unwrap());
    assert_eq!(lr.profile, "lightroom");
    assert_eq!(lr.options.image_double_click, ImageDoubleClick::Leave);
    assert_eq!(lr.options.slider_wheel, SliderWheel::Guarded);
    for (keys, command) in [
        ("PrimaryClick", Some(&gesture::IMAGE_ZOOM_TOGGLE)),
        ("PrimaryDrag", Some(&gesture::IMAGE_PAN)),
        // Lightroom's plain wheel does not zoom or pan the Loupe:
        // unbound; Ctrl with it zooms.
        ("Wheel", None),
        ("Ctrl+Wheel", Some(&gesture::IMAGE_ZOOM)),
        ("Alt+Wheel", None),
        ("MiddleDrag", None),
    ] {
        assert_eq!(lr.command(SCOPE_IMAGE, &chord(keys)), command, "{keys}");
    }
    // A Shift drag is finer; no drag is coarser; Alt is the
    // application's (clipping on the Basic panel), not a step.
    assert_eq!(
        chords_of(&lr, SCOPE_VALUE, &gesture::VALUE_FINE),
        ["Shift+PrimaryDrag"]
    );
    assert!(chords_of(&lr, SCOPE_VALUE, &gesture::VALUE_COARSE).is_empty());
    assert_eq!(lr.command(SCOPE_VALUE, &chord("Alt+PrimaryDrag")), None);
    // The arrows: Shift larger, Alt smaller.
    assert_eq!(lr.value_keys.fine, "Alt");
    assert_eq!(lr.value_keys.coarse, "Shift");
}

#[test]
fn the_retired_adobe_id_names_lightroom_and_is_listed_nowhere() {
    assert_eq!(pointer::canonical_id("adobe"), "lightroom");
    assert_eq!(pointer::canonical_id("photoshop"), "photoshop");
    assert_eq!(pointer::profile("adobe").unwrap().id, "lightroom");
    assert!(!pointer::ids().contains(&"adobe"));
    for (alias, to) in pointer::ALIASES {
        assert!(pointer::profile(alias).is_some(), "{alias}");
        assert!(pointer::ids().contains(to), "{alias} names {to}");
    }
    // A settings document of version 1 that saved `adobe` reads as
    // `lightroom`; other values are left alone.
    let old = r#"{"version": 1, "kind": "settings", "values": {"settings/input.pointer_profile": "adobe", "settings/input.slider_wheel": "value"}}"#;
    let read = settings_schema().load::<SettingsDoc>(old).unwrap();
    assert_eq!(
        read.value.values["settings/input.pointer_profile"],
        serde_json::json!("lightroom")
    );
    assert_eq!(
        read.value.values["settings/input.slider_wheel"],
        serde_json::json!("value")
    );
}

#[test]
fn keymap_documents_of_version_2_read_as_version_3() {
    let old = r#"{"version": 2, "kind": "keymap", "bindings": [{"keys": "Ctrl+K", "command": "demo.go"}]}"#;
    let read = egui_pop_model::keymap_schema()
        .load::<KeymapDoc>(old)
        .unwrap();
    assert_eq!(read.from_version, 2);
    assert_eq!(read.value.bindings[0].keys, "Ctrl+K");
    assert!(egui_pop_model::keymap_schema().gaps().is_empty());
    let saved = egui_pop_model::keymap_schema().save(&read.value).unwrap();
    assert!(saved.contains("\"version\": 3"), "{saved}");
    // A shipped profile reads through its own schema.
    let text = pointer::schema().save(pointer::default_profile()).unwrap();
    assert_eq!(
        &pointer::parse_profile(&text).unwrap(),
        pointer::default_profile()
    );
}
