# Authoring a DSL rule pack

How to write and ship a `rules/dsl/<pack>/<pack>.json` pack. Field-by-field semantics live in
[dsl-reference.md](dsl-reference.md); this doc covers placement, a worked example, performance, testing,
and when to reach for a native rule instead.

## File placement

A pack is one `<id>.json` file, loaded from a configured packs directory via
`zzop_core::pack_loader::load_dsl_packs`. Two directory shapes are supported, and may be mixed in the
same directory.

**How you name that directory depends on which surface you are on, and the two spellings are different
words** — a pack author who guesses gets an ignored key rather than an error:

| Surface | Spelling |
|---|---|
| `zzop.config.jsonc` (CLI, MCP) | `packs.extraDirs` — an array of directories |
| no config at all | drop the file in `<tree>/zzop/rules/` and it loads (`DEFAULT_AUTHORED_PACKS_DIR`) |
| embedding `zzop-facade` directly | the `packsDir` request field — see [../modules/facade.md](../modules/facade.md#the-zzop-facade-json-contract) |

`packs.extraDirs` **replaces** the `zzop/rules/` default rather than adding to it, so the two config-file
rows above are alternatives, not a pair. Below, `<packsDir>` means "whichever directory you named".

- **Flat** — `<packsDir>/<id>.json`, directly under the directory. This is what an external/third-party
  `packsDir` typically uses.
- **Depth-1 nested (pack folder)** — `<packsDir>/<name>/<id>.json`, one subdirectory per pack. This repo's
  own first-party packs use this shape: `rules/dsl/<pack>/<pack>.json`, with the pack's end-to-end tests
  co-located right next to it as `rules/dsl/<pack>/<pack>.rs` (wired into the `zzop-rule-packs` crate,
  `rules/Cargo.toml` — see [`rules/README.md`](../../rules/README.md)).

Neither shape is required over the other — nesting is purely organizational. Both are scanned in the same
call: every `*.json` found (flat or one level down) is read, parsed into `RulePackDef`, and sorted by full
path for deterministic load order — registration order must never depend on OS directory-listing order. A
malformed or too-new-schema file is reported as a per-file error (`PackLoadError`); it does not stop the
other packs in the directory from loading. First-party packs ship in this repo's own `rules/dsl/`;
third-party packs use the exact same schema and load path (flat layout is the simplest choice for a small
third-party pack set, but nesting works too).

`packsDir` accepts either one directory or an array of directories — each is loaded independently with
`load_dsl_packs` and then merged by pack `id`: if the same `id` shows up in more than one directory, the
pack from the LATER directory in the list replaces the earlier one WHOLE (not a per-rule merge). See
[../modules/facade.md](../modules/facade.md#defaults-a-config-is-required-what-it-does-not-have-to-say)'s "Defaults" section for how a
host uses this to let a caller add packs alongside the bundled ones instead of replacing them.

A host with no filesystem-resident pack directory at all (e.g. a self-contained binary embedding its
packs at compile time) can instead hand already-parsed `RulePackDef` data straight to the engine via the
request-level `packDefs` field, bypassing `pack_loader` entirely — same schema, same same-id-collision
rule, just no directory read involved. This changes nothing about how a pack is authored or tested; it is
one more way the finished JSON reaches the engine.

## Worked example

A pack that flags a hardcoded `X-Debug-Token` header value (should come from config/env, not be baked
into source) — a small but realistic `line-scan` rule. Note two things the JSON does NOT contain. There
is no `suppress_marker` field: the inline marker is derived as `zzop-<id>-ok` (here
`zzop-hardcoded-debug-token-ok`). And the `message` does not name that marker — the engine appends the
sentence that does, along with the disable hint (see "The auto-appended suppress and disable
sentences" below). Write the cause and the fix; the two escape hatches are added for you.

```json
{
  "id": "debug-headers",
  "schema_version": 1,
  "rules": [
    {
      "id": "hardcoded-debug-token",
      "severity": "warning",
      "message": "X-Debug-Token header set to a string literal — this bypasses per-environment config and risks shipping a real token. Read it from env/config instead.",
      "matcher": {
        "type": "line-scan",
        "file_pattern": "(?i)\\.(ts|tsx)$",
        "require_file": "X-Debug-Token",
        "skip_comment_lines": true,
        "line_pattern": "[\"']X-Debug-Token[\"']\\s*:\\s*[\"'][^\"'`]+[\"']",
        "snippet_max": 160
      }
    }
  ]
}
```

- `require_file` is a cheap whole-text pre-skip: most files never mention `X-Debug-Token` at all, so this
  avoids running the real (costlier) pattern against every line of every file.
- `// zzop-hardcoded-debug-token-ok: rotated in CI` on the offending line or the single line directly above it
  suppresses the finding — the marker is `zzop-` plus the rule's id plus `-ok`, derived automatically, never declared
  (see `docs/rules/dsl-reference.md#suppress-marker-semantics`).
