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

## Breaking in the current 0.x: suppress markers gained a `zzop-` prefix, and rule ids were renamed

The one 0.x break written down here rather than left to the release notes, because it takes away
something that was already working in *your* files and does it in two places at once. It is not the
start of a changelog and sets no precedent — the paragraph above still holds for every other 0.x
change. What is kept below is what a migration needs: what changed, the id tables to map against, and
how to find a stale id without diffing those tables by hand.

**1. Every DSL suppress marker is now spelled `zzop-<rule id>-ok`** (was `<rule id>-ok`):

```diff
- const items = list.map(x => db.find(x.id)); // nplus1-ok: batched below, false positive
+ const items = list.map(x => db.find(x.id)); // zzop-nplus1-ok: batched below, false positive
```

The marker is DERIVED from the rule id at runtime, never authored or stored, so **renaming a rule
renames its marker too** — a rule in the table below needs both edits at once (`// auth-gates-ok` →
`// zzop-protected-path-no-auth-evidence-ok`). `zzop explain <rule id>` prints the marker to write.
Full semantics: [`docs/rules/dsl-reference.md`](docs/rules/dsl-reference.md#suppress-marker-semantics).

The two comment-driven channels the NATIVE analyses read are unaffected, because neither is derived
from a rule id: the hand-authored `// idempotent-ok: <reason>` and the generated-file banner. Both
keep their existing spelling.

An un-migrated marker does not fail silently: a comment shaped like a suppression marker but not
matching the one the rule honors is named in the finding's own message alongside the marker that
would have worked.

**2. Renamed rule ids.** These are the ids `disabledRules` / `severityOverrides` / `suppressions`
match on, and they are matched exactly — an old id disables nothing and suppresses nothing. You do
not have to diff the tables by hand to find one: a config naming an unknown id says so, listing the
offending ids on `configWarnings` (`disabledRules`, `severityOverrides`) and `warnings`
(`suppressions`). Run once against your config and read those two arrays. The current id set is
always [`docs/rules/catalog.md`](docs/rules/catalog.md).

⚠ `http/get-route-no-cache-marker` (renamed from `read-model-path`) was later REMOVED entirely and
has **no replacement id** — a config naming either spelling gets the same unknown-rule-id warning.

† The two `typescript` rows are a different case, and the difference matters: those ids were not
removed, the whole `typescript` pack MOVED OUT OF THE BUNDLE on 2026-08-11 (it is now
[`examples/packs/typescript-lint.json`](examples/packs/typescript-lint.json), shipped in the repository
but not loaded by default). The new ids are still the right ids and the rule still fires — but only in a
run that loads that pack, so on a default run a config naming one gets the unknown-rule-id warning,
whose text suggests checking for a typo. It is not a typo — load the pack. How to point a run at a
repository pack is [`examples/packs/README.md`](examples/packs/README.md)'s to say, and it is not
restated here.

‡ The three `orm-eager` rows are the same case as `typescript` above with one difference worth stating:
the pack did NOT move whole. `perf` still exists and still ships `api-in-loop`; only the three rules that
declare `"axis": "opinion"` left it, into
[`examples/packs/orm-eager.json`](examples/packs/orm-eager.json). That is why these are RENAMES and the
`typescript` ones were not — a rule id is `<pack>/<rule>`, so a rule that leaves one pack for another is
renamed by construction, while a whole pack takes its id with it.

**Your suppress markers are unaffected.** The marker is derived from the BARE rule id
(`zzop-jpa-eager-fetch-ok`), not the pack-qualified one, so every marker already in your code keeps
suppressing after the move. What breaks is a `disabledRules` / `severityOverrides` / `rules` entry naming
the old full id: that gets the unknown-rule-id warning, and the fix is the new id plus loading the pack.

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
| `http` | `read-model-path` | `get-route-no-cache-marker` |
| `http` | `auth-gates` | `protected-path-no-auth-evidence` |
| `http` | `route-exposure` | `dev-path-no-guard-hint` |
| `react` | `setstate-after-await-unmounted` | `setstate-after-async-unguarded` |
| `redis` | `keys-glob-scan` | `keys-command-in-code` |
| `reliability` | `await-in-map` | `map-async-no-promise-all` |
| `reliability` | `promise-all-writes` | `promise-all-and-writes` |
| `security` | `raw-query-interpolation` | `raw-query-unsafe-api` |
| `security` | `sql-taint` | `sql-string-concat` |
| `security` | `csp-disabled` | `csp-weak-or-disabled` |
| `typescript` † | `always-false-comparison` | `always-constant-comparison` |
| `typescript` † | `unhandled-promise-use-effect` | `use-effect-async-callback` |

**Cross-pack moves** need their own table, because the two columns above assume one pack: a rule that
leaves `perf` for `orm-eager` has a different pack on each side, so both sides are spelled in full.

| old id | new id |
|---|---|
| `perf/eager-relation-declared` | `orm-eager/eager-relation-declared` |
| `perf/jpa-eager-fetch` | `orm-eager/jpa-eager-fetch` |
| `perf/sqlalchemy-eager-relationship` | `orm-eager/sqlalchemy-eager-relationship` |
| `sql/query-logic-density` | `sql-preferences/query-logic-density` |
| `sql/app-side-aggregation-reduce` | `sql-preferences/app-side-aggregation-reduce` |
| `sql/app-side-aggregation-filter-length` | `sql-preferences/app-side-aggregation-filter-length` |
| `sql/select-star` | `sql-preferences/select-star` |
| `sql/like-leading-wildcard` | `sql-preferences/like-leading-wildcard` |
| `browser/no-system-dialogs` | `code-hygiene/no-system-dialogs` |
| `egress/localhost-url-literal-committed` ¶ | `code-hygiene/localhost-url-literal-committed` |
| `reliability/env-nonnull-assert` | `code-hygiene/env-nonnull-assert` |
| `reliability/process-exit-in-lib` | `code-hygiene/process-exit-in-lib` |
| `reliability/console-in-be` | `code-hygiene/console-in-be` |
| `reliability/console-in-loop` | `code-hygiene/console-in-loop` |
| `reliability/env-outside-config` | `code-hygiene/env-outside-config` |
| `reliability/promise-race-no-cancel` | `code-hygiene/promise-race-no-cancel` |
| `egress/localhost-egress-committed` ¶¶ | `code-hygiene/localhost-url-literal-committed` |
| `reliability/promise-race-resource-leak` | `code-hygiene/promise-race-no-cancel` |

The five `sql-preferences` rows (2026-08-12) are the same shape as the `orm-eager` ones: `sql` did not
move, it shed rules that declare `"axis": "opinion"` and still ships eight, so each of the five is
renamed by construction. The pack is
[`examples/packs/sql-preferences.json`](examples/packs/sql-preferences.json).

A SIXTH rule, `sql/destructive-migration`, was exported the same day and returned the same day, and this
table deliberately carries no row for it. A rename row exists to migrate a user, and no user could have
been carrying the exported spelling: `sql/destructive-migration` is the id `v0.29.0` shipped, and no tag
was cut in between — `git tag --list` still tops out at `v0.29.0`, so both the exporting commit and this
one describe as `v0.29.0-<n>`, differing only in how far past that tag they sit. Listing a
round trip nobody could observe would send readers to `disabledRules`/`suppressions` entries that never
existed. The window this reasoning depends on closes the moment a tag lands — after that, a rename is
shipped whether or not it is later undone, and BOTH hops belong here as the `¶¶` rows are spelled.
Bundling it again also makes it the one bundled rule declaring `"axis": "opinion"`, so the
`grep -c` count below is 0 everywhere except `rules/dsl/sql/sql.json`, where it is 1.

The eight `code-hygiene` rows (2026-08-12) close the `axis: opinion` export — after them exactly ONE
bundled rule declares that axis (`grep -c '"axis": "opinion"' rules/dsl/*/*.json` → 0 everywhere except
`rules/dsl/sql/sql.json`, which is 1: the returned `destructive-migration` above). Unlike the
two increments before it, this one draws from THREE packs at once; all three (`browser`, `egress`,
`reliability`) stay bundled and keep shipping their defect rules, so every one of the eight is renamed
by construction rather than carried along by a pack that moved. The pack is
[`examples/packs/code-hygiene.json`](examples/packs/code-hygiene.json).

¶¶ The last two rows are TWO-HOP and are here rather than in the same-pack table above for that reason.
Both were ordinary within-pack renames first (`egress/localhost-egress-committed` →
`localhost-url-literal-committed`; `reliability/promise-race-resource-leak` → `promise-race-no-cancel`),
and then the renamed rule left its pack in the 2026-08-12 export. A reader migrating from the ORIGINAL
id needs the destination that exists today, not the intermediate one — so each row spells the whole
journey in one hop. That is also what keeps them checkable: `scripts/check-prose-rule-ids.sh` resolves
every rename table's target against what this repo ships, and the intermediate spellings resolve to
nothing.

¶ `localhost-url-literal-committed` is this increment's row with a behavioural consequence beyond the
id — the same shape that sent `destructive-migration` back to the bundle, and the reason that one is
worth reading beside this one: there, the deferring siblings were CRITICAL rules whose exclusion of
migration paths only made sense while the disclosure ran, and a real destructive statement went
unreported in the dogfood corpus as a result. Here the deferring rules are themselves opinion-adjacent
and no defect goes unreported, which is why this export stands and that one did not. TWO rules that stayed
decline the localhost/private-IP case, each on its own scope grounds rather than on a premise about this
one: `egress/http-url-literal` by an `exclude_pattern` naming loopback and private-range hosts outright
(the exact regex is not restated here — `zzop explain egress/http-url-literal` prints it, and which
addresses it covers is a question about the rule you are running, not about this release), and the native
`cross-layer/external-ip-literal` by measuring whether a literal PINS AN ENVIRONMENT, which loopback does
not. Both still ship and still decline — so on a default run a committed `http://localhost:3000` or
`https://127.0.0.1/...` is now reported by nothing until you load `code-hygiene`. That is deliberate
(whether a committed dev URL is wrong is a fact about the project, not about the line), but it is a real
change in what a default run says about committed dev URLs, not just a change of id. The routing is
pinned from both sides in `examples/packs/tests/egress_handoff.rs` and
`rules/dsl/egress/http_shapes.rs`; why neither narrowing may name this rule by id is owned by
[`examples/packs/README.md`](examples/packs/README.md), not restated here.

Seven more renames land on the NATIVE analysis ids, which are not DSL rules and live in no pack. They
are listed separately because the marker rule above does not reach them: **no native analysis derives
a suppress marker at all**, so for these there is nothing to migrate in your source — only in
`disabledRules` / `severityOverrides` / `suppressions`.

| old id | new id |
|---|---|
| `dead-exports` | `unimported-export` |
| `cross-layer/shared-db-table` | `cross-layer/db-table-name-in-multiple-sources` |
| `cross-layer/external-duplicated-integration` | `cross-layer/external-host-in-multiple-sources` |
| `cross-layer/sdk-import-no-visible-consume` | `cross-layer/untraced-client-import-no-visible-consume` |
| `schema/dead-model` | `schema/unreferenced-model-name` |
| `schema/dead-field` | `schema/unreferenced-field-name` |
| `schema/schema-churn` | `schema/model-churn` |

**Additive alongside them: the 12 `schema/*` ids became real ids.** `schema` findings always carried
`ruleId` as `schema/<issue>`, but only the two family gates were registered — so a `disabledRules`
entry naming the id you read in your own output disabled nothing, and a `severityOverrides` entry for
it was reported as unknown *and quietly honored anyway*. All 12 are registered analyses now, so they
disable, remap, and suppress individually; the two family gates keep working as before. Nothing you
had configured stops working.

**3. One detection change rides the same release and is not a rename: `redis/counter-get-set` gained
an order gate.** It used to fire on mere co-occurrence; it now requires the arithmetic `.set(` to
appear lexically after the read in the nearest enclosing function. Two shapes that used to be
reported are now silent (a set that precedes the only read, and a read in a sibling closure), so a
repository whose code did not change can see this rule's finding count drop on upgrade — worth
knowing if you gate CI on it. Exact finding sets are not a compatibility surface (see the list
below), so this is recorded here rather than versioned.

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
  (it invalidates stale cache entries). They are not a public version, and neither one carries
  the release version. Disk is bounded instead by a size cap that evicts the oldest entries, so the
  directory no longer grows without limit either (see
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#caching)).
  **Which releases this holds for:** **v0.30.0 onward.** The key is a hash of the sources that produce
  the cached bytes, so **an upgrade that changed nothing about what gets extracted or stored leaves your
  warm cache warm.** Up to and including v0.29.1 the schema version led with the release number instead,
  so *every* upgrade wiped `cacheDir` whether or not anything about the analysis had moved (`v0.29.0 ->
  v0.29.1` touched zero bytes of the hashed closure and wiped every cache anyway) — that is what a reader
  on an older pin will still see. This boundary is a released fact, not a promise about the next tag; it
  was written in the future tense until 2026-08-11, which made it read as unshipped for a whole release.
- **Exact finding sets, counts, and message wording** — detection is *total by default* and
  improves continuously, so which findings a run emits (and their exact text) shifts release
  to release. Gate CI by reading the severity/rule-id counts you care about from the JSON
  output, not on an exact total finding count.
- **The Rust crates (`zzop-*`)** — internal workspace crates, not a published stable library
  API. The consumer surfaces are the `zzop` CLI binary and the `zzop-mcp` binary (MCP tools), the
  Claude Code plugin / Claude Desktop `.mcpb` bundle built from the latter, and the Normalized AST
  protocol.

## Documents that are compiled INTO the binary — changing one requires a release

These files are not read from disk at run time. They are `include_str!`-baked into the binary by
`crates/summary/src/contracts.rs` and served as the `zzop contract` / `zzop://contract/*` resources,
so **a reader on a prebuilt binary sees the bytes from the release they installed**, never the ones
in this repository.

The consequence is a rule, not a nuance: *"only documentation changed, so no release is needed"* is
**false for every file in this list**. A pack author on the previous release cannot see a knob, a
schema field, or a rule row added here until a new binary ships.

<!-- EMBEDDED-CONTRACT-DOCS: this list is machine-checked against the `include_str!` calls in
     crates/summary/src/contracts.rs by scripts/check-embedded-contract-docs.sh. Adding a baked
     document without listing it here (or the reverse) fails that guard. -->

- `docs/adapters/envelope.schema.json`
- `docs/NORMALIZED_AST.md`
- `docs/adapters/key-normalization.fixture.json`
- `docs/adapters/README.md`
- `docs/rules/dsl-reference.md`
- `docs/rules/authoring-guide.md`
- `docs/contracts/rule-pack.schema.json`
- `docs/contracts/example-envelope.json`
- `docs/rules/catalog.md`
- `examples/packs/typescript-lint.json`
- `examples/packs/orm-eager.json`
- `examples/packs/sql-preferences.json`
- `examples/packs/code-hygiene.json`

Every other file under `docs/` is read from the repository or the website and needs no release to
reach a reader — the distinction is exactly "is it in the list above".

The `examples/packs/*.json` entries are a different KIND of document and are listed for the same reason. An exported rule
pack is shipped in the repository but not bundled, so its rules run only when a config points at
them — and until 2026-08-12 no shipped artifact carried the file at all, which made "copy the pack
back" an instruction whose first step needed a source checkout. It is baked now, served as the
`example-pack-<stem>` contract resource, and the retrieval path is: print the resource, save it as
`<stem>.json` under `<tree>/zzop/rules/`, done. Unlike the `docs/` entries above, these rows reach the
table through a build script (`crates/config/build.rs`) rather than an `include_str!` in
`contracts.rs`, which is why the guard reads `git ls-files 'examples/packs/*.json'` for this half.
## How versions are produced

The version SSOT is the workspace `Cargo.toml`'s `[workspace.package] version` (2026-07-22 reform).
Every crate inherits it via `version.workspace = true`, and both binaries report it directly as
`CARGO_PKG_VERSION` — the same value `zzop version` / `zzop-mcp version` print and the MCP `initialize`
reply's `serverInfo.version` reports. A release bumps that one number in a commit and pushes it to
`main`; an auto-tag lane picks up the bump, creates the matching `vX.Y.Z` tag, and runs the full release
from there — a manual `git tag`/`git push` of a `v*` tag is not merely unneeded but FORBIDDEN since
2026-08-09 (the `meta` job rejects a hand-pushed tag with an error and releases nothing). CI's release job fails unless the tag,
`Cargo.toml`, and `.claude-plugin/plugin.json`'s `"version"` all agree, so the binaries and the Claude
Code plugin are always released in lockstep. (The old tag-stamped `ZZOP_RELEASE_VERSION` env is gone;
the `0.0.0` placeholder is gone *from the Rust workspace*. The npm and `.mcpb` manifests under
`packages/` deliberately keep `"version": "0.0.0"` in-tree — they are rewritten at publish time from
the release tag, and CONTRIBUTING's version-propagation guard exempts them for exactly that reason, so
a committed real number there would be the stale copy.) The npm packages (`@zzop/cli` and its 5 platform sub-packages) are
stamped with the same number at publish time from the release tag, verified by the same CI gate.

