# Versioning & compatibility

## Current status: pre-1.0 (`0.x`) — unstable

zzop is pre-1.0. Every `0.x` release — **minor or patch** — may change analysis behavior,
output shape, the rule set, CLI flags, config keys, or defaults, without prior notice or a
migration path. There are no backward-compatibility guarantees yet, and there is
deliberately **no `CHANGELOG.md`** during the `0.x` series (see below).

If you depend on zzop, **pin an exact version** and re-test before upgrading. Both binaries are versioned
by the same release tag: download the exact `zzop-<platform>[.exe]` and/or `zzop-mcp-<platform>[.exe]`
asset for the tag you want from [GitHub Releases](https://github.com/eezz4/zzop/releases) rather than
tracking a "latest" link. The
Claude Code plugin pins the same way, via its own `version` field in
[`.claude-plugin/plugin.json`](.claude-plugin/plugin.json) — bump/reinstall a specific plugin version
instead of always taking the marketplace's newest.

## What 1.0.0 will mark

`1.0.0` is the line where zzop starts making promises:

- **Semantic Versioning** takes effect (see the surface below).
- A maintained **`CHANGELOG.md`** begins, documenting every release from `1.0.0` onward.
  The `0.x` history is intentionally not reconstructed — it was pre-stable.

Until then, the git tag list and the GitHub release notes (auto-generated per tag) are the
record of what shipped.

## The compatibility surface (from 1.0.0)

Under Semantic Versioning, from `1.0.0`:

- **MAJOR** — a breaking change to any surface below.
- **MINOR** — additive: new rules, new analyses, new **additive** output fields, new opt-in
  config.
- **PATCH** — bug fixes and precision improvements that do not change the contract.

The surfaces SemVer will cover:

| Surface | What's covered |
|---|---|
| SDK / CLI JSON output (`analyze` / `analyzeTrees` / `analyzeEnvelope`) | Field names and types. New fields are added (minor); existing fields are not removed or repurposed without a major bump. |
| CLI flags & config keys | Removing or repurposing a flag/key is a major bump; adding one is minor. Unknown keys are ignored with a warning, never a hard error. |
| Normalized AST envelope input ([`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)) | The frozen `v1` contract external parser adapters emit. A new contract version would be additive, never a silent change to `v1`. |
| Rule ids | The `disabledRules` / `severityOverrides` ids you configure against. A rename is a major bump. |

## Explicitly NOT part of the compatibility surface

These change freely at any time, by design — do not build on them:

- **`PARSER_FINGERPRINT` / `CACHE_SCHEMA_VERSION`** — internal cache keys. They change
  whenever extraction output or the cache payload changes; that churn is their whole job
  (it invalidates stale cache entries). They are not a public version.
- **Exact finding sets, counts, and message wording** — detection is *total by default* and
  improves continuously, so which findings a run emits (and their exact text) shifts release
  to release. Gate CI by reading the severity/rule-id counts you care about from the JSON
  output, not on an exact total finding count.
- **The Rust crates (`zzop-*`)** — internal workspace crates, not a published stable library
  API. The consumer surfaces are the `zzop-mcp` binary (CLI subcommands + MCP tools), the Claude
  Code plugin / Claude Desktop `.mcpb` bundle built from it, and the Normalized AST protocol.

## How versions are produced

The version SSOT is the workspace `Cargo.toml`'s `[workspace.package] version` (2026-07-22 reform).
Every crate inherits it via `version.workspace = true`, and both binaries report it directly as
`CARGO_PKG_VERSION` — the same value `zzop version` / `zzop-mcp version` print and the MCP `initialize`
reply's `serverInfo.version` reports. A release bumps that one number in a commit, then tags it
(`git tag vX.Y.Z && git push origin vX.Y.Z`); CI's release job fails unless the pushed tag,
`Cargo.toml`, and `.claude-plugin/plugin.json`'s `"version"` all agree, so the binaries and the Claude
Code plugin are always released in lockstep. (The old tag-stamped `ZZOP_RELEASE_VERSION` env and the
`0.0.0` placeholder are gone.)
