---
egui-pop-model: minor
egui-pop-api: minor
egui-pop-agent: minor
egui-pop-test: minor
---

The agent socket pairs scenario runners in advance, and asks the user only for the calls that destroy.

- **Pre-issued pairing.** An application issues a token at start (`egui_pop_agent::PreIssue`: from `EGUI_POP_AGENT_TOKEN`, or a new token written to the file `EGUI_POP_AGENT_TOKEN_FILE` names, mode 0600) with `AppBuilder::pre_issued_pairing`, or `pairing::pre_issue` and `pairing::pre_issue_file`. A client saying `hello` with it is paired at once as that id, for this run (never saved, `Pairing::pre_issued`), held to `PreIssue::grant`; unpaired clients keep theirs (read-only by default).
- **The live runner says `hello`.** `egui_pop_test::scenario::live::LiveRunner::token`, `token_file` and `token_from_env`, and the `egui-pop-scenario` command line's `--token` and `--token-file`, pair the runner with that token; without one it stays an unpaired client, as before.
- **Destructive by its arguments.** `CommandSpec::destructive_if(arg, value)` makes only the calls whose argument `arg` (a name, or a JSON Pointer) equals `value` destructive: an agent's run of those waits for the user's confirmation, the others run at once (an export asks only with `clash: "overwrite"`). `CommandSpec::is_destructive(args)` answers for a call; `describe` and the `commands` collector list the rules as `destructive_if`, and the MCP tools keep `destructiveHint` for such commands. `CommandSpec::destructive()` is unchanged.
