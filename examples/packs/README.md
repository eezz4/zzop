# Example rule packs — written by us, deliberately NOT bundled

A pack here is real, loadable, and shipped in the repository, but it is **not** in `rules/dsl/`, so no
zzop run loads it unless you point a config at it. Copy one, edit it, or use it as the shape reference
for your own.

Two ways, both verified against the shipped binary:

```jsonc
// zzop.config.jsonc — name the directory explicitly
{ "packs": { "extraDirs": ["path/to/your/packs"] } }
```

```
# ...or write zero config: drop the file here and it loads
<your-tree>/zzop/rules/<anything>.json
```

⚠ The two do not combine the way you would expect. `packs.extraDirs` **replaces** the default
`zzop/rules/` location rather than adding to it, so if you already declare `extraDirs`, a file dropped
in `zzop/rules/` is silently not read — add its directory to your array instead. (Bundled packs are
unaffected either way: they are compiled into the binary, not loaded from disk.)

⚠ `packsDir` is **not** a config-file key. It is the embedder request field
([`docs/modules/facade.md`](../../docs/modules/facade.md)), and until 2026-08-11 this README printed it
inside a `zzop.config.jsonc` block — the only retrieval instruction in the repo, and it did not work.
Measured: the config front end answers `unknown config key "packsDir" (ignored)` and loads 11 packs,
where `packs.extraDirs` loads 12.

## A copy taken from a binary knows which binary it came from

