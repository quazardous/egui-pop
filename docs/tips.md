# Tips 

A tip of the day and contextual hints, registered by plugins,
translated, dismissed for good and remembered in the settings. The
gallery's `tips` page shows them.

![The tips page in pop-dark](../crates/egui-pop-gallery/tests/snapshots/tips-pop-dark.png)

## Registering tips

```rust
use egui_pop::api::tips::{TIPS, TIPS_PLUGIN, Tip, TipsPlugin};
use egui_pop::api::{App, Ctx, Plugin, PluginError};
use egui_pop::model::ids::PluginId;

struct Photos;

impl Plugin for Photos {
    fn id(&self) -> PluginId {
        PluginId::from_static("photos")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![TIPS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let tips = ctx.app().service(&TIPS).expect("required");
        tips.add(Tip::new("photos.rate", "Rate quickly", "Press 1 to 5 to rate the selected photos."));
        tips.add(Tip::new("photos.export-here", "Export from here", "Exports with the last settings.").contextual());
        Ok(())
    }
}

let app = App::builder()
    .plugin(TipsPlugin)
    .plugin(Photos)
    .build(&egui_pop::egui::Context::default())
    .unwrap();
assert_eq!(app.handle().service(&TIPS).unwrap().all().len(), 2);
```

Add `TipsPlugin` to the application. A tip has an id, an English title
and text; a contextual tip is shown only where the application asks for
it, never as the tip of the day.

## Showing them

- `tips::tip_of_the_day(ui, app)`: a card with the tip in turn among
  those not dismissed, "Previous tip", "Next tip", where it is in the
  turn, "Don't show again" and "Close" (for this session). It returns
  whether it drew a card.
- `tips::hint(ui, app, "photos.export-here")`: the tip as a card where it
  is called, beside what it is about, with "Got it".

The card is a note for screen readers; its buttons are named ("Got it:
do not show `<title>` again").

## Remembered in the settings

| Setting | What |
|---|---|
| `tips.show` | Show tips at all (on by default) |
| `tips.dismissed` | The ids of the tips not to show again, separated by commas |

Dismissing a tip writes `tips.dismissed` without an undo step, through
the `tips.dismiss` step (`tip`, `cause`: `user` or `agent`; a veto keeps
it showing). The settings are saved with the others, so a tip dismissed
stays dismissed in the next run. `tips.reset` ("Show every tip again")
empties the list; for agents, `tips.list` and `tips.dismiss` (`tip`).

## Translated

A tip's title and text are looked up under keys derived from its id,
`tip-<id>.title` and `tip-<id>.text` (the id's dots become dashes), in
the application's catalogues; the English given is the fallback. The
gallery's own tips are translated in `crates/egui-pop-gallery/locales`:

```ftl
tip-gallery-next-page =
    .text = Alt+Bas et Alt+Haut passent à la page suivante et à la précédente.
    .title = Parcourir les pages
```

The card's own text ("Tip of the day", "Next tip", "Got it"...) is in
egui-pop's catalogues.

## Survey (2026-10-13)

egui's tooltips (`on_hover_text`) stay what they are, help on hover. No
crate was found offering a tip of the day, onboarding hints or tours
for egui with dismissal kept in settings and translations: owned.

Not done yet: tips in files (`tips/<id>.md`) shipped by plugins, rules
choosing a tip from the selection, a card pointing at a widget by its
id (a hint is drawn where it is called), a per-plugin off switch.
