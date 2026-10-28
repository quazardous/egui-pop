//! The tips browser ("Tips…"): every tip, grouped by the plugin that
//! declared it, each group with its own switch, each tip with what it
//! says, whether it was dismissed or its command used, and a button
//! showing it.

use egui::Ui;
use egui_pop_core::strings;

use super::{TIPS, Tip, markup};
use crate::app::AppHandle;
use crate::strings as txt;

/// The tips grouped by origin, in the order their first tip was
/// registered.
pub fn groups(tips: &[Tip]) -> Vec<(String, Vec<Tip>)> {
    let mut out: Vec<(String, Vec<Tip>)> = Vec::new();
    for t in tips {
        let origin = t.origin_name();
        match out.iter_mut().find(|(o, _)| o == origin) {
            Some((_, list)) => list.push(t.clone()),
            None => out.push((origin.to_string(), vec![t.clone()])),
        }
    }
    out
}

/// An origin's name as shown: its plugin's title when a plugin has that
/// id, else the origin itself.
fn origin_title(app: &AppHandle, origin: &str) -> String {
    app.plugins()
        .into_iter()
        .find(|p| p.id.as_str() == origin)
        .filter(|p| !p.title.is_empty())
        .map(|p| {
            strings::declared(
                &app.egui(),
                strings::kind::PLUGIN,
                origin,
                Some("title"),
                &p.title,
            )
        })
        .unwrap_or_else(|| origin.to_string())
}

/// Draws the browser (it needs [`super::TipsPlugin`]).
pub fn browser(ui: &mut Ui, app: &AppHandle) {
    let Some(tips) = app.service(&TIPS) else {
        return;
    };
    let ctx = ui.ctx().clone();
    let all = tips.all();
    if all.is_empty() {
        ui.weak(strings::tr(&ctx, &txt::TIPS_NONE));
        return;
    }
    let dismissed = tips.dismissed(app);
    let used = tips.used(app);
    let off = tips.off(app);
    for (origin, list) in groups(&all) {
        let title = origin_title(app, &origin);
        ui.horizontal(|ui| {
            ui.strong(&title);
            let mut on = !off.contains(&origin);
            let label = strings::tr(&ctx, &txt::TIPS_ORIGIN_ON);
            let r = ui.checkbox(&mut on, &label);
            let name = strings::tr_args(&ctx, &txt::TIPS_ORIGIN_ON_NAME, &[("origin", &title)]);
            egui_pop_core::a11y::name(&r, egui::WidgetType::Checkbox, &name);
            if r.changed()
                && let Err(e) = tips.set_origin_on(app, &origin, on)
            {
                log::warn!("tips: {e}");
            }
        });
        ui.indent(("egui_pop.tips.browser", &origin), |ui| {
            for tip in &list {
                let tip_title = tip.title_in(&ctx);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&tip_title).strong());
                    let state = if dismissed.contains(&tip.id) {
                        Some(&txt::TIPS_STATE_DISMISSED)
                    } else if tip
                        .command
                        .as_ref()
                        .is_some_and(|c| used.contains(c.as_str()))
                    {
                        Some(&txt::TIPS_STATE_USED)
                    } else {
                        None
                    };
                    if let Some(s) = state {
                        ui.weak(strings::tr(&ctx, s));
                    }
                    let r = ui.small_button(strings::tr(&ctx, &txt::TIPS_SHOW));
                    let name = strings::tr_args(&ctx, &txt::TIPS_SHOW_NAME, &[("tip", &tip_title)]);
                    egui_pop_core::a11y::name(&r, egui::WidgetType::Button, &name);
                    if r.clicked()
                        && let Err(e) = tips.show(app, &tip.id)
                    {
                        log::warn!("tips: {e}");
                    }
                });
                let text = tip.text_shown(&ctx, app);
                let job = markup::job(&text, ui.style(), ui.available_width());
                ui.label(job);
                ui.add_space(ui.spacing().item_spacing.y);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_group_by_origin_in_the_order_registered() {
        let tips = [
            Tip::new("photos.rate", "Rate", "R."),
            Tip::new("maps.zoom", "Zoom", "Z."),
            Tip::new("photos.compare", "Compare", "C."),
            Tip::new("any.thing", "Thing", "T.").origin("maps"),
        ];
        let g: Vec<(String, usize)> = groups(&tips)
            .into_iter()
            .map(|(o, l)| (o, l.len()))
            .collect();
        assert_eq!(g, [("photos".to_string(), 2), ("maps".to_string(), 2)]);
    }
}
