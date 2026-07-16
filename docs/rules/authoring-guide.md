# Authoring a DSL rule pack

How to write and ship a `rules/dsl/*.json` pack. Field-by-field semantics live in
[dsl-reference.md](dsl-reference.md); this doc covers placement, a worked example, performance, testing,
and when to reach for a native rule instead.

## File placement

A pack is one `<id>.json` file, loaded from a configured packs directory (the `packsDir` option — see
[../modules/napi.md](../modules/napi.md)) via `zzop_core::pack_loader::load_dsl_packs`. Two directory shapes
are supported, and may be mixed in the same directory:

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
[../modules/napi.md](../modules/napi.md)'s "Defaults" section for how the JS wrapper uses this to let a
caller add packs alongside the bundled ones instead of replacing them.

A host with no filesystem-resident pack directory at all (e.g. a self-contained binary embedding its
packs at compile time) can instead hand already-parsed `RulePackDef` data straight to the engine via the
request-level `packDefs` field, bypassing `pack_loader` entirely — same schema, same same-id-collision
rule, just no directory read involved. This changes nothing about how a pack is authored or tested; it is
one more way the finished JSON reaches the engine.

## Worked example

A pack that flags a hardcoded `X-Debug-Token` header value (should come from config/env, not be baked
into source) — a small but realistic `line-scan` rule with a suppress marker:

```json
{
  "id": "debug-headers",
  "framework": "any",
  "schema_version": 1,
  "rules": [
    {
      "id": "hardcoded-debug-token",
      "severity": "warning",
      "message": "X-Debug-Token header set to a string literal — this bypasses per-environment config and risks shipping a real token. Read it from env/config instead.",
      "suppress_marker": "debug-token-ok",
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
- `// debug-token-ok: rotated in CI` on the offending line or the single line directly above it suppresses
  the finding (see `docs/rules/dsl-reference.md#suppress-marker-semantics`).
- Drop this into a `RuleContext` (or run it through `zzop_engine::analyze_tree` with `packs` including it)
  and it behaves exactly like any shipped pack — there is no first-party/third-party distinction at the
  interpreter level.

[dsl-reference.md](dsl-reference.md) is the field-by-field reference for all four matcher shapes; worked
`method-scan`, `symbol-scan`, and `io-scan` examples live in `crates/core/src/dsl.rs`'s own test module (the
`http-conventions` fixture pack there is a full `symbol-scan` + `io-scan` end-to-end demo, kept test-only
rather than shipped — it exists to demonstrate the matcher shapes, not because it detects anything real).

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

- Loads the real `rules/dsl/<pack>.json` (via `load_dsl_packs`, exactly as the engine would), not an
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

Some detections are structurally impossible to express with the four matchers above, no matter how
clever the regex. Reach for a native rule (`rules/native/*`, statically linked into `core`) instead when
the check needs:

- **Absence beyond what `absent` expresses.** `method-scan`'s `absent` only vetoes on a *pattern
  appearing in the same span*; it cannot express "this identifier is declared but never read" or "this
  key is set but its TTL is never checked" — that needs real declaration→use correlation, not
  co-occurrence. (`redisTtlMissing`'s Map-alias exclusion is exactly this shape — deferred to the native
  backlog for this reason.)
- **Cross-file joins.** All four matchers operate on one file's `SourceFile` slice (text + symbols + IO
  facts) in isolation; nothing in the DSL contract can see a second file's content. A rule that needs to
  resolve a constant defined in another module, join against a shared `REDIS_KEYS`-style vocabulary
  module, or correlate a route registration in one file with its handler's body in another (`http`
  pack's `auth-gates`/`route-exposure` already approximate this by folding everything onto one
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

- **Marker presence** — every DSL rule has a non-empty `suppress_marker`, and no two rules in the same pack
  share one (a shared marker would silently co-suppress both rules' findings).
- **Message triple** — every DSL rule's `message` names its own suppress marker (or, for a disable-only
  rule, the literal `disabled_rules`/`disabledRules` string) somewhere in the text — the "how to exclude"
  leg every finding must carry alongside its problem/fix explanation.
