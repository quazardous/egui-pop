# Governance

How egui-pop is decided, how decisions are kept, and what the project
promises the people who build on it: applications and extensions.

## Who decides

- **Maintainers** are the people with merge rights on the repository.
  They review pull requests, run releases and keep the
  [registry of extensions](docs/extensions.md).
- **The lead maintainer** has the final word when maintainers disagree,
  and decides what does not fit an existing rule. Today the project has
  one maintainer, who is its lead.
- **Contributors** propose changes through issues and pull requests
  ([CONTRIBUTING.md](CONTRIBUTING.md)). A maintainer's own change goes
  through a pull request too when it touches a contract below.
- Becoming a maintainer: after sustained, reviewed contributions, on
  the lead maintainer's invitation; the change is recorded in this file.

Decisions follow the [credo](CREDO.md), in its order of precedence.

## How decisions are recorded

A decision that shapes the code is an
[architecture decision record](docs/adr/README.md): context, decision,
consequences, half a page, dated. An ADR is never edited afterwards
except to mark it superseded by a later one.

A change to a contract extensions rely on (below), and any change to
`egui-pop-api` beyond an addition, starts as a **request for comments**:
a pull request adding an ADR with `Status: proposed`. It stays open for
comments at least seven days, and is merged with `Status: accepted` (or
closed, with the reason in the thread) by the lead maintainer. The code
may land in the same pull request or after it, never before.

Smaller decisions live in the commit that makes them: what was chosen,
and why (a survey of existing crates, when one was adopted or not).

## Versions

egui-pop follows [Semantic Versioning](https://semver.org/) with **one
egui-pop minor per egui minor**: each egui minor brings an egui-pop
minor built on it, and an extension supports an egui-pop minor exactly
as it supports that egui. That is the `egui-pop` crate's version; each
sub-crate has its own, which moves only when the crate does (see
[API stability](docs/stability.md#versions-of-the-crates)). Before 1.0 a minor may break the API; patch
releases only add or fix. What each item promises (stable, supported,
experimental) is in [API stability](docs/stability.md).

## What breaks an extension

The **extension contract** is what an extension or an application sees
of egui-pop besides Rust signatures. A change to it is breaking, and
waits for a minor, when it removes, renames or changes the meaning of:

- **The application API and the host contract**: `Plugin`, `Ctx`,
  `AppBuilder`, `App`, `AppHandle`, zones and blocks, services,
  stories, hooks, and the `Host` trait with its `Dispatch`, `Reply` and
  journal (experimental for now: see [API stability](docs/stability.md)).
- **Tokens**: a system token's path, its type or the role it names; a
  component token egui-pop documents; a contrast pair the built-in
  themes guarantee (`contrast::PAIRS`), since extensions alias system
  tokens on the strength of it; the DTCG file format.
- **Steps**: a step egui-pop declares, its arguments' shape, what its
  default does, or the order and meaning of its hooks (before, replace,
  after).
- **Commands**: a command's id, its arguments, whether agents may run
  it.
- **Strings and ids**: egui-pop's Fluent keys, the keys derived from
  ids (`command-<id>.title`...), the rule that a plugin's catalogue
  carries its id, and the syntax of typed ids.
- **The debug bridge**: collector names and snapshot shapes agents read.

Adding is not breaking: a token, a step, a command, an optional
argument, a built-in theme that meets the contrast pairs, a collector.
Pixel output is not part of the contract: drawings improve, snapshots
are refreshed.

## Deprecation

- A stable item to be removed is first **deprecated** in a minor: the
  Rust item carries `#[deprecated]` naming its replacement, and the
  changelog's `Deprecated` section says how to move. It is removed no
  earlier than the **next minor**, so an extension always has one full
  egui cycle in which both the old and the new work.
- For what is not Rust, the old name keeps working through that same
  minor: a renamed token stays as an alias of the new one, a renamed
  command or step keeps its old id forwarding to the new one, a renamed
  Fluent key keeps its old message.
- Supported items follow the same window unless a defect forces a
  change in a patch, said in the changelog.
- Experimental items may change in any release; the changelog says how
  to move.

## Upstream

egui-pop works around the limits of egui and its other dependencies,
files them upstream and removes the workaround once upstream fixes
them. The process and the register are in
[docs/upstream.md](docs/upstream.md); it is reviewed at every egui bump.

## Extensions

Extensions are listed in [docs/extensions.md](docs/extensions.md) in
three tiers (official, community, featured) with a promotion path and
the checks an entry passes. The [extension template](templates/extension)
is built by every commit's gate, so it always matches the current API.
Rules for extension authors are in
[CONTRIBUTING.md](CONTRIBUTING.md#extensions).
