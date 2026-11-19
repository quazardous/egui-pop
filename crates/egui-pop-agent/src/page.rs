//! The Agents page of the Settings app (`developer/agents`), declared as
//! a manifest: the paired agents (name, id, paired since, last seen,
//! connected now) with their access, and the defaults. The access is a
//! preset chosen in a row field, private data (places, people, file
//! paths) a switch beside it ([`CMD_GRANT`], applied live through
//! `set_grants`); a row's Revoke asks, then revokes ([`CMD_REVOKE`]).
//!
//! Its commands that change something are the user's: kept from agents
//! (`no_agent`), and refused to any other origin by their handlers too.

use egui_pop_api::admin::ADMIN;
use egui_pop_api::admin::model::{Action, Confirmation, FORMAT, Manifest, Records, RowField};
use egui_pop_api::{AppHandle, Ctx, PluginError};
use egui_pop_core::strings::{self, StrKey};
use egui_pop_model::commands::{CommandSpec, Origin};
use serde_json::{Value as Json, json};

use crate::grants::{self, Grant};
use crate::pairing::{self, CMD_GRANT, CMD_LIST, CMD_REVOKE};
use crate::strings as txt;

/// The page's path in the Settings app.
pub(crate) const PATH: &str = "developer/agents";

/// The presets a grant may be set to, by name.
const PRESETS: [&str; 3] = ["read-only", "editor", "full"];

fn preset(name: &str) -> Option<Grant> {
    match name {
        "read-only" => Some(Grant::read_only()),
        "editor" => Some(Grant::editor()),
        "full" => Some(Grant::full()),
        _ => None,
    }
}

/// The preset a grant is, or `custom`. Private data is a switch of its
/// own: withholding it leaves the preset as it was.
fn preset_of(g: &Grant) -> &'static str {
    let g = g.clone().private(true);
    PRESETS
        .into_iter()
        .find(|p| preset(p).as_ref() == Some(&g))
        .unwrap_or("custom")
}

/// A grant in words, in the interface's language.
fn details(ctx: &egui::Context, g: &Grant) -> String {
    let list = |v: &[String]| {
        if v.is_empty() {
            strings::tr(ctx, &txt::NONE)
        } else {
            v.join(", ")
        }
    };
    match preset_of(g) {
        "read-only" => strings::tr(ctx, &txt::DETAIL_READ_ONLY),
        "editor" => strings::tr(ctx, &txt::DETAIL_EDITOR),
        "full" => strings::tr(ctx, &txt::DETAIL_FULL),
        _ if g.read_only => strings::tr_args(
            ctx,
            &txt::DETAIL_CUSTOM_READ,
            &[("commands", &list(&g.commands))],
        ),
        _ => strings::tr_args(
            ctx,
            &txt::DETAIL_CUSTOM,
            &[
                ("commands", &list(&g.commands)),
                ("values", &list(&g.values)),
                (
                    "ui",
                    &strings::tr(ctx, if g.ui { &txt::YES } else { &txt::NO }),
                ),
            ],
        ),
    }
}

/// A moment, in seconds since the Unix epoch, as `2026-11-19 10:42 UTC`.
fn moment(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The choice a row offers besides the presets: its own grant, when it
/// is none of them.
fn custom_choice(ctx: &egui::Context, preset: &str) -> Json {
    if preset == "custom" {
        json!([{ "value": "custom", "label": strings::tr(ctx, &txt::PRESET_CUSTOM) }])
    } else {
        json!([])
    }
}

/// What `pop.agent.list` answers.
fn list(app: &AppHandle) -> Json {
    let ctx = app.egui();
    let g = grants::grants(app);
    let agents: Vec<Json> = pairing::pairings(app)
        .into_iter()
        .map(|p| {
            let grant = g.agents.get(&p.id).unwrap_or(&g.paired);
            let preset = preset_of(grant);
            let open = pairing::connected(app, &p.id);
            json!({
                "id": p.id,
                "name": p.name,
                "pairedSince": moment(p.paired_at),
                "lastSeen": if p.last_seen == 0 {
                    strings::tr(&ctx, &txt::NEVER)
                } else {
                    moment(p.last_seen)
                },
                "connected": if open == 0 {
                    strings::tr(&ctx, &txt::NO)
                } else {
                    strings::tr_args(&ctx, &txt::CONNECTED_YES, &[("count", &open.to_string())])
                },
                "connections": open,
                "state": if open == 0 { "info" } else { "success" },
                "preset": preset,
                "private": grant.private,
                "own": g.agents.contains_key(&p.id),
                "custom": custom_choice(&ctx, preset),
                "details": details(&ctx, grant),
            })
        })
        .collect();
    let default = |id: &str, who: &StrKey, grant: &Grant| {
        let preset = preset_of(grant);
        json!({
            "id": id,
            "who": strings::tr(&ctx, who),
            "preset": preset,
            "private": grant.private,
            "custom": custom_choice(&ctx, preset),
            "details": details(&ctx, grant),
        })
    };
    json!({
        "agents": agents,
        "defaults": [
            default("unpaired", &txt::DEFAULT_UNPAIRED, &g.unpaired),
            default("paired", &txt::DEFAULT_PAIRED, &g.paired),
        ],
    })
}

/// Only the user changes grants and pairings.
fn user_only(origin: &Origin) -> Result<(), String> {
    if *origin == Origin::User {
        Ok(())
    } else {
        Err("only the user changes agents' access and pairings".into())
    }
}

/// A switch's value: a boolean, or its text.
fn switch(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        Json::String(s) if s == "true" => Some(true),
        Json::String(s) if s == "false" => Some(false),
        _ => None,
    }
}