- Drop this into a `RuleContext` (or run it through `zzop_engine::analyze_tree` with `packs` including it)
  and it behaves exactly like any shipped pack — there is no first-party/third-party distinction at the
  interpreter level.

[dsl-reference.md](dsl-reference.md) is the field-by-field reference for every matcher shape; worked
`method-scan`, `symbol-scan`, and `io-scan` examples live in `crates/core/src/dsl.rs`'s own test module (the
`http-conventions` fixture pack there is a full `symbol-scan` + `io-scan` end-to-end demo, kept test-only
rather than shipped — it exists to demonstrate the matcher shapes, not because it detects anything real).

**`symbol-scan` ships with zero rules using it, and you should know that before you pick it.** The
matcher kind is implemented, evaluated, and covered by that end-to-end demo — it is not a stub — but no
rule in any bundled pack is a `symbol-scan` today. Practically that means two things: the shape has no
production-scale exercise behind it, and it is the one kind you cannot copy a shipped example of. Nothing
here discourages using it; the point is that the other five kinds have been run over real corpora
thousands of times and this one has not. (Said out loud rather than left to be discovered, on the same
disclosure rule the rest of this engine follows. The claim is machine-held —
`zzop_core::dsl::tests_shipped_matcher_kinds` counts the bundled packs and fails if a `symbol-scan` rule
ever ships, which is what forces this paragraph to be deleted on the day it stops being true.)

## The auto-appended suppress sentence