- **Native message contract** — a pragmatic grep over `rules/native/*/src/**/*.rs`: any file that
  constructs a `Finding` via a literal `rule_id: "..."` must also mention `disabled_rules` somewhere in the
  same file (native findings are built in code, so there is no single declarative `message` field to
  inspect precisely the way the DSL check above can — see the test's own doc comment for exactly what this
  proxy can and cannot prove).
- **Id hygiene** — DSL pack ids are unique across packs, rule ids are unique within a pack, and no DSL
  `"pack"` or `"pack/rule"` id collides with a native analysis id (all three id shapes share one
  `disabled_rules`/`suppressions` string-match space — see `crates/core/src/registry.rs::is_enabled`).
- **Catalog sync** — [catalog.md](catalog.md)'s totals sentence (`N DSL packs, N DSL rules, N native
  analysis ids`) matches what `load_dsl_packs`/`register_native_analyses` actually load, and every native
  analysis id / DSL pack id appears somewhere in the catalog's text.
- **Determinism guard** — loading `rules/dsl` twice yields identical pack data in identical order (a cheap
  regression net for map/directory-iteration-order bugs in pack parsing).

## Recurring defect classes — checklist for every new rule

Successive review rounds kept re-finding the same two defect classes under different rule names, because
each fix was applied to one sampled rule instead of the whole class. A whole-catalog sweep fixed the backlog
and turned the underlying judgment calls into a checklist every new `line-scan`/`method-scan` rule must run
through before it ships:

1. **Can this pattern match inside a comment?** For a keyword/call-shaped `line_pattern`/`patterns` regex,
   the answer is almost always yes — a JSDoc example, an ESLint-disable comment naming the rule, prose
   mentioning the keyword, or commented-out old code all read as ordinary source text to a regex. Set
   `"skip_comment_lines": true` unless the rule is deliberately inspecting comment/annotation *content*
   itself (no rule in the current packs does this — a hypothetical TODO-marker rule would be the shape of
   exception that qualifies; a Java `@Annotation(...)` pattern does NOT qualify as an exception, since an
   annotation is code, not a comment, and turning the flag on only filters lines that are genuinely
   comments). `skip_comment_lines` skips a line whose trimmed text starts with `//`, `/*`, or `*` for BOTH
   `line-scan` (whole-line matching) and `method-scan` (per-line within the symbol span, including `absent`
   guard checks) — safe to enable by default because it can only remove comment-only false matches, never a
   real code-line match.
2. **Is this rule about deployed surface or repo content?** Most rules reason about what the application
   *does* at runtime (a missing `await`, a wildcard CORS origin, an unbounded query) — call this
   **deployed-surface**: a test file exercising the same code shape isn't a production bug, so exclude test
   paths. A minority of rules reason about a literal value simply being *present in the repo*
   (`be-security/hardcoded-secret`, `be-security/hardcoded-password`) — call this **repo-content**: a
   secret committed inside a test fixture is still a leaked credential the moment it's pushed, so these must
   scan every path, test directories included. Decide which one a new rule is, and for deployed-surface
   rules add the shared canonical test-path exclude (copy it verbatim, do not invent a new regex):
   ```
   "file_exclude_pattern": "(?i)((^|/)(e2e|tests?|__tests?__|spec|fixtures?)/|\\.(test|spec)\\.|\\.stories\\.|(^|/)\\.storybook/|(^|/)(playwright|vitest|jest|cypress)\\.config\\.)"
   ```
   This is the same string `be-reliability/debug-true-committed` and `fullstack/localhost-egress-committed`
   already used before the sweep unified every other deployed-surface DSL rule onto it. If a rule already
   has a `file_exclude_pattern` for an unrelated reason (e.g. `be-reliability/process-exit-in-lib` excludes
   `scripts?/tools/bin` as CLI-entrypoint dirs), leave that alone rather than conflating two different
   exclude reasons into one regex. (`be-reliability/env-outside-config` is a deliberate exception: it
   excludes config-file basenames AND folds in the canonical test-path/`scripts/` exclusion, documented
   in its own `message` — see that rule for the reasoning.)

   Adversarial review on a large real monorepo closed three more gaps in the canonical string: NestJS
   `*.e2e-spec.ts` files (the old `\.(test|spec)\.` alternative requires a literal `.spec.`, which an
   `-spec.` hyphen separator doesn't produce), `packages/testing/` helper directories, and `vite.config.*`
   (the tool-config alternation had vitest/jest/playwright/cypress but not vite). The canonical string is
   now:
   ```
   "file_exclude_pattern": "(?i)((^|/)(e2e|tests?|__tests?__|spec|fixtures?|testing)/|\\.(test|spec)\\.|\\.stories\\.|[.-]spec\\.|(^|/)\\.storybook/|(^|/)(playwright|vitest|jest|cypress|vite)\\.config\\.)"
   ```
3. **Does the message carry problem + fix + suppress?** Every DSL rule's `message` must explain what's wrong,
   how to fix it, and name its own `suppress_marker` — already machine-enforced by the "Message triple" check
   above, but worth checking by eye while drafting: a reviewer should never have to guess how to vet a
   false positive.
4. **Does the message make a claim about what the matcher does or doesn't flag?** If the message
   says "plain `as X` is not flagged", "only literal query strings are kept", "a bare `$transaction`
   still fires" — that claim is a testable contract, and an unpinned claim WILL drift (shipped
   examples: `as-cast`'s pattern matched a lone `as unknown` its message promised to skip;
   `race-condition-toctou`'s message called `$transaction` insufficient while its matcher still
   vetoed on it). Add a fixture to the pack's `.rs` test that asserts the claimed behavior — one
   positive or negative per claim, named after the claim.
