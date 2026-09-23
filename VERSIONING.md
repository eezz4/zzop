# Versioning & compatibility

## Current status: pre-1.0 (`0.x`) — unstable

zzop is pre-1.0. Every `0.x` release — **minor or patch** — may change analysis behavior,
output shape, the rule set, CLI flags, config keys, or defaults. Semantic Versioning is not in
effect yet, so the version number alone does not tell you whether an upgrade breaks you, and there
are no backward-compatibility guarantees yet.

What the `0.x` license does **not** include is silence. A break to one of the surfaces in
[The compatibility surface](#the-compatibility-surface) is written down — old and new spelling both,
in [`CHANGELOG.md`](CHANGELOG.md) — instead of being left for you to discover. That is a promise
about the RECORD, not about the rate: it narrows the paragraph above by exactly nothing. It is
scoped that narrowly on purpose — the section directly below is the same promise already being kept
for the one `0.x` break big enough to need it, and a wider promise made here would be a claim rather
than a description.

If you depend on zzop, **pin an exact version** and re-test before upgrading. Both binaries are versioned
by the same release tag, so pin by tag: take the assets for the tag you want from [GitHub
Releases](https://github.com/eezz4/zzop/releases) rather than tracking a "latest" link (the install lanes
themselves are listed once in the [README's Quick start](README.md#quick-start)). The
Claude Code plugin pins the same way, via its own `version` field in
[`.claude-plugin/plugin.json`](.claude-plugin/plugin.json) — bump/reinstall a specific plugin version
instead of always taking the marketplace's newest.

## Breaking in the current 0.x: suppress markers gained a `zzop-` prefix, and rule ids were renamed

The one 0.x break written down here rather than left to the release notes, because it takes away
something that was already working in *your* files and does it in two places at once. It sits here
rather than in [`CHANGELOG.md`](CHANGELOG.md) because what it carries is a MIGRATION — id tables to
map against, and how to find a stale id without diffing them by hand — which is a thing you read
once and to the end, not a row per release. It sets no precedent for the rest of a `0.x` release:
the top paragraph still holds for every change outside the surfaces the compatibility table names.

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
the pack did NOT move whole. `perf` outlived the export and went on shipping `api-in-loop` (until the
2026-09-03 merge took that rule to `reliability` and the pack with it); only the three rules that
declare `"axis": "opinion"` left it in 2026-08-12, into
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
| `go/goroutine-in-loop` | `reliability/goroutine-in-loop` |
| `perf/api-in-loop` | `reliability/api-in-loop` |
| `react/setstate-after-async-unguarded` | `reliability/setstate-after-async-unguarded` |
| `react/setstate-after-await-unmounted` | `reliability/setstate-after-async-unguarded` |

The last four rows (2026-09-03) are the single-rule-pack merge: `go`, `perf` and `react` each shipped
exactly ONE rule, so each pack name was a second spelling of a rule id that a user had to learn, keep
in `packs.only`, and find a catalog section for. All three rules moved into `reliability` unchanged —
11 bundled packs became 8 while the DSL rule count stayed at 118, which is the arithmetic that says
nothing was dropped on the way. **`packs.only: ["go"]`, `["perf"]` or `["react"]` now names no loaded
pack**, and a `packs.only` that resolves to nothing disables every DSL rule; the run warns, but the
warning is the only signal, so those three words are worth grepping for in a config that predates this
release.

The FOURTH of those rows collapses two hops. `setstate-after-await-unmounted` was renamed inside the
old `react` pack before that pack was merged, so a user still carrying the oldest spelling would
otherwise have to walk the same-pack table to a `react/` id that no longer resolves and then walk this
one — and the same-pack table above no longer carries that row for exactly that reason. The `··` rows
were written to the same rule.
The five `sql-preferences` rows (2026-08-12) are the same shape as the `orm-eager` ones: `sql` did not
move, it shed rules that declare `"axis": "opinion"` and still ships eight, so each of the five is
renamed by construction. The pack is
[`examples/packs/sql-preferences.json`](examples/packs/sql-preferences.json).

A SIXTH rule, `sql/destructive-migration`, was exported the same day and returned the same day, and this
table deliberately carries no row for it. A rename row exists to migrate a user, and no user could have
been carrying the exported spelling: `sql/destructive-migration` is the id `v0.30.0` shipped, and no tag
was cut between the exporting commit (`9a49080`) and the returning one (`c0cc8ed`) — at both hops the
remote tag list topped out at `v0.30.0` (`e7b20da`), an ANCESTOR of both, and the next tag (`v0.31.0`)
landed only after the round trip had completed — so both hops describe as `v0.30.0-<n>`, differing only
in how far past that tag they sit. Listing a
round trip nobody could observe would send readers to `disabledRules`/`suppressions` entries that never
existed. The window this reasoning depends on closes the moment a tag lands — after that, a rename is
shipped whether or not it is later undone, and BOTH hops belong here as the `¶¶` rows are spelled.
Bundling it again also makes it the one bundled rule declaring `"axis": "opinion"`, so the
`grep -c` count below is 0 everywhere except `rules/dsl/sql/sql.json`, where it is 1.

Read that tag coordinate off the REMOTE — `git ls-remote --tags origin` — and never off a local
`git tag --list`. Releases are cut by CI on the remote, so a clone that has never fetched tags reports a
top tag some number of releases stale and every `git describe` taken in it is anchored to the wrong
release. This paragraph said `v0.29.0` until 2026-08-14 for exactly that reason: the local list was two
tags behind (it had neither `v0.29.1` nor `v0.30.0`). The argument survived the correction — the tag
that was missing sits BEFORE both hops, not between them — but the numbers and the witness did not.

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

`1.0.0` is the line where the promise stops being about whether you hear about a break and starts
being about **when one may land at all**:

- **Semantic Versioning** takes effect (see the surface below). A break to a covered surface stops
  being something a minor or patch release may do, and becomes a MAJOR bump.

[`CHANGELOG.md`](CHANGELOG.md) does not wait for that line — it is here now. It indexes every
released tag rather than reconstructing the `0.x` series as prose, because the releases themselves
are the record and a retold one would be a second copy of it. The GitHub release notes
(auto-generated per tag) stay the detail behind each of its rows.

## The compatibility surface

This table is the scope of **both** promises: what a break is recorded against today, and what
Semantic Versioning will cover from `1.0.0`. Nothing outside it is in scope on either side. The
scope is deliberately narrow — it holds only what has already been held in practice. Widening it
later adds a promise; narrowing it breaks one.

Under Semantic Versioning, from `1.0.0`:

- **MAJOR** — a breaking change to any surface below.
- **MINOR** — additive: new rules, new analyses, new **additive** output fields, new opt-in
  config.
- **PATCH** — bug fixes and precision improvements that do not change the contract.

The surfaces:

| Surface | What's covered |
|---|---|
| CLI JSON output — the reply shapes of `zzop analyze`, `zzop cross` and `zzop analyze-envelope` (described in [docs/modules/facade.md](docs/modules/facade.md) under the names `analyze` / `analyzeTrees` / `analyzeEnvelope`; **those three are the facade's own function names, not a surface** — the exclusion list below removes the Rust crates, so a row that identified this promise by them named its scope in vocabulary it had already disclaimed) | Field names and types. New fields are added (minor); existing fields are not removed or repurposed without a major bump. ⚠ That document describes TWO shapes and only one of them is this surface: the **shaped summary** every CLI subcommand and MCP tool returns, not the raw facade output beside it. `ir`, `nodes` and `scores` are documented there and are never in a CLI reply — a consumer pinning those is pinning a shape this surface does not carry, and the exclusion list below does not mention them because they are not omitted from a promise, they were never in one. |
| CLI flags & config keys | Removing or repurposing a flag/key is a major bump; adding one is minor. A key this release does not know is ignored with a warning, never a hard error — that covers both keys it never had and RETIRED keys, ones that stopped meaning anything. ⚠ **Two moved-key outcomes, and which one you get follows what the stale value would silently do.** A moved key is one whose value still means something, so dropping it with a line of text over it does not fail loudly later. **⑴ Refused, exit 1, naming both spellings** — when the stale value would mis-key the cross-tree join and then report confidently. That is the deployment-topology class, and its roster is `crates/config/src/mapper/topology.rs`; the refusal itself tells you the new path, so no list is kept here. **⑵ Reported at exit 0, naming both spellings, value not honoured** — when the stale value would move a SCORE rather than a finding or a join. Today that is `scores.excludeTestFilesFromPopulation` → `scores.excludeTestFilesFromFileMetrics` (`crates/facade/src/config/declared.rs`). It is reported rather than dropped for the reason that whole branch exists: serde discards an unknown field without a word, so a config carried across that rename would otherwise change its own `pain` with nothing said. ⚠ **This second outcome was undocumented here until 2026-09-23**, while this row stated ⑴ as the whole rule — read together with the branch that does ⑵, the two halves of one release said opposite things about the same key class (pinned by `the_old_flat_topology_keys_are_refused_and_name_their_new_home`). Through 0.x a moved key has no grace period at all; from 1.0 it owes one, and this row moves with that. |
| Normalized AST envelope input ([`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)) | The envelope shape external parser adapters emit. Its `version` field is a RELEASE number in these same units, and moves only when the shape moves — so an adapter emitting a given version keeps being accepted through every later release that did not change the shape. A shape change is never silent: a consumer rejects a version above its own, and a field whose absence would change the analysis carries an explicit floor. |
| Rule ids | The `disabledRules` / `severityOverrides` ids you configure against. A rename is a major bump. |
| CLI exit codes `0` / `1` / `2` | Whether the CALL was right: `0` answered, `1` zzop could not answer, `2` the invocation was refused. Narrowing what earns a refusal is recorded; changing what a code MEANS is a major bump. `--fail-on`'s `3` is deliberately NOT here — see the exclusions below. |

### Inside those surfaces, six things are still moving

Being in the table means a change to it is **recorded**, not that it will not happen. Six
properties inside it are known to be unsettled today. Each is named here rather than left for you to
infer from a run, because a consumer that assumes the settled version of any of them is building on
something this project has said out loud it is not holding.

**They are not the same kind of unsettled, and `1.0.0` does not treat them alike.** Only the first
names something the freeze has to *stop*: a rule id is a surface in the table above, so from
`1.0.0` every rename is a MAJOR bump, which means the renaming has to be finished before the
promise starts rather than after it. The other five are unsettled by decision and stay that way,
and what `1.0.0` freezes is the *declaration* that they move — a severity band and a shipped-off
default are field VALUES, not the field names and types the table covers; `SourceSymbol.id`'s
non-uniqueness IS the contract, and a disambiguating spelling added later would be additive; and
the three native id namespaces are explicitly not being normalized, so the mixed convention is the
settled state rather than a stop on the way to one; and the keys inside `findings[].data` are unpromised
because no machine could hold that promise, which makes not making it the settled state too. Read as a single bucket, this list says
something it does not mean: that `1.0.0` cannot be declared while any of the six is moving.

- **Rule ids are still being renamed, and one defect concept is often several ids.** The same
  concept is frequently spelled once per language rather than once, and folding those spellings into
  a single id was considered and declined — so the plural stays, and individual ids keep moving as
  their matchers sharpen. Every rename that *reached a release* lands as a row: the pre-`1.0.0` ones
  are the tables in *Breaking in the current `0.x`* above, and each release's are in
  [`CHANGELOG.md`](CHANGELOG.md). The qualifier is load-bearing and that section names its one
  exception — a rename that was applied and undone between tags carries no row, because a row exists
  to migrate a user and no user could have been carrying the intermediate spelling.
  Configure against [`docs/rules/catalog.md`](docs/rules/catalog.md),
  which is always the current set, and let a run's `configWarnings` / `warnings` tell you when an id
  you named no longer exists.
- **A rule's SEVERITY is not a settled property, and one rule of judgment is currently moving many
  of them at once.** The band is meant to say how strong the rule's own evidence is, and the rule
  applied since 2026-08-25 is that a rule which SPELLS OUT a disqualifying condition it cannot detect
  is describing a gate it did not build — so the band, not the prose, is where that fact belongs. On
  2026-09-03 that rule was applied to the whole class rather than to the one rule it was written for:
  **27 rules moved `warning` → `info`** because each already said in its own message that its two
  matched halves are independent and nothing links them. **This changes what `--fail-on warning`
  breaks on**, and for a security-heavy config it changes it a lot — 12 of the 27 are `security/`
  rules, `taint-flow` and the command-injection and path-traversal families among them. Nothing was
  removed and no finding count moved: the same findings report, under a band that now states the
  evidence rather than the topic. If your gate depended on those rules failing a build, gate on
  `--fail-on info`, or re-raise the ones you trust with `severityOverrides`. The band moves back up
  per rule as the missing gate is actually built.
  **A second judgment moved six more on 2026-09-06, and this one is about what a band is FOR.** A rule
  that reports a shape the reader may legitimately intend — an import cycle, a wide model, an optional
  foreign key, one external host called from two trees — is not making a claim that can be wrong, so it
  does not belong in a band that asks for action: `circular`, `schema/god-model`, `schema/nullable-fk`,
  `schema/model-churn`, `cross-layer/db-table-name-in-multiple-sources` and
  `cross-layer/external-host-in-multiple-sources` moved `warning` → `info`. Each one's own message
  already conceded it. Same consequence as the row above and the same remedies: no finding is removed,
  no count moves, `--fail-on warning` stops breaking on these six. `schema/model-churn` also lost its
  `critical` tier — its escalation line is one the rule's message calls unmeasurable.

- **Whether a rule runs BY DEFAULT is not a settled property either, and three analyses now ship
  off.** `unimported-export`, `dead-candidates` and `unreachable` are unused-code hygiene rather than
  defect claims, and across the dogfood corpus they were **61.7% of every finding a run reported** —
  a first run whose top half is hygiene buries the findings that claim a defect. Since 2026-09-03 the
  ENGINE ships them off; before that the same opinion sat as three `"off"` lines in the starter
  config, which reached only trees created after it landed. **A run that wants one must now name it**
  — `"dead-candidates": "info"` in `rules`, the same gesture that changes any other rule's band, so
  there is no opt-in vocabulary to learn. Nothing is removed from the build, and every reply lists
  what it skipped in `nativeAnalyses.shippedOff` — kept separate from `nativeAnalyses.disabled`
  precisely so a project default never reads as your decision. A library caller constructing an
  `EngineConfig` directly is unaffected and still gets every registered analysis. Which ids are in
  that set is not a promise: read `nativeAnalyses.shippedOff` off your own run rather than pinning
  the three names.
- **`SourceSymbol.id` is NOT UNIQUE — that is the declared contract, and it collides today.**
  Java/C# overloads, TypeScript overload signatures, and TS declaration merging each collapse
  several declarations onto one id. **Treat it as a label, not a key**: keying a map by it silently
  drops the colliding siblings, and iteration order then decides which one survived — a security
  verdict has been flipped exactly that way. The contract, the measured collision count, and the one
  convention a consumer that must pick a winner is required to use are on the field itself, in
  [`docs/adapters/envelope.schema.json`](docs/adapters/envelope.schema.json) (`sourceSymbol.id`) and
  `crates/core/src/ir.rs`. Making it unique was considered and declined; a disambiguating spelling
  added later would be additive, so nothing here forecloses it.
- **Native analysis ids use three namespace conventions at once, and are not being normalized.** A
  native id is bare, `cross-layer/`-prefixed, or `schema/`-prefixed, and the prefix carries meaning
  rather than history: bare = judged inside a single tree, `cross-layer/` = exists only where a
  join across trees does, `schema/` = emitted by the schema family gates. Some analysis names occur
  in both a bare and a `cross-layer/` form, as two genuinely different analyses for that reason. Do
  not infer that a bare id will grow a prefix, do not strip one, and do not treat a bare/prefixed
  pair as a duplicate. How many such pairs there are is not stated here, and neither are their names —
  this sentence read "One analysis name occurs" while the command below printed two (external review
  round 22, 2026-09-14), which is what a number in prose does. The command is the owner:

  ```sh
  zzop contract rule-catalog | grep -oE '^\| `[a-z0-9/-]+`' | tr -d '|` ' | sort -u > /tmp/ids
  comm -12 <(grep '^cross-layer/' /tmp/ids | sed 's|^cross-layer/||' | sort) \
           <(grep -v '/' /tmp/ids | sort)
  ```

- **The KEYS INSIDE `findings[].data` are not covered, though the field itself is.** The table's first row covers
  field names and types, and `data` qualifies: it is always present and always a JSON object. What a rule writes
  into it is another matter. `docs/rules/dsl-reference.md` documents each matcher's payload so a reply is readable,
  and those descriptions track the code rather than binding it. This is unsettled BY DECISION and stays that way:
  `Finding::data` is a free-form `serde_json::Value` at the engine boundary, its own doc refuses a per-rule table
  of shapes on the grounds that a thirteenth rule leaves such a table silently short, and the guard that exists
  (`crates/engine/tests/rule_contracts/finding_data_keys.rs`) asserts one direction only — that every key a shipped
  consumer reads is spelled by some producer. Nothing watches a rule changing the shape it writes. Promising a shape
  no machine can hold is the failure mode this section exists to prevent, so the promise is not made. Read `data`
  defensively; a key you need that is absent means that rule does not carry it.

## Explicitly NOT part of the compatibility surface

These change freely at any time, by design — do not build on them:

- **The `--baseline` file's FORMAT.** zzop writes it, and its own `meaning` field tells you to commit
  it — so it lands in your repository and looks like a contract. It is not one. Its keys are today's
  shape: `byRule` is the ratchet the gate reads, and a file that parses as JSON without it is refused
  with exit 1 rather than treated as empty. There is no schema version in it; `zzopVersion` records
  which build wrote it and nothing promises the next build reads it. If an upgrade starts refusing
  your committed baseline, delete it and re-run to re-record — that is one command and the recorded
  counts are re-derived from the tree, not from the file.

  Listed here on 2026-09-15 (external review round 23, ledger V251) because it was in NEITHER list.
  The table above is what this project promises and this section is what it refuses to promise; a file
  zzop instructs you to commit belongs in one of them, and silence is the one thing the 0.x license
  does not include.

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
- **`--fail-on`'s exit code `3`** — it reports how many findings cleared a threshold, so it moves with
  the finding set for exactly the reason that set is excluded above. A run that starts or stops exiting
  `3` because a rule's band moved is the gate working, not a broken promise. The other three exit codes
  ARE covered — they answer whether the CALL was right rather than what the code contains, and splitting
  them is deliberate: one verdict over all four would have to be wrong about one half (2026-08-25).

- **The Rust crates (`zzop-*`)** — internal workspace crates, not a published stable library
  API. The consumer surfaces are the `zzop` CLI binary and the `zzop-mcp` binary (MCP tools), the
  Claude Code plugin / Claude Desktop `.mcpb` bundle built from the latter, and the Normalized AST
  protocol.

## Documents that are compiled INTO the binary — changing one requires a release

These files are not read from disk at run time. They are `include_str!`-baked into the binary and served
as the `zzop contract` / `zzop://contract/*` resources by `crates/summary/src/contracts.rs`, so **a
reader on a prebuilt binary sees the bytes from the release they installed**, never the ones in this
repository. The `include_str!` is not always written in `contracts.rs` itself — see the two notes below
the list — which is why the guard derives this set through three paths rather than one grep.

The consequence is a rule, not a nuance: *"only documentation changed, so no release is needed"* is
**false for every file in this list**. A pack author on the previous release cannot see a knob, a
schema field, or a rule row added here until a new binary ships.

<!-- EMBEDDED-CONTRACT-DOCS: this list is machine-checked against the `include_str!` calls in
     crates/summary/src/contracts.rs, the generated example-pack rows, and the `include_str!` behind
     every cross-crate constant contracts.rs serves, by scripts/check-embedded-contract-docs.sh.
     Adding a baked document without listing it here (or the reverse) fails that guard. -->

- `docs/adapters/envelope.schema.json`
- `docs/NORMALIZED_AST.md`
- `docs/adapters/key-normalization.fixture.json`
- `docs/adapters/README.md`
- `docs/rules/dsl-reference.md`
- `docs/rules/authoring-guide.md`
- `docs/contracts/rule-pack.schema.json`
- `docs/contracts/example-envelope.json`
- `docs/rules/catalog.md`
- `crates/config/config-surface.json`
- `crates/config/src/config-template.jsonc`
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

`crates/config/config-surface.json` is the THIRD path and was missing from this list until 2026-08-14.
`contracts.rs` serves it as the `config-surface` resource, but the `include_str!` that bakes it is in
`crates/config/src/lib.rs` (`CONFIG_SURFACE_JSON`) — the same bytes the unknown-key warner reads, so
there is one embed and one truth. A grep for `include_str!` inside `contracts.rs` cannot see it, which
is exactly how it stayed unlisted while being shipped; the guard now follows every `zzop_*::CONST` that
`contracts.rs` serves back to the crate that defines it. ONE served resource still has no row here and
never will, because there is no file to edit: `disclosure-classes` is RENDERED at build time from Rust.
It still needs a release to reach a reader — a source file is not a document — and the guard aborts
rather than skips if a served constant resolves to neither an `include_str!` nor a definition it could
read. `config-template` was the second such resource until this unreleased window, when the starter
document moved out of a raw string literal in `crates/config/src/template.rs` into
`crates/config/src/config-template.jsonc` and gained its row above; the carve-out was for a resource
with no file, and it now has one.

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