Every DSL finding's `message` gets up to two more sentences appended by the engine at runtime, AFTER
your own `message` text — you write neither yourself. This section covers the first; [the next
section](#the-auto-appended-disable-hint) covers the second, which always comes last.

**How to suppress this one finding.** `zzop_core::dsl::suppress_hint`
(`crates/core/src/dsl/markers/channel.rs`) builds it from your rule's matcher kind:

```
line-scan / method-scan     Suppress a vetted case with `// zzop-<id>-ok`.
call-scan / literal-scan    Suppress a vetted case with `// zzop-<id>-ok` (`# zzop-<id>-ok` in Python).
symbol-scan / io-scan       (nothing — see below)
```

The leader set differs because the channels differ, and `zzop explain <rule-id>` prints the full answer
for any rule (`suppress marker:`). Two kinds get NO sentence, and both silences are deliberate:
`symbol-scan` findings name a symbol rather than a line, so no comment anywhere can suppress them; an
`io-scan` finding's anchor line is re-read through a callback that envelope mode (`analyze-envelope`)
answers with nothing, so its marker is inert there. The engine would rather say nothing than tell you to
write a comment that cannot work. **If you author a `symbol-scan` or `io-scan` rule, say how to exclude
it in your own `message`** — nothing is appended for you.

## The auto-appended disable hint

**How to turn the rule off wholesale** — the second appended sentence, and always the last thing in the
message. `zzop_core::disable_hint` (`crates/core/src/finding.rs`) builds:

```
Disable via config `rules: { "<pack>/<rule>": "off" }` (embedders: `disabledRules`)
```

Both appended sentences come from `crates/engine/src/pipeline/findings.rs`'s `append_hints`, which every DSL
finding-construction site routes through — the fused per-file pass, `envelope::file_pass` (Mode B), and
the whole-tree io-scan pass — one shared builder, never a second hand-written copy. This happens once,
before the finding reaches `AnalysisCache::put_findings`, so both sentences are baked into the cached
`message` and are not re-appended on a warm cache hit.

**What this means for your `message` field**: write the cause and the fix. That is the whole contract
for what you author. Do NOT hand-write either appended sentence — the engine adds them, and a copy
renders TWICE in the finding a reader actually sees. Both halves are mechanized:
`scripts/check-pack-suppress-sentence.sh` rejects a `message` that ends with the suppress sentence the
append would have added, and contract 17 rejects one naming `disabledRules`/`disabled_rules`.

**When your rule genuinely needs different wording**, write it and name the marker once more anywhere in
your `message` — the append opts out as soon as the message already names `zzop-<id>-ok`, so your
sentence stands alone. That is not a loophole, it is the mechanism for the cases the append cannot know
about: `security/config-file-secret` matches `.env`/`.yml`/`.toml`, where `//` is not a comment at all,
so it writes its own `` `# zzop-config-file-secret-ok` `` sentence and keeps it.

## Performance: `require_file`/`require_file_all` rare-token-first

`require_file`/`require_file_all` are pre-skips evaluated against a file's **whole text** before the
per-line/per-span logic runs — cheap only if they actually reject most files quickly. Order
`require_file_all`'s entries **rare-token-first**: a single `A[\s\S]*B` clause whose `A` is a common token
(e.g. `from`, present in nearly every import line) forces a full-text regex scan of almost every file in
the repo before it can reject anything. Splitting into `[rare, common]` AND-parts — and listing the rare
probe first — lets it reject most files up front, so the expensive clause only ever runs against the
small remaining set.

This is not a hypothetical: it is exactly what happened with `sql/query-logic-density` during the first
performance pass over a 1,355-file corpus. Rule profiling (`EngineConfig::profile_rules` →
`rule_timings`, the ESLint `TIMING=1` / oxlint rule-timing equivalent) identified it as the top-1 hotspot
(suspected regex backtracking, `(?i)\bcase\b` scanned unconditionally). Adding a rare-token-first
`require_file_all` pre-skip — plus, for `method-scan` generally, a whole-file "every `patterns` entry
must match somewhere in the file" pre-skip ahead of the per-span loop (a strict subsumption of the
per-span check, so findings are unchanged) — took the corpus's cold run from 4.15s to 3.04s with the
finding count byte-for-byte identical. The lesson generalizes: **when authoring a pattern-heavy rule,
reach for `rule_timings` before assuming a slow pack needs a native rewrite** — a cheap, rare-token-first
pre-skip is usually enough.

## Testing convention

Pack correctness is tested as **engine end-to-end over fixture trees**, not by unit-testing the
interpreter against synthetic JSON alone (though `dsl.rs`'s own test module does plenty of that for the
matcher machinery itself). A pack's own test suite:

- Loads the real `rules/dsl/<pack>/<pack>.json` (via `load_dsl_packs`, exactly as the engine would), not an
  inlined copy — so the shipped file is what's actually under test.
- Runs it through `eval_pack`/`analyze_tree` against small hand-built source fixtures that reproduce the
  rule's documented reference cases.
