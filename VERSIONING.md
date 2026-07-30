# Versioning & compatibility

## Current status: pre-1.0 (`0.x`) — unstable

zzop is pre-1.0. Every `0.x` release — **minor or patch** — may change analysis behavior,
output shape, the rule set, CLI flags, config keys, or defaults, without prior notice or a
migration path. There are no backward-compatibility guarantees yet, and there is
deliberately **no `CHANGELOG.md`** during the `0.x` series (see below).

If you depend on zzop, **pin an exact version** and re-test before upgrading. Both binaries are versioned
by the same release tag, so pin by tag: take the assets for the tag you want from [GitHub
Releases](https://github.com/eezz4/zzop/releases) rather than tracking a "latest" link (the install lanes
themselves are listed once in the [README's Quick start](README.md#quick-start)). The
Claude Code plugin pins the same way, via its own `version` field in
[`.claude-plugin/plugin.json`](.claude-plugin/plugin.json) — bump/reinstall a specific plugin version
instead of always taking the marketplace's newest.

## Breaking in the current 0.x: suppress markers gained a `zzop-` prefix, and 30 rule ids were renamed

This is the one 0.x break written down here rather than left to the release notes, because it takes
away something that was already working in user code and does it in two places at once. It is not the
start of a changelog and sets no precedent — the paragraph above still holds for every other 0.x change.

**1. Every DSL suppress marker is now spelled `zzop-<rule id>-ok`.** The old spelling was
`<rule id>-ok`. The marker is DERIVED from the rule id at runtime (`RuleDef::suppress_marker()`), never
authored or stored, so there is no per-rule exception and no config to flip:

```diff
- const items = list.map(x => db.find(x.id)); // nplus1-ok: batched below, false positive
+ const items = list.map(x => db.find(x.id)); // zzop-nplus1-ok: batched below, false positive
```

Because the marker is derived, **renaming a rule renames its marker too** — a rule in the first table
below needs both edits at once (`// auth-gates-ok` → `// zzop-protected-path-no-auth-evidence-ok`). The
marker is never written down anywhere you can look it up per rule; it is `zzop-` + the id + `-ok`, so a
renamed id silently invalidates every comment written against the old one. `zzop explain <new-id>` prints
the marker to write, and the second table below (the native/schema renames) needs no source edit at all,
because those analyses honor no derived marker.

The two comment-driven channels the NATIVE analyses read are NOT affected, because neither is derived
from a rule id: the hand-authored `// idempotent-ok: <reason>` read by
`non-idempotent-write`/`unsafe-read-endpoint`, and the generated-file banner (`@generated` and friends)
that `dead-candidates`/`unimported-export` skip a file on. Both keep their existing spelling.

An un-migrated marker does not fail silently. A comment shaped like a suppression marker but not
matching the one the rule honors is named in the finding's own message alongside the marker that would
have worked — so a stale `// nplus1-ok` turns into a finding that tells you what to write instead.
Full semantics: [`docs/rules/dsl-reference.md`](docs/rules/dsl-reference.md#suppress-marker-semantics).

**2. Renamed rule ids.** These are the ids `disabledRules` / `severityOverrides` / the `rule` argument
match on, and they are matched exactly — an old id disables nothing, remaps nothing, suppresses nothing.
It is not silent, though, and you do not have to diff the table below by hand to find one. Any run over a
config naming an unknown id says so, listing the offending ids:

- a stale `disabledRules` entry and a stale `severityOverrides` entry each produce a `configWarnings`
  line ("… matching no known rule id: … — these did NOT disable anything" / "… did NOT remap any
  finding's severity");
- a stale `suppressions` `rule` produces the same shape on `warnings` ("… these did NOT suppress
  anything").

(Computed in `crates/engine/src/analyze/diagnostics/coverage_report.rs`, rendered in
`crates/metrics/src/diagnostics.rs`.) Run once against the config, read those two arrays, and every stale
id is named. The current id set is always [`docs/rules/catalog.md`](docs/rules/catalog.md).

| pack | old id | new id |
|---|---|---|
| `browser` | `unsanitized-markdown-html` | `markdown-and-html-sink-unsanitized` |
| `db` | `client-per-request` | `client-new-in-handler` |
| `db` | `external-call-in-tx` | `external-call-and-tx` |
| `db` | `empty-catch-on-write` | `empty-catch-and-write` |
| `db` | `idempotency-key-regenerated-per-retry` | `idempotency-key-regenerated-in-loop` |
| `db` | `tx-swallows-error-commits` | `tx-and-empty-catch` |
| `db` | `tx-in-loop-long-hold` | `tx-and-db-call-in-loop` |
| `db` | `critical-write-default-isolation` | `money-tx-no-isolation-level` |
| `egress` | `get-with-body` | `get-and-body` |
| `egress` | `mixed-content-egress` | `http-url-literal` |
| `egress` | `localhost-egress-committed` | `localhost-url-literal-committed` |
| `http` | `read-model-path` | `get-route-no-cache-marker` |
| `http` | `auth-gates` | `protected-path-no-auth-evidence` |
| `http` | `route-exposure` | `dev-path-no-guard-hint` |
| `react` | `setstate-after-await-unmounted` | `setstate-after-async-unguarded` |
| `redis` | `keys-glob-scan` | `keys-command-in-code` |
| `reliability` | `await-in-map` | `map-async-no-promise-all` |
| `reliability` | `promise-all-writes` | `promise-all-and-writes` |
| `reliability` | `promise-race-resource-leak` | `promise-race-no-cancel` |
| `security` | `raw-query-interpolation` | `raw-query-unsafe-api` |
| `security` | `sql-taint` | `sql-string-concat` |
| `security` | `csp-disabled` | `csp-weak-or-disabled` |
| `typescript` | `always-false-comparison` | `always-constant-comparison` |
| `typescript` | `unhandled-promise-use-effect` | `use-effect-async-callback` |

Six more renames land on the NATIVE analysis ids, which are not DSL rules and live in no pack. They are
listed separately because the marker rule above does not reach them: **no native analysis derives a
suppress marker at all** (the two exceptions, `unsafe-read-endpoint`/`non-idempotent-write`, read a
hand-authored `// idempotent-ok:` comment that is not derived from any id and is unaffected). For these
six there is nothing to migrate in your source — only in `disabledRules` / `severityOverrides` /
`suppressions`:

| old id | new id |
|---|---|
| `dead-exports` | `unimported-export` |
| `cross-layer/shared-db-table` | `cross-layer/db-table-name-in-multiple-sources` |
| `cross-layer/external-duplicated-integration` | `cross-layer/external-host-in-multiple-sources` |
| `cross-layer/sdk-import-no-visible-consume` | `cross-layer/untraced-client-import-no-visible-consume` |
| `schema/dead-model` | `schema/unreferenced-model-name` |
| `schema/dead-field` | `schema/unreferenced-field-name` |

Each rename makes the id describe what the matcher literally checks, rather than the conclusion a
reader might draw from it — `await-in-map` never looked for `await`, `sql-taint` performs no taint
analysis, `dead-exports` reported a symbol as dead in the same breath as telling you it was referenced,
and `shared-db-table`'s own message said "not that they physically share one database". Detection
behavior is unchanged by the renames themselves.

**Additive alongside them: the 12 `schema/*` ids became real ids.** `schema` findings have always spelled
`ruleId` as `schema/<issue>` (`schema/god-model`, `schema/fk-no-index`, ...), but only the two family gates
`schema-structural` / `schema-usage` were registered — so a `disabledRules` entry naming the id printed in
your own output disabled nothing and was reported as unknown, while a `severityOverrides` entry on the same
string was reported as unknown *and quietly honored anyway*. All 12 are registered analyses now: they
disable, remap, and suppress individually, and the two family gates keep working exactly as
before (disabling a gate still switches its whole pass off). `zzop explain` is not among those verbs —
it reads only the compiled-in DSL pack data and exits 1 on a native analysis id; `zzop contract
rule-catalog` is where these 12 carry their prose. Nothing you had configured stops working —
apart from the two ids in the table above, this only starts honoring configuration that previously did
nothing.

**3. One detection change rides the same release, and it is not a rename: `redis/counter-get-set` gained
an order gate.** The matcher used to fire on mere co-occurrence — a `.get(` read and an arithmetic
`.set(` anywhere in the same function. It now requires the arithmetic set to appear *lexically after* a
read in the *nearest enclosing* function (`after` + `after_in_same_function` in
`rules/dsl/redis/redis.json`). Two shapes that used to be reported are now silent: an arithmetic set that
**precedes** the only read in its function, and a read that lives in a **sibling closure** rather than in
the set's own function (both pinned by tests in `rules/dsl/redis/redis_counter_get_set.rs`). This
narrows detection, so a repository whose code did not change can see this rule's finding count drop on
upgrade — worth knowing if you gate CI on it. Per the `0.x` policy above (and the "explicitly NOT part of
the compatibility surface" list below), exact finding sets are not a compatibility surface, so this is
recorded here rather than versioned.

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
| Normalized AST envelope input ([`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)) | The envelope shape external parser adapters emit. Its `version` field is a RELEASE number in these same units, and moves only when the shape moves — so an adapter emitting a given version keeps being accepted through every later release that did not change the shape. A shape change is never silent: a consumer rejects a version above its own, and a field whose absence would change the analysis carries an explicit floor. |
| Rule ids | The `disabledRules` / `severityOverrides` ids you configure against. A rename is a major bump. |

## Explicitly NOT part of the compatibility surface

These change freely at any time, by design — do not build on them:

- **`PARSER_FINGERPRINT` / `CACHE_SCHEMA_VERSION`** — internal cache keys. They change
  whenever extraction output or the cache payload changes; that churn is their whole job
  (it invalidates stale cache entries). They are not a public version. `CACHE_SCHEMA_VERSION`
  additionally *derives* its leading component from the release version below, so **every
  release reclaims one cache generation**: the schema version always differs after an upgrade,
  the previous `cacheDir` contents are wiped once, and the first run after an upgrade is cold.
  That is the designed replacement for a garbage collector the cache deliberately does not have
  (see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#caching)).
- **Exact finding sets, counts, and message wording** — detection is *total by default* and
  improves continuously, so which findings a run emits (and their exact text) shifts release
  to release. Gate CI by reading the severity/rule-id counts you care about from the JSON
  output, not on an exact total finding count.
- **The Rust crates (`zzop-*`)** — internal workspace crates, not a published stable library
  API. The consumer surfaces are the `zzop` CLI binary and the `zzop-mcp` binary (MCP tools), the
  Claude Code plugin / Claude Desktop `.mcpb` bundle built from the latter, and the Normalized AST
  protocol.

## How versions are produced

The version SSOT is the workspace `Cargo.toml`'s `[workspace.package] version` (2026-07-22 reform).
Every crate inherits it via `version.workspace = true`, and both binaries report it directly as
`CARGO_PKG_VERSION` — the same value `zzop version` / `zzop-mcp version` print and the MCP `initialize`
reply's `serverInfo.version` reports. A release bumps that one number in a commit and pushes it to
`main`; an auto-tag lane picks up the bump, creates the matching `vX.Y.Z` tag, and runs the full release
from there — no separate manual `git tag`/`git push` step needed. CI's release job fails unless the tag,
`Cargo.toml`, and `.claude-plugin/plugin.json`'s `"version"` all agree, so the binaries and the Claude
Code plugin are always released in lockstep. (The old tag-stamped `ZZOP_RELEASE_VERSION` env and the
`0.0.0` placeholder are gone.) The npm packages (`@zzop/cli` and its 5 platform sub-packages) are
stamped with the same number at publish time from the release tag, verified by the same CI gate.
