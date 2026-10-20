# AccessKit bounds ignore layer transforms

> **Before submitting (delete this block).** Repository: emilk/egui, "Bug
> report" template (label `bug`); fill its OS / version fields. egui's
> CONTRIBUTING ("Agents & LLM policy"): the human filing is responsible for
> the issue; no AI disclosure is required, but one line is welcome. No
> duplicate found (searched issues and PRs for accesskit + transform / layer /
> Scene / bounds, 2026-10-20). Checked against `main` at db63b6d.

**Describe the bug**

A widget in a transformed layer (`Context::set_transform_layer`, which
`Scene` uses) reports its rectangle to AccessKit in the layer's own
coordinates, before the transform. Screen readers, and egui_kittest (which
clicks at the centre of a node's bounds), then place the widget where it is
not drawn.

`Response::fill_accesskit_node_common` sets the bounds from `self.rect` as
is:
<https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui/src/response.rs#L990-L1000>

whereas hit testing applies the layer transform
(`Context::rect_contains_pointer`):
<https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui/src/context.rs#L3657-L3661>

Text runs already go through a transform (`global_from_galley` in
`text_selection/accesskit_text.rs`), so only the widget bounds are affected.

Found while building a desktop framework on egui
(<https://github.com/quazardous/egui-pop>), whose node-graph editor lives in
a `Scene`.

**To Reproduce**

```rust
use egui::emath::TSTransform;
use egui_kittest::{kittest::Queryable as _, Harness};

#[test]
fn accesskit_bounds_follow_layer_transform() {
    let mut clicks = 0;
    let mut harness = Harness::new_ui(|ui| {
        let ctx = ui.ctx().clone();
        let area = egui::Area::new(egui::Id::new("moved"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(&ctx, |ui| ui.button("inside"));
        if area.inner.clicked() {
            clicks += 1;
        }
        ctx.set_transform_layer(
            area.response.layer_id,
            TSTransform::new(egui::vec2(100.0, 50.0), 2.0),
        );
    });
    harness.run();

    // The button is painted at (100, 50) and twice its size,
    // but its node still has the untransformed rect near (0, 0):
    println!("{:?}", harness.get_by_label("inside").rect());

    harness.get_by_label("inside").click();
    harness.run();
    drop(harness);
    assert_eq!(clicks, 1); // fails: the click lands outside the painted button
}
```

**Expected behavior**

The node's bounds are the widget's rectangle on screen, that is
`layer_transform_to_global(layer_id) * rect`, so assistive technology and
kittest find the widget where it is drawn.

**Suggested fix**

In `Response::fill_accesskit_node_common`:

```rust
let rect = self
    .ctx
    .layer_transform_to_global(self.layer_id)
    .map_or(self.rect, |to_global| to_global * self.rect);
builder.set_bounds(accesskit::Rect {
    x0: rect.min.x.into(),
    y0: rect.min.y.into(),
    x1: rect.max.x.into(),
    y1: rect.max.y.into(),
});
```

(Widgets that set their bounds themselves after this call keep doing so.)