- Asserts both the positive cases (the pattern fires) and the negative cases (it doesn't) — a rule that
  only ever fires is not tested, it's decorated.

**The fidelity bar**: a rule must reproduce every one of its own documented reference cases before it
ships — or ship a documented, narrower subset with the gap explicitly called out (see any pack's
`message` field for examples of documented precision limits, e.g. `security/taint-flow`'s "coarse v1
approximation" note). A rule that silently drops cases relative to its own stated intent is a worse
outcome than not shipping it at all — it teaches users not to trust findings from a whole pack, not just
the one weak rule.

## When a rule does NOT fit the DSL

Some detections are structurally impossible to express with the matchers above, no matter how
clever the regex. Reach for a native rule (`rules/native/*`, statically linked into `zzop-engine` — never into
`zzop-core`, which stays rule-agnostic; see `rules/README.md`) instead when
the check needs:

- **Absence beyond what `absent` expresses.** `method-scan`'s `absent` only vetoes on a *pattern
  appearing in the same span*; it cannot express "this identifier is declared but never read" or "this
  key is set but its TTL is never checked" — that needs real declaration→use correlation, not
  co-occurrence. (A "cache key set without an expiry" check is exactly this shape: distinguishing a real
  Redis client from a `Map` used as a cache needs the declaration, not a nearby token — which is why no
  such rule ships as DSL.)
- **Cross-file joins.** Every matcher operates on one file's `SourceFile` slice (text + symbols + IO
  facts) in isolation; nothing in the DSL contract can see a second file's content. A rule that needs to
  resolve a constant defined in another module, join against a shared `REDIS_KEYS`-style vocabulary
  module, or correlate a route registration in one file with its handler's body in another (`http`
  pack's `protected-path-no-auth-evidence`/`dev-path-no-guard-hint` already approximate this by folding everything onto one
  registration line — the real cross-file handler-body check is out of scope for line-scan) needs either
  a whole-graph native rule or a new IR-level join primitive.
- **Declaration→use / call-graph tracking.** Any check that must follow "handler X is registered at this
  route, and X (or something X calls, transitively) does Y" is a call-graph BFS problem, not a
  per-file pattern match. `unsafe-read-endpoint`/`non-idempotent-write` (`rules/native/rules-http`) are
  exactly this shape: they resolve an `ApiEndpoint`'s handler to a symbol, then BFS the whole-repo
  `SymbolGraph` for a reachable write site.
- **AST shape rather than text co-occurrence.** Anything that genuinely needs a parse tree — cyclomatic/
  cognitive complexity, nested-loop depth, JSX/React-specific structural analyses — has no honest
  regex-over-lines encoding. These stay native (or wait on a parser projection rich enough to expose the
  needed shape as new `SourceSymbol`/`IoFacts` fields, keeping the DSL itself unchanged).

See [catalog.md](catalog.md) for the current native-analysis inventory, including the roadmap backlog of
detections that fit neither category yet.

## Machine-enforced contracts

The cross-cutting rules above (marker on every finding, message tells the reader how to exclude it, catalog
totals match reality) used to be conventions a human had to remember — and drifted, silently, more than
once. `crates/engine/tests/rule_contracts/` machine-enforces them over every shipped DSL pack and the
native registry, so a violation is a failing test in `cargo test --workspace`, not something a reviewer has
to notice by eye. If that file's tests fail on your change, the test name and failure message identify
exactly which rule/pack/doc line to fix — do not silence the test, fix the offending rule or doc.

What it checks:

- **Derived-marker uniqueness** — markers are derived `zzop-<id>-ok`, so presence and the `-ok` shape are
  construction guarantees; what the test still enforces is that no two rules — in any pack — derive the same
  marker (i.e. rule ids are globally unique), since a shared marker would silently co-suppress both.
- **Message triple** — every DSL rule's message **as a reader receives it** (your `message` plus the
  engine-appended suppress sentence) names its own derived marker (or, for a disable-only
  rule, the literal `disabledRules` string — `disabledRules` is the wire spelling every embedder
  request surface actually accepts; the contract also still recognizes the retired snake_case
  `disabled_rules`) somewhere in the text — the "how to exclude"
  leg every finding must carry alongside its problem/fix explanation.
- **Native message contract** — a pragmatic grep over `rules/native/*/src/**/*.rs`: any file that
  constructs a `Finding` via a literal `rule_id: "..."` must also mention `disabledRules` (or the Rust
  field spelling `disabled_rules`, or call `zzop_core::disable_hint`) somewhere in the
  same file (native findings are built in code, so there is no single declarative `message` field to
  inspect precisely the way the DSL check above can — see the test's own doc comment for exactly what this
  proxy can and cannot prove).