/// `pop.agent.grant`: a preset, private data shown or withheld, or both,
/// set live.
fn grant(app: &AppHandle, args: &Json) -> Result<Json, String> {
    let name = args["preset"].as_str();
    let private = match &args["private"] {
        Json::Null => None,
        v => Some(switch(v).ok_or("`private` is a boolean")?),
    };
    if name.is_none() && private.is_none() {
        return Err("`preset` or `private` is missing".into());
    }
    // `custom` keeps the grant the app set.
    let chosen = match name {
        None | Some("custom") => None,
        Some(n) => {
            Some(preset(n).ok_or_else(|| format!("no preset `{n}`: read-only, editor or full"))?)
        }
    };
    if chosen.is_none() && private.is_none() {
        return Ok(json!({ "changed": false }));
    }
    let mut g = grants::grants(app);
    let slot = if let Some(id) = args["agent"].as_str() {
        if !pairing::pairings(app).iter().any(|p| p.id == id) {
            return Err(format!("no agent is paired as `{id}`"));
        }
        let paired = g.paired.clone();
        g.agents.entry(id.into()).or_insert(paired)
    } else {
        match args["default"].as_str() {
            Some("unpaired") => &mut g.unpaired,
            Some("paired") => &mut g.paired,
            _ => return Err("`agent`, or `default` (unpaired or paired), is missing".into()),
        }
    };
    // A new preset keeps the private switch; the switch keeps the rest.
    let keep = slot.private;
    if let Some(p) = chosen {
        *slot = p.private(keep);
    }
    if let Some(on) = private {
        slot.private = on;
    }
    let result = json!({ "changed": true, "preset": preset_of(slot), "private": slot.private });
    grants::set_grants(app, g);
    Ok(result)
}

/// The commands and the page.
pub(crate) fn declare(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(
        CommandSpec::new(CMD_LIST, "List paired agents", "Agents")
            .description("The paired agents (connected now, last seen) and the defaults, with what each may do.")
            .hidden()
            .read_only(),
        |call| Ok(list(call.app)),
    )?;
    ctx.command(
        CommandSpec::new(CMD_GRANT, "Set an agent's access", "Agents")
            .description("Sets a paired agent's access (`agent`), or a default's (`default`: unpaired or paired): a preset (`preset`: read-only, editor or full), private data shown or withheld (`private`: places, people, file paths), or both. The user's only.")
            .args(json!({ "agent": "claude-code", "preset": "editor", "private": true }))
            .hidden()
            .no_agent(),
        |call| {
            user_only(call.origin)?;
            grant(call.app, call.args)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_REVOKE, "Revoke an agent's pairing", "Agents")
            .description("Revokes a pairing (`agent`): its token no longer pairs and its open connections close. The user's only.")
            .args(json!({ "agent": "claude-code" }))
            .hidden()
            .destructive()
            .no_agent(),
        |call| {
            user_only(call.origin)?;
            let id = call.args["agent"].as_str().ok_or("`agent` is missing")?;
            let closed = pairing::revoke(call.app, id)?;
            Ok(json!({ "agent": id, "closed": closed }))
        },
    )?;
    if let Some(admin) = ctx.app().service(&ADMIN) {
        admin.texts(txt::PAGE);
        admin.manifest(manifest());
    }
    Ok(())
}

