//! The tips' lint: every tip checked against the application, so a
//! renamed command or target cannot leave a dead tip behind.

use egui_pop_model::when::When;

use super::{TIPS, Tip, markup, targets_seen};
use crate::app::AppHandle;

/// What is wrong with the tips registered, one line each: an unknown
/// command (its own or one a `{key:<command>}` names), a `{key}`
/// without a command, a surface that does not parse, a target never
/// named (neither in `targets` nor drawn so far with [`super::mark`] or
/// [`super::target`]), an id not prefixed by its origin, an empty title
/// or text.
pub fn problems(app: &AppHandle, targets: &[&str]) -> Vec<String> {
    let Some(tips) = app.service(&TIPS) else {
        return vec!["the tips plugin is not connected".into()];
    };
    let commands: Vec<String> = app
        .commands()
        .into_iter()
        .map(|(s, _)| s.id.as_str().to_string())
        .collect();
    let mut named: Vec<String> = targets.iter().map(|t| t.to_string()).collect();
    named.extend(targets_seen(&app.egui()));
    tips.all()
        .iter()
        .flat_map(|t| check(t, &commands, &named))
        .collect()
}

/// One tip's problems.
pub fn check(tip: &Tip, commands: &[String], targets: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let id = &tip.id;
    let mut say = |what: String| out.push(format!("tip `{id}`: {what}"));
    if tip.title.trim().is_empty() {
        say("its title is empty".into());
    }
    if tip.text.trim().is_empty() {
        say("its text is empty".into());
    }
    if !tip.origin.is_empty() && !id.starts_with(&format!("{}.", tip.origin)) {
        say(format!(
            "its id is not prefixed by its origin `{}`",
            tip.origin
        ));
    }
    match &tip.command {
        Some(c) if !commands.iter().any(|k| k == c.as_str()) => {
            say(format!("its command `{c}` is not declared"));
        }
        None if markup::has_own_key(&tip.text) => {
            say("`{key}` without a command".into());
        }
        _ => {}
    }
    for c in markup::named_commands(&tip.text) {
        if !commands.contains(&c) {
            say(format!("`{{key:{c}}}` names a command not declared"));
        }
    }
    if !tip.surface.is_empty()
        && let Err(e) = When::parse(&tip.surface)
    {
        say(format!("its surface `{}` does not parse: {e}", tip.surface));
    }
    if let Some(t) = &tip.target
        && !targets.contains(t)
    {
        say(format!("its target `{t}` is never named"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_model::ids::CommandId;

    #[test]
    fn a_dead_tip_is_reported() {
        let commands = vec!["photos.rate".to_string()];
        let targets = vec!["photos.stars".to_string()];
        let good = Tip::new("photos.rate", "Rate", "Press {key}.")
            .command(CommandId::from_static("photos.rate"))
            .surface("panel == 'grid'")
            .target("photos.stars")
            .origin("photos");
        assert!(check(&good, &commands, &targets).is_empty());
        let dead = Tip::new("rate", "Rate", "Press {key}, {key:photos.gone}.")
            .command(CommandId::from_static("photos.renamed"))
            .surface("panel ==")
            .target("photos.moved")
            .origin("photos");
        let p = check(&dead, &commands, &targets);
        assert_eq!(p.len(), 5, "{p:#?}");
        let bare = Tip::new("x.y", "T", "Press {key}.");
        assert_eq!(check(&bare, &commands, &targets).len(), 1);
    }
}