- **Id hygiene** — DSL pack ids are unique across packs, rule ids are unique within a pack, and no DSL
  `"pack"` or `"pack/rule"` id collides with a native analysis id (all three id shapes share one
  `disabledRules`/`suppressions` string-match space — see `crates/core/src/registry.rs::is_enabled`).
- **Public site** — after writing your catalog row, run `node scripts/gen-site-rules.mjs`. It rewrites
  `site/rules.html`'s rule rows from [catalog.md](catalog.md), which is their only source; hand-editing
  those rows fails `check-rules-catalog-sync.sh`. Two consequences worth knowing before you write the
  row: whatever you put in the `Detects` cell is what the public page will say, and a fact you state
  ONLY on the site is a fact the next generation deletes.
- **Catalog sync** — [catalog.md](catalog.md)'s totals sentence (`N DSL packs, N DSL rules, N native
  analysis ids`) matches what `load_dsl_packs`/`register_native_analyses` actually load, and every native
  analysis id / DSL pack id appears somewhere in the catalog's text. The sightline surface is pinned as a
  set equality, both directions: every catalog row carrying a sightline paragraph must have a
  `RuleSightline` declaration (or an explicit, documented exemption in the test), and every declaration
  must have a catalog sightline paragraph — a rule may not publish a blind-spot claim the machine half
  doesn't know, nor declare one the prose doesn't state.
- **Determinism guard** — loading `rules/dsl` twice yields identical pack data in identical order (a cheap
  regression net for map/directory-iteration-order bugs in pack parsing).

## Recurring defect classes — checklist for every new rule

Successive review rounds kept re-finding the same two defect classes under different rule names, because
each fix was applied to one sampled rule instead of the whole class. A whole-catalog sweep fixed the backlog
and turned the underlying judgment calls into a checklist every new `line-scan`/`method-scan` rule must run
through before it ships:

0. **Is this a defect, or a preference?** Declare it: `"axis": "defect"` or `"axis": "opinion"` (see
   [dsl-reference.md § Axis](dsl-reference.md#axis-defect-vs-opinion)). The test is one question —
   *could a competent team deliberately choose the flagged shape, in production, and be right?* If yes it
   is an `opinion`, and the `message` must name that case, the way `sql/destructive-migration` does.
   ⚠ That rule is also the standing warning against reading this answer as an export decision: it
   declares `opinion`, correctly, and is BUNDLED anyway, because three critical siblings narrow
   themselves on the premise that it runs. Exporting it on the strength of its axis alone left a real
   `DROP TABLE` unreported until it was put back. `axis` answers *what kind of claim is this*; whether a
   default run still makes sense without the rule is a separate question with a separate answer. This
   is the only checklist item a rule shipped from this repo cannot skip — `examples/packs/` included,
   since 2026-08-12: the field defaults to `defect` for third-party compatibility, but
   `rule_contracts::rule_axis::every_shipped_rule_declares_its_axis` reads the pack JSON text and
   rejects a rule in either directory that says nothing. Do NOT reach for `severity` to express this —
   it answers confidence × blast radius, and the two were measured to disagree.

1. **Can this pattern match inside a comment?** For a keyword/call-shaped `line_pattern`/`patterns` regex,
   the answer is almost always yes — a JSDoc example, an ESLint-disable comment naming the rule, prose
   mentioning the keyword, or commented-out old code all read as ordinary source text to a regex. Set
   `"skip_comment_lines": true` unless the rule is deliberately inspecting comment/annotation *content*
   itself (no rule in the current packs does this — a hypothetical TODO-marker rule would be the shape of
   exception that qualifies; a Java `@Annotation(...)` pattern does NOT qualify as an exception, since an
   annotation is code, not a comment, and turning the flag on only filters lines that are genuinely
   comments). `skip_comment_lines` skips a line whose trimmed text opens a comment in that file's own
   syntax (`//`, `/*`, `*` everywhere, and `--` in a `.sql` file — one extension-keyed table, not a fixed
   triple) for BOTH
   `line-scan` (whole-line matching) and `method-scan` (per-line within the symbol span, including `absent`
   guard checks) — safe to enable by default because it can only remove comment-only false matches, never a
   real code-line match.
