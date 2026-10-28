//! The links' lint: every link the application declares checked against
//! the application and the policy, so a renamed command, a removed file
//! or a blocked scheme cannot leave a dead link behind.

use egui_pop_model::links::{self as model, Target};
use egui_pop_model::menus::Item;

use super::{CMD_OPEN, CMD_REVEAL, declared, links};
use crate::app::AppHandle;

/// What is wrong with the links the application declares, one line each:
/// a target that does not parse, a blocked scheme, a `command:` link
/// naming a command that is not declared, a local file that is not
/// there, a `docs:` page the viewer does not have (when a viewer is
/// installed and knows its pages). Walked: links declared by plugins ([`crate::Ctx::link`],
/// [`super::declare`]) and drawn as declared so far ([`super::link`]),
/// menu items running `links.open` or `files.reveal`, the links in the
/// tips' texts, and the actions of the toasts shown so far.
pub fn problems(app: &AppHandle) -> Vec<String> {
    let Some(links) = links(app) else {
        return vec!["the links plugin is not connected".into()];
    };
    let policy = links.policy();
    let docs = links.inside().and_then(|i| i.docs_pages());
    let commands: Vec<String> = app
        .commands()
        .into_iter()
        .map(|(s, _)| s.id.as_str().to_string())
        .collect();
    let mut all: Vec<(String, String)> = declared(app);
    for menu in app.menu_ids() {
        for item in app.menu(&menu) {
            if let Item::Command { id, args, .. } = &item.item
                && (*id == CMD_OPEN || *id == CMD_REVEAL)
            {
                let key = if *id == CMD_OPEN { "target" } else { "path" };
                match args[key].as_str() {
                    Some(t) => all.push((format!("menu `{menu}`"), t.to_string())),
                    None => all.push((format!("menu `{menu}`"), String::new())),
                }
            }
        }
    }
    if let Some(tips) = app.service(&crate::tips::TIPS) {
        for tip in tips.all() {
            for f in model::find(&tip.text) {
                all.push((format!("tip `{}`", tip.id), f.target));
            }
        }
    }
    if let Some(toasts) = app.service(&crate::toasts::TOASTS) {
        for t in toasts.history().into_iter().chain(toasts.shown()) {
            if let Some(a) = &t.toast.action
                && (a.command == CMD_OPEN || a.command == CMD_REVEAL)
                && let Some(target) = a.args["target"].as_str().or(a.args["path"].as_str())
            {
                all.push((format!("toast `{}`", t.toast.text), target.to_string()));
            }
        }
    }
    all.sort();
    all.dedup();
    all.iter()
        .filter_map(|(place, target)| {
            check(&policy, target, &commands, docs.as_deref())
                .err()
                .map(|e| format!("{place}: {e}"))
        })
        .collect()
}

/// One declared link's problem, if any; `docs` lists the `docs:` pages
/// there are (`None`: not checked).
pub fn check(
    policy: &model::Policy,
    target: &str,
    commands: &[String],
    docs: Option<&[String]>,
) -> Result<(), String> {
    model::check_declared(policy, target, commands, docs)?;
    // A file the application ships or names must be there.
    if let Ok(t) = Target::parse(target)
        && let Some(facts) = super::facts_of(&t)
        && !facts.exists
    {
        return Err(format!("`{target}`: the file is not there"));
    }
    Ok(())
}