You do not need this checkout. Every shipped binary serves each pack here as a contract resource named
`example-pack-<file stem>` (over MCP `resources/read`, and from the CLI's contract lane) — that is the
retrieval path for anyone who installed zzop rather than cloning it.

Those served bytes are the file below **plus one line**:

```json
"exported_from": { "zzop_version": "<the serving build's version>", "contract": "example-pack-orm-eager" }
```

That line is why a saved copy stops going quietly stale. A pack sitting in `zzop/rules/` keeps running
the rule set its source build shipped, and rules move between packs — and in and out of the bundle —
release to release, which is the whole subject of the sections below. When the running engine's version
differs from the stamp, every run prints one `warnings` line naming both versions and the resource to
re-read. The pack still loads whole and nothing is skipped; staying on an older rule set on purpose is
a valid choice and the warning says so.

The stamp is minted at retrieval, so the committed files here carry none — a version typed into a
tracked file is a value someone has to keep true forever. A pack copied straight out of this directory
is therefore unstamped and silent: nobody derived a provenance for it. Full field docs:
[dsl-reference § Retrieval stamp](../../docs/rules/dsl-reference.md#retrieval-stamp-exported_from).

`docs/rules/authoring-guide.md` is the how-to; `docs/rules/dsl-reference.md` is the field-by-field
reference. Nothing here is a second-class pack — the interpreter has no first-party/third-party
distinction.

## `orm-eager.json` — 3 eager-loading declarations (the OPINION axis)

**A different reason from `typescript-lint` at the bottom of this file.** `typescript-lint` left because other tools already see it.
These three left because they are **opinions**: each declares `"axis": "opinion"` in its own JSON
([dsl-reference § Axis](../../docs/rules/dsl-reference.md#axis-defect-vs-opinion)), meaning a project
that deliberately does the flagged thing is not wrong — it disagrees. Entity-level eager loading is a
real trade, and it is also the right call for a small association read on every access. A rule that
fires on the DECLARATION hands such a project one finding per declaration, forever.

That axis is machine-held rather than a judgment restated here: every bundled rule must declare it
(`crates/engine/tests/rule_contracts/rule_axis.rs`), so "which rules are opinions" is a command rather
than a list that rots:

```
grep -l '"axis": "opinion"' rules/dsl/*/*.json examples/packs/*.json
```

⚠ **The ids changed; the suppress markers did not.** A rule id is `<pack>/<rule>`, and only three of
`perf`'s four rules moved — `perf` still ships `api-in-loop` — so these three were RENAMED by the move
(`perf/jpa-eager-fetch` becomes `orm-eager/jpa-eager-fetch`; the table is in
[VERSIONING.md](../../VERSIONING.md)). The marker is derived from the BARE rule id, so every
`// zzop-jpa-eager-fetch-ok` already sitting in your code keeps suppressing. What breaks is a
`disabledRules` / `severityOverrides` / `rules` entry naming the old full id.

**This pack keeps its tests.** `examples/packs/tests/orm_eager.rs` is a real `[[test]]` target wired from
`rules/Cargo.toml`, holding the three per-rule test modules moved verbatim. The `typescript-lint` export
did not do this and deleted 1,392 lines of tests along with the pack — leaving rules a user can still
load and run with nothing checking them. Do it this way for the next export.

## `sql-preferences.json` — 5 SQL rules on the OPINION axis

**Same test as `orm-eager` above, applied to the biggest opinionated cluster left in the bundle.**
Six of `sql`'s thirteen rules declared `"axis": "opinion"`; they left on 2026-08-12, ONE came back the
same day (see the ⚠ below), and `sql` still ships eight — `nplus1`, `count-in-loop`, both check-then-act
race rules, the three critical whole-table-write rules, and the returned `destructive-migration`. Which
rules are where is a command, not a judgment restated here:

```
grep -l '"axis": "opinion"' rules/dsl/*/*.json examples/packs/*.json
```

What unites the five that stayed exported is that the flagged shape's verdict depends on a fact the
rule cannot see:
**how much data is on the other side.** `SELECT *` is over-fetch in a hot path and unremarkable in a
one-row admin lookup. A leading `%` wildcard defeats an index on a large table and costs nothing on a
small one. `reduce(...)` / `filter(...).length` over a query result is the right call whenever the
result is already small and reused. `CASE WHEN` density in SQL is a testability trade some teams take
deliberately. Bundling those means every project pays attention to one project's taste.

⚠ **`destructive-migration` was the sixth, and it went back to the bundle the same day. This is the
export rule that cost the most to learn.** It is `info`-severity review-time disclosure — a preference
by the bar above, and its `"axis": "opinion"` declaration was correct then and is still there now. But
it is also the DISCLOSURE half of a handoff. `sql/delete-no-where`, `sql/update-no-where` and
`sql/truncate-in-app-code` all EXCLUDE migration paths, and their messages say so by naming this rule
as what covers them instead. Those three stayed. So a default run said nothing at all about a
`DROP TABLE` under `migrations/`, `migrate/` or `alembic/versions/` — not because it judged it safe,
but because the rule that spoke had become opt-in. That was not hypothetical: a `DROP TABLE` on a
populated table sits in the dogfood corpus at
`corpus/oss/be-express/src/prisma/migrations/20211001143221_implicit_tags/migration.sql:15`, and for the
length of that export nothing reported it.

**The lesson is a SECOND question, not a corrected answer to the first.** `axis` asks *what kind of
claim does this rule make* — preference or defect. Exporting asks *does a default run still make sense
without it* — and the three critical siblings had already answered no, in their own message text, by
narrowing themselves on this rule's behalf. Nobody asked the second question, because the first one had
a clean answer. Ask both. Two alternatives were measured and rejected before reverting: widening the
siblings' excludes recovered 0 of 1 real defect and added 3 false CRITICALs, and leaving it exported
with the ⚠ notice already written into the siblings' messages delivered that notice to 0 runtime readers
across 18 checkouts (the rules carrying the sentence structurally never fire under a migration path).
Those two numbers are a dated 2026-08-12 record — neither alternative exists to re-measure. The cost of
reverting is not: it is 4 `info` findings and 0 `critical` on the dogfood corpus, and that one recounts.

```
cargo run --release -p zzop-engine --example corpus_rule_counts -- corpus/oss sql/destructive-migration
```

That handoff is machine-held rather than trusted:
`rules/dsl/sql/language_scope.rs::destructive_migration_admits_every_extension_its_critical_siblings_exclude`
fails if an extension the critical rules admit in app code is one the disclosure does not admit under a
migration path — i.e. if their message ever promises a disclosure nobody emits. It read both packs while
the rule was exported, and reads one again now.

⚠ **The ids changed; the suppress markers did not.** A rule id is `<pack>/<rule>`, so all five that
stayed exported were RENAMED by the move (`sql/select-star` becomes `sql-preferences/select-star`; the
table is in [VERSIONING.md](../../VERSIONING.md)). `destructive-migration` has NO row there — it never
shipped under the exported spelling, so no user has an id to migrate. The marker is derived from the BARE rule id, so every
`// zzop-select-star-ok` already sitting in your code keeps suppressing. What breaks is a
`disabledRules` / `severityOverrides` / `rules` entry naming the old full id.

**This pack keeps its tests, and this is the export where that got hard.** `orm-eager` moved three
whole test files. Here the tests were SHARED with rules that stayed, so three files
(`aggregation.rs`, `suppression.rs`, `language_scope.rs`) were SPLIT rather than moved: each moved
rule's fixtures came here, each staying rule's fixtures stayed. `examples/packs/tests/sql_preferences.rs`
is the `[[test]]` target, wired from `rules/Cargo.toml`. **Nothing was lost in either direction**: two
language-scope fixtures that judged five rules at once became one per side, and the same rule ran the
other way when `destructive-migration` came back — its file went back to `rules/dsl/sql/` WITH its
tests. The count is deliberately not printed here. The two targets holding this split are `pack_sql` and
`pack_sql_preferences`, and

```
cargo test -p zzop-rule-packs --test pack_sql --test pack_sql_preferences
```

is the source of truth, not this prose. (Until 2026-08-12 this paragraph read *"124 tests before the
split, 126 after … and the count held"*, naming no target set at all — by the time anyone recounted, the
two targets summed to 135, and a reader had no way to tell an addition from a miscount.) Do it this way
for the next export: a rule a user can load and run must have something checking it.

## `code-hygiene.json` — 8 everyday-code rules on the OPINION axis

**The increment that finished the job.** After it, exactly ONE bundled rule declares
`"axis": "opinion"` — the `destructive-migration` disclosure that was exported and re-bundled the same
day (see the ⚠ in the `sql-preferences` section above). That is a command, not a claim:

```
grep -c '"axis": "opinion"' rules/dsl/*/*.json      # 0 in every pack but sql, which is 1
grep -c '"axis": "opinion"' examples/packs/*.json   # 8 + 3 + 5 + 3
```

(That trailing `3` was `0` until 2026-08-12: `typescript-lint.json` predated the field and declared it on
no rule, so its 12 loaded as the `defect` default. Declaring them split the pack 9 defect / 3 opinion —
which is the point of the axis being per RULE and the export test being per PACK. The same line claimed
`12` for that pack earlier the same day, which was the rule COUNT, not the axis count; both errors are
why `every_shipped_rule_declares_its_axis` now reads the pack JSON text.)

Unlike the two exports before it, this one draws from **three** bundled packs at once — `browser` (1),
`egress` (1) and `reliability` (6). All three stay and all three keep shipping their defect rules; what
left is only the rules that declared the axis in their own JSON.

What unites the eight is that each names a real trade whose verdict lives outside the line the rule can
see. A blocking `confirm()` is the wrong control in an app with a design system and the right one in a
200-line admin page. `process.exit()` inside a function is a crash in a library and the correct ending
for a CLI. A committed `http://localhost:3000` is broken in production and exactly right in a dev-only
fixture. `console.log` in `api/` is unstructured output — and unstructured output is what a small
service's operator actually reads. `process.env.X!` trades a startup check for a first-use crash, which
is a bad trade in a server and a fine one in a script that runs for four seconds.

⚠ **`localhost-url-literal-committed` is the one to read before exporting** — the same class
`destructive-migration` was in the previous increment, and it was found by asking the question BEFORE
the move rather than discovering it after: *which staying rule narrows itself because a moving rule
covers that case?* Grepping the pack JSONs alone answers "none" here, and that answer is wrong: two
narrowings live outside `rules/dsl/*/*.json`. Both of them define their own SCOPE rather than lean on a
premise about who else is running — which is what makes them survive the export, and what makes them
invisible to a search for the moving id.

* **`egress/http-url-literal`** — its `exclude_pattern` names loopback and private-range hosts outright,
  so it has never fired on a committed dev URL. The exclusion is in a regex, not in prose naming the
  sibling. **The tokens are not enumerated here** — `zzop explain egress/http-url-literal` prints that
  pattern, and the pattern is the source of truth, not this sentence. (Enumerating them was tried and
  dropped on 2026-08-12: the same six-token list sat verbatim in three files, two of which named THIS
  file as its owner in the same breath, and all three were already short of what the regex excludes.)
* **`cross-layer/external-ip-literal`** (NATIVE Rust, `rules/native/rules-cross-layer/`) — skips
  loopback hosts. A native rule is in no pack JSON at all, and until 2026-08-12 its message handed the
  case off BY ID (*"that's the DSL `localhost-url-literal-committed` rule's turf"*), which is how the
  narrowing was found. That pointer is now GONE: the exclusion was rewritten onto the rule's own
  yardstick — a literal IP is flagged because it PINS AN ENVIRONMENT, and loopback pins none — and
  `the_loopback_exclusion_is_justified_without_naming_any_other_rule` fails if any foreign rule id is
  ever planted back in the text. **Read that as the general lesson, not as a repair to this one rule:**
  a message that delegates to another rule is only true while that rule is actually running, and a
  finding cannot know that about a rule in a pack the user may not have loaded.

Both stayed, and both still decline. So a default run now says nothing at all about a committed
`http://localhost:3000` or `https://127.0.0.1/...` — not because it judged the line fine, but because
the rule that spoke is opt-in. Load this pack if committed dev URLs matter to you.

That handoff is machine-held rather than trusted, from **both** sides:
`examples/packs/tests/egress_handoff.rs` loads this pack AND the bundled `egress` pack and asserts the
routing on every address the sibling excludes — one rule fires, its cross-pack sibling stays silent —
because a one-pack test cannot tell a sibling's decision from its absence.
`rules/dsl/egress/http_shapes.rs::localhost_shapes_are_outside_this_rules_public_wire_scope` is the
bundled-side mirror. The native rule is pinned the OTHER way round — see the second bullet above: its
test now forbids naming any other rule, so nothing there can dangle when a pack moves.

⚠ **The ids changed; the suppress markers did not.** A rule id is `<pack>/<rule>`, so all eight were
RENAMED by the move (`reliability/console-in-be` becomes `code-hygiene/console-in-be`; the table is in
[VERSIONING.md](../../VERSIONING.md)). The marker is derived from the BARE rule id, so every
`// zzop-console-in-be-ok` already sitting in your code keeps suppressing. What breaks is a
`disabledRules` / `severityOverrides` / `rules` entry naming the old full id.

**This pack keeps its tests.** Four whole files moved (`console_in_loop.rs`, `env_outside_config.rs`,
`w2_languages.rs`, `localhost_egress.rs`) and four were SPLIT out of files shared with rules that
stayed (`dialogs.rs`, `config_flags.rs`, `fetch_and_process.rs`, `server_hygiene.rs`). Two `browser`
fixtures judged both a moved and a staying rule at once; they were DUPLICATED rather than cut, because
trimming a shared fixture changes what the remaining rule is measured against.
`examples/packs/tests/code_hygiene.rs` is the `[[test]]` target, wired from `rules/Cargo.toml`, and it
also holds the Mode-B overlay helpers that came with `env-outside-config` — that rule was their only
consumer, so they left `reliability.rs` with it. **Nothing was dropped here either**, and the duplicated
`browser` fixtures mean both sides gained rather than one losing. The four targets this split spans are
`pack_browser`, `pack_egress`, `pack_reliability` and `pack_code_hygiene`; recount them with

```
cargo test -p zzop-rule-packs --test pack_browser --test pack_egress --test pack_reliability --test pack_code_hygiene
```

— that command is the source of truth, not a number in this paragraph. (It read *"295 tests before the
split, 302 after"* until 2026-08-12, over "the four targets" that no sentence named.)

## `typescript-lint.json` — 12 general-purpose TypeScript/JavaScript rules

**Why it is here and not bundled (2026-08-11).** Every rule in this pack judges a single language with
no framework, no project vocabulary, and no whole-tree fact — which is exactly the work
`typescript-eslint`, `biome` and `oxlint` already do in the repositories that care, several of them with
**type information zzop does not have**. Shipping a weaker second copy was not free:

```
measured on the reference corpus, before this pack moved out
be-express (36 findings):  typescript/no-explicit-any 20      ← 56% of the report
be-nest    (12 findings):  typescript/no-explicit-any 6  ·  mutating-route-no-auth 2
```

Read the second line. Two unauthenticated mutating routes — a finding **no linter produces**, and the
kind of thing zzop exists for — sat underneath six lines of a check the project's own linter already
gives it. Across the corpus's six TypeScript trees this pack was 35% of all findings while firing in
only two of them: quiet where it was not needed, loud where it drowned the signal.

So the bar changed from *"is this rule useful?"* to **"can the tools this user already runs see it?"**
A rule earns bundling by depending on something zzop alone has — the cross-layer join, the whole-tree
graph, framework knowledge, or a declared project vocabulary. `security/hardcoded-secret` is a line
scan too and stays bundled, because one config points it at **JS/TS, Java and Rust together**
(`file_pattern` = `(?i)\.(ts|tsx|js|mjs|cjs|java|rs)$`) and no single-language linter covers that set.
These twelve read one language.

An earlier revision of this paragraph said *"eight languages"*. Re-measured 2026-08-11: it is three.
The direction of the argument is unchanged — a credential scan spanning three languages from one config
is still something no single-language linter does — but the size was inflated, and this sentence is the
worked example the whole bundling bar rests on, so it is the last place that can afford a number nobody
checked. Recount with `matcher.file_pattern` in `rules/dsl/*/*.json`; that expression is the source of
truth, not this prose.

**Nothing is lost by taking it back.** Point a config at this directory and every rule behaves exactly
as it did when bundled — same ids, same messages, same `// zzop-<id>-ok` markers. That is the whole
reason it was moved rather than deleted.

| Rule | Nearest standard-linter equivalent |
|---|---|
| `no-explicit-any` | `@typescript-eslint/no-explicit-any` (with types) |
| `as-cast` | `@typescript-eslint/consistent-type-assertions` |
| `use-effect-async-callback` | `eslint-plugin-react-hooks` |
| `foreach-async-callback` | `@typescript-eslint/no-misused-promises` |
| `promise-async-executor` | core ESLint `no-async-promise-executor` |
| `parseint-no-radix` | core ESLint `radix` |
| `always-constant-comparison` | core ESLint `no-constant-binary-expression` |
| `async-handler-no-try` · `float-equality` · `numeric-string-comparison` · `tofixed-arithmetic` · `date-pitfalls` | no direct equivalent — kept here rather than bundled because they are still single-language, single-file judgments |