2. **Is this rule about deployed surface or repo content?** Most rules reason about what the application
   *does* at runtime (a missing `await`, a wildcard CORS origin, an unbounded query) — call this
   **deployed-surface**: a test file exercising the same code shape isn't a production bug, so exclude test
   paths. A minority of rules reason about a literal value simply being *present in the repo*
   (`security/hardcoded-secret`, `security/hardcoded-password`) — call this **repo-content**: a
   secret committed inside a test fixture is still a leaked credential the moment it's pushed, so these must
   scan every path, test directories included. Decide which one a new rule is, and for deployed-surface
   rules add the shared canonical test-path exclude (copy it verbatim, do not invent a new regex):
   ```
   "file_exclude_pattern": "(?i)((^|/)(e2e|tests?|__tests?__|spec|fixtures?)/|\\.(test|spec)\\.|\\.stories\\.|(^|/)\\.storybook/|(^|/)(playwright|vitest|jest|cypress)\\.config\\.)"
   ```
   This is the same string `reliability/debug-true-committed` and `code-hygiene/localhost-url-literal-committed`
   already used before the sweep unified every other deployed-surface DSL rule onto it. If a rule already
   has a `file_exclude_pattern` for an unrelated reason (e.g. `code-hygiene/process-exit-in-lib` excludes
   `scripts?/tools/bin` as CLI-entrypoint dirs), leave that alone rather than conflating two different
   exclude reasons into one regex. (`code-hygiene/env-outside-config` used to fold a config-basename guess
   into the same field; it now carries only the shared test-path fragment and gets its real exemption from
   a declared attribute — see item 3.)
   Adversarial review on a large real monorepo closed three more gaps in the canonical string: NestJS
   `*.e2e-spec.ts` files (the old `\.(test|spec)\.` alternative requires a literal `.spec.`, which an
   `-spec.` hyphen separator doesn't produce), `packages/testing/` helper directories, and `vite.config.*`
   (the tool-config alternation had vitest/jest/playwright/cypress but not vite). The canonical string is
   now:
   ```
   "file_exclude_pattern": "${test-paths-stories}"
   ```
   **Reference it; never paste a copy.** The string used to be quoted in full right here, and quoting it
   is how it rots: a copy cannot pick up the next arm added to the shared body. That is not
   hypothetical — `reliability/sync-fs-in-handler` shipped exactly such a hand-copy (the same body plus
   `scripts?|tools|bin`), and when the shared vocabulary gained the Go/Python/C# conventions on
   2026-08-10 the copy kept the TypeScript-only arms and went on judging `handler_test.go`. If you need
   "the shared body plus one arm", the mechanism is a pack-local fragment NAMED `test-paths-<something>`:
   the `test-paths-*` family is what `dsl::tests_fragments::superset` pins as a strict superset of the
   base, and a name outside that family is a copy nothing guards.

   The shared body is the single owner of "what is a test path" for the whole engine —
   `zzop_core::is_test_file`, the native/cross-layer predicate, reads the same string — and it covers each
   language's own convention, not one language's applied to all. `docs/rules/dsl-reference.md`'s
   "Path-exclusion semantics" section has the per-language table and the `vocabulary.extraTestPathPatterns`
   key a project uses to add its own.
3. **Does this rule rest on a fact the source cannot state?** "Which directory is the config module",
   "which tree is generated", "which paths are vendored" are facts about the *project*, not about the
   code — and a basename regex or a whole-file shape heuristic that infers one is a guess, which this
   engine's soundness floor rules out. Gate on a declaration instead: `attr_absent`/`attr_present` read
   an attribute the project injects through an overlay, and `require_attr_declared` makes the rule stand
   down — loudly, in `warnings` — when nobody has declared one. See
   [dsl-reference: Attribute gates](dsl-reference.md#attribute-gates-consuming-a-declaration). The trade
   has to be made deliberately: a user who declared nothing for it then gets silence plus a disclosure instead of a
   partly-right answer, so this is right only where a wrong answer is worse than none.
   `code-hygiene/env-outside-config` is the shipped example — it previously guessed the config module from
   its basename and from two whole-file syntax shapes, and its own message admitted the decisive gap
   ("a whole-tree fact no file-local matcher can establish").
4. **Does the message carry problem + fix + suppress?** Every DSL rule's `message` must explain what's wrong,
   how to fix it, and name its own derived marker `zzop-<id>-ok` — already machine-enforced by the "Message triple" check
   above, but worth checking by eye while drafting: a reviewer should never have to guess how to vet a
   false positive.
5. **Does the message make a claim about what the matcher does or doesn't flag?** If the message
   says "plain `as X` is not flagged", "only literal query strings are kept", "a bare `$transaction`
   still fires" — that claim is a testable contract, and an unpinned claim WILL drift (shipped
   examples: `as-cast`'s pattern matched a lone `as unknown` its message promised to skip;
   `race-condition-toctou`'s message called `$transaction` insufficient while its matcher still
   vetoed on it). Add a fixture to the pack's `.rs` test that asserts the claimed behavior — one
   positive or negative per claim, named after the claim.
6. **Is this pattern an English word that could appear in prose?** Comments are already excluded by item 1,
   but a string literal isn't — a keyword pattern that happens to be an ordinary English word (`do`, `for`,
   `while`, `update`, `delete`, `select`, etc.) will also match that same word sitting inside prose text
   (`"logged in to do this"` matches a bare `\bdo\b`; `"waiting for ${x}"` matches a bare `\bfor\b`). Require
   an adjacent syntax anchor — a `(`, `{`, a wrapping quote, etc. — immediately before/after the word in the
   same alternative (`\bdo\s*\{`, not bare `\bdo\b`; `"..."` wrapping `SELECT`/`UPDATE`, not a bare
   `\bUPDATE\b`), never a bare word alone. Machine-checked by the `rule_contracts` meta-test's
   `dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches` test (see that test's own doc comment
   for the curated word list and exactly what the check can/cannot prove) — this is the fix that shipped for
   `perf/api-in-loop` (bare `\bdo\b`) and `security/sql-string-concat` (bare `UPDATE`).