/// The page, declared.
pub(crate) fn manifest() -> Manifest {
    let t = |k: &StrKey| k.fallback;
    let choices = |cmd_arg: Json| {
        RowField::choice(
            "preset",
            t(&txt::COL_ACCESS),
            &[
                (json!("read-only"), t(&txt::PRESET_READ_ONLY)),
                (json!("editor"), t(&txt::PRESET_EDITOR)),
                (json!("full"), t(&txt::PRESET_FULL)),
            ],
            CMD_GRANT.as_str(),
            cmd_arg,
        )
        .choices_from("custom")
        .help(t(&txt::ACCESS_HELP))
    };
    let private = |cmd_arg: Json| {
        RowField::toggle("private", t(&txt::COL_PRIVATE), CMD_GRANT.as_str(), cmd_arg)
            .help(t(&txt::PRIVATE_HELP))
    };
    Manifest::new(FORMAT).section(PATH, t(&txt::AGENTS_SECTION), |s| {
        s.description(t(&txt::AGENTS_ABOUT))
            .keywords(&["agent", "pairing", "token", "grant", "mcp", "socket"])
            .group("paired", t(&txt::AGENTS_PAIRED))
            .table(
                Records::command("agents", "", CMD_LIST.as_str(), "/agents", "id")
                    .column("name", t(&txt::COL_NAME))
                    .column("id", t(&txt::COL_ID))
                    .column("pairedSince", t(&txt::COL_PAIRED))
                    .column("lastSeen", t(&txt::COL_SEEN))
                    .column("connected", t(&txt::COL_CONNECTED))
                    .column("preset", t(&txt::COL_ACCESS))
                    .column("details", t(&txt::COL_DETAILS))
                    .tone("state")
                    .row_field(choices(
                        json!({ "agent": "${row.id}", "preset": "${value}" }),
                    ))
                    .row_field(private(
                        json!({ "agent": "${row.id}", "private": "${value}" }),
                    ))
                    .row_action(
                        Action::command(CMD_REVOKE.as_str())
                            .title(t(&txt::REVOKE))
                            .args(json!({ "agent": "${row.id}" }))
                            .danger()
                            .confirmation(
                                Confirmation::new(
                                    t(&txt::REVOKE_TITLE),
                                    t(&txt::REVOKE_TEXT),
                                    t(&txt::REVOKE),
                                )
                                .danger(),
                            ),
                    )
                    .empty(t(&txt::AGENTS_EMPTY), None)
                    .refresh("agent.*"),
            )
            .group("defaults", t(&txt::AGENTS_DEFAULTS))
            .table(
                Records::command("defaults", "", CMD_LIST.as_str(), "/defaults", "id")
                    .column("who", t(&txt::COL_WHO))
                    .column("preset", t(&txt::COL_ACCESS))
                    .column("details", t(&txt::COL_DETAILS))
                    .row_field(choices(
                        json!({ "default": "${row.id}", "preset": "${value}" }),
                    ))
                    .row_field(private(
                        json!({ "default": "${row.id}", "private": "${value}" }),
                    )),
            );
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_are_civil_utc() {
        assert_eq!(moment(0), "1970-01-01 00:00 UTC");
        assert_eq!(moment(1_795_000_000), "2026-11-18 11:06 UTC");
    }

    #[test]
    fn presets_are_named_and_others_are_custom() {
        assert_eq!(preset_of(&Grant::read_only()), "read-only");
        assert_eq!(preset_of(&Grant::editor()), "editor");
        assert_eq!(preset_of(&Grant::full()), "full");
        assert_eq!(preset_of(&Grant::editor().commands(["about.*"])), "custom");
        // Private data withheld leaves the preset as it was.
        assert_eq!(preset_of(&Grant::editor().private(false)), "editor");
    }

    #[test]
    fn the_page_checks_and_its_texts_are_registered() {
        let mut m = manifest();
        let d = m.check(&["developer".into()], &|_| true);
        assert!(d.is_empty(), "{d:?}");
        let text = m.to_json().to_string();
        assert!(text.contains(txt::REVOKE_TITLE.fallback));
        assert!(text.contains(txt::AGENTS_SECTION.fallback));
    }
}