5. **Is this pattern an English word that could appear in prose?** Comments are already excluded by item 1,
   but a string literal isn't — a keyword pattern that happens to be an ordinary English word (`do`, `for`,
   `while`, `update`, `delete`, `select`, etc.) will also match that same word sitting inside prose text
   (`"logged in to do this"` matches a bare `\bdo\b`; `"waiting for ${x}"` matches a bare `\bfor\b`). Require
   an adjacent syntax anchor — a `(`, `{`, a wrapping quote, etc. — immediately before/after the word in the
   same alternative (`\bdo\s*\{`, not bare `\bdo\b`; `"..."` wrapping `SELECT`/`UPDATE`, not a bare
   `\bUPDATE\b`), never a bare word alone. Machine-checked by the `rule_contracts` meta-test's
   `dangerous_bare_words_are_syntax_anchored_not_bare_prose_matches` test (see that test's own doc comment
   for the curated word list and exactly what the check can/cannot prove) — this is the fix that shipped for
   `perf/api-in-loop` (bare `\bdo\b`) and `be-security/sql-taint` (bare `UPDATE`).
6. **What is the nearest benign lookalike, and is it pinned as a negative fixture?** Before shipping,
   name the most common INNOCENT code that matches the same surface shape the rule keys on, and pin it
   as a negative test in the pack's `.rs` — not a synthetic near-miss, but the real-world idiom a scan
   of an ordinary repo will actually hit. Field-audit examples of rules that shipped without this and
   fired on their lookalike immediately: `sql/truncate-in-app-code` (SQL `TRUNCATE` vs a JSX `truncate`
   boolean prop AND Tailwind's `truncate` utility class), `be-security/private-key-committed` (a PEM
   header carrying a key vs a doc/i18n sentence merely *naming* the header),
   `be-reliability/sync-fs-in-handler` (Express's `res` vs `const res = await fetch(...)`), and
   `perf/api-in-loop` (a request-per-iteration loop vs the universal single-fetch-then-`.map()`
   response transform). A positive fixture proves the rule CAN fire; only the benign-lookalike negative
   proves it knows when NOT to.
7. **Does the claim need structure the matcher doesn't have?** `line-scan`/`method-scan` see text
   co-occurrence within a span — they cannot see containment (X *inside* a loop), order (X *then* Y), or
   dataflow (X *flows into* Y). If the rule's value depends on such a relation, either (a) use a
   structural fact the parser projects (e.g. `MethodScan::trigger_in_loop` over `loop_spans`, the fix
   that replaced `perf/api-in-loop`/`sql/nplus1`/`sql/count-in-loop`'s loop-token co-occurrence after a
   field audit found 11/11 false positives), or (b) keep the co-occurrence matcher but make the message
   SAY co-occurrence, in the `be-db/multi-write-no-tx` house style ("This is a co-occurrence heuristic,
   not proof ..."), and cap severity at `warning` — `critical` is reserved for matchers that PROVE their
   claim (a closed literal, an unambiguous token). Never ship a structural claim on a textual matcher.