7. **What is the nearest benign lookalike, and is it pinned as a negative fixture?** Before shipping,
   name the most common INNOCENT code that matches the same surface shape the rule keys on, and pin it
   as a negative test in the pack's `.rs` — not a synthetic near-miss, but the real-world idiom a scan
   of an ordinary repo will actually hit. Field-audit examples of rules that shipped without this and
   fired on their lookalike immediately: `sql/truncate-in-app-code` (SQL `TRUNCATE` vs a JSX `truncate`
   boolean prop AND Tailwind's `truncate` utility class), `security/private-key-committed` (a PEM
   header carrying a key vs a doc/i18n sentence merely *naming* the header),
   `reliability/sync-fs-in-handler` (Express's `res` vs `const res = await fetch(...)`), and
   `perf/api-in-loop` (a request-per-iteration loop vs the universal single-fetch-then-`.map()`
   response transform). A positive fixture proves the rule CAN fire; only the benign-lookalike negative
   proves it knows when NOT to.
8. **Does the claim need structure the matcher doesn't have?** `line-scan`/`method-scan` see text
   co-occurrence within a span — they cannot see containment (X *inside* a loop), order (X *then* Y), or
   dataflow (X *flows into* Y). If the rule's value depends on such a relation, either (a) use a
   structural fact the parser projects (e.g. `MethodScan::trigger_in_loop` over `loop_spans`, the fix
   that replaced `perf/api-in-loop`/`sql/nplus1`/`sql/count-in-loop`'s loop-token co-occurrence after a
   field audit found 11/11 false positives), or (b) keep the co-occurrence matcher but make the message
   SAY co-occurrence, in the `db/multi-write-no-tx` house style ("This is a co-occurrence heuristic,
   not proof ..."), and cap severity at `warning` — `critical` is reserved for matchers that PROVE their
   claim (a closed literal, an unambiguous token). Never ship a structural claim on a textual matcher.
