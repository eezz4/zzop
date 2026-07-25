# DSL rule pack reference

Normative schema for `rules/dsl/*.json`. Source of truth: `crates/core/src/dsl.rs` (interpreter) and
`crates/core/src/pack_loader.rs` (loader/schema-version gate). Every field below is read directly from
those files — if they diverge, the Rust source wins.

See also: [authoring-guide.md](authoring-guide.md) (how to write a pack), [catalog.md](catalog.md) (what
ships today). A machine-readable JSON Schema for this shape ships at
[../contracts/rule-pack.schema.json](../contracts/rule-pack.schema.json), and `zzop validate-rule-pack
<path>` (CLI) / the `validate_rule_pack` MCP tool check a pack file against the loader's own load-time
judgments — structure only, never rule quality — before you ship it. That includes the dead-rule
census: a matcher regex that does not compile, and the two structural shapes that parse fine and can
never fire (a line-scan declaring neither `line_pattern` nor `any`; a method-scan whose `trigger`
names a label no `patterns` entry declares).

## Pack shape (`RulePackDef`)

```json
{
  "id": "sql",
  "framework": "any",
  "schema_version": 1,
  "fragments": { "sql-where-veto": "(?i)\\bWHERE\\b|\\$\\{|\\+\\s*[\"'`]|[\"'`]\\s*\\+" },
  "rules": [ /* RuleDef[] */ ]
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | — | Pack id; a finding's `rule_id` is `"{id}/{rule.id}"`. |
| `framework` | string | `"any"` | Declared target environment (`"any"` \| `"react"` \| `"prisma"` \| ...). Currently informational: it is parsed and carried on the loaded pack, but no engine code path filters on it today — nothing in the engine decides "what target is this tree", so there is no target gating for it to feed. The per-file pre-filter that does run is path-based (`pack_loader::applies_to`, over each rule's `file_pattern`), not framework-based. |
| `schema_version` | u32 | `1` | DSL schema this pack was authored against — see [Schema version policy](#schema-version-policy). |
| `fragments` | `{ name: regex }` | `{}` | Named regex fragments this pack can reference by `${NAME}` — see [Fragments (`${NAME}` references)](#fragments-name-references). |
| `rules` | `RuleDef[]` | — | The pack's rules. |

## Fragments (`${NAME}` references)

Every DSL regex idiom that recurs across many rules — most visibly the test-path `file_exclude_pattern`
duplicated in ~90 rules across 11 packs before this mechanism existed — can be factored into a named
fragment and referenced instead of copy-pasted, so a single fix (or a deliberate per-pack override) lands
in one place.

**Reference syntax.** A pattern-bearing field (`file_pattern`, `file_exclude_pattern`, `require_file`,
each `require_file_all`/`require_file_absent` entry, `line_pattern`, each `any[].pattern`,
`exclude_pattern`, each `patterns[].pattern`/`absent[].pattern`, `name_pattern`, `key_pattern`) may be
spelled as a whole string EXACTLY `${NAME}` — no other characters before or after — instead of a literal
regex. This is deliberately **whole-value only**: `"foo ${bar} baz"` is an ordinary literal string, not a
reference; there is no inline substring composition in this pass. `${NAME}` is collision-safe as a
sentinel because under the `regex` crate's syntax a bare `{` is only valid as a numeric repetition
quantifier (`{n}`/`{n,}`/`{n,m}`, digits only) — a fragment name is a kebab-case identifier, never
all-digits, so `${NAME}` can never simultaneously be a value a pack author would hand-write as a real
pattern AND compile as one. A committed test
(`crates/core/src/dsl/tests_fragments/byte_identity.rs::no_shipped_pattern_contains_the_sentinel_except_as_an_intended_whole_value_ref`)
asserts no shipped `rules/dsl/**` pattern contains `${` except as a complete, resolvable reference.

**Where names resolve from.** A `${NAME}` reference resolves against this pack's own `fragments` map
merged UNDER a SHARED bundled set the engine ships (`zzop_core::dsl::fragments::shared_fragments` — an
`include_str!`-embedded `{name: regex}` JSON, resolved identically whether the pack arrives from a
`packsDir` file or an inline `packDefs` entry, with zero filesystem dependency at runtime). A name declared
in a pack's own `fragments` WINS a collision against a shared fragment of the same name, so a pack can
locally override a shared idiom without renaming it. Today's shared set: `test-paths` and
`test-paths-stories` (the two DISTINCT test-path `file_exclude_pattern` strings shipped packs actually use
— see the note below on why they were never unified).

**Expand-then-clear.** `RulePackDef::expand_fragments` resolves every reference, then clears the pack's own
`fragments` map to empty — so the loaded, in-memory `RulePackDef` for a pack authored with fragments is
byte-identical (same `Debug` output, same hash, same cache fingerprint, same findings) to the equivalent
pack authored with every pattern spelled out inline. This runs at every `RulePackDef` deserialize boundary
— `pack_loader::parse_dsl_pack` (disk load, the `validate_rule_pack` validator, and bundled-pack parsing
all funnel through it) and the inline `packDefs` wire path — BEFORE the pack is hashed or evaluated, so
fragments never reach the DSL interpreter or the cache fingerprint.

**Errors, not silent passthrough.** Resolution is single-pass, not recursive: a fragment whose own value is
itself a whole-value `${...}` reference is a hard load error (`FragmentError::Nested`), never a silent
no-op or a chained expansion. An unknown fragment name (`${typo}` naming nothing in either the pack's own
`fragments` or the shared set) is likewise a hard load error (`FragmentError::Unknown`) — exactly like a
malformed JSON body or an unsupported `schema_version`, never a rule that silently never fires. `zzop
validate-rule-pack`/the `validate_rule_pack` MCP tool surface either as an ordinary issue.

**Why two near-identical test-path fragments, not one.** `test-paths` and `test-paths-stories` differ only
in whether they also exclude `.stories.`/`.storybook/` files — a real, pre-existing behavioral split across
shipped rules (some rules intentionally still scan Storybook files, some don't). Unifying them would be a
silent behavior change (a rule that used to scan a `.stories.tsx` file would stop, or vice versa), so each
rule references whichever of the two fragments matches its OWN pre-existing string — migration only
factored out the duplication, it never changed which files any rule scans.

## Rule shape (`RuleDef`)

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | — | Rule id within the pack. |
| `severity` | `"critical"` \| `"warning"` \| `"info"` | — | Default severity (overridable per-id via `RuleConfig::severity_overrides`). |
| `message` | string | — | Human-facing cause/fix-hint, copied verbatim into every finding — but NOT the whole of what ships: the engine auto-appends a disable hint at runtime (see the note right below this table). |
| `matcher` | `Matcher` | — | One of the four matcher shapes below (`type` tag, kebab-case). |

There is **no `suppress_marker` field** — the inline ok-marker is DERIVED as `<id>-ok`
(`RuleDef::suppress_marker()`), so it is never authored or stored. See
[Suppress-marker semantics](#suppress-marker-semantics).

**Do not hand-write a disable hint in `message`.** At runtime the engine appends one more sentence to
every DSL finding's `message`, after whatever you write: `` Disable via config `rules: { "<pack>/<rule>": "off" }` (embedders: `disabled_rules`) `` (`zzop_core::disable_hint`, appended by
`crates/engine/src/pipeline/findings.rs::append_disable_hints`) — the exact same fragment native findings
carry, built from the one shared helper. Write the cause, the fix, and your rule's own derived `<id>-ok`
marker in `message`; a hand-written "disable via config ..." sentence renders TWICE. See
[authoring-guide.md](authoring-guide.md#the-auto-appended-disable-hint) for the full contract.

## Matchers

`Matcher` is tagged on `"type"` (kebab-case): `line-scan`, `method-scan`, `symbol-scan`, `io-scan`.
Whole-graph / cross-file queries are out of scope for all four — see
[authoring-guide.md#when-a-rule-does-not-fit-the-dsl](authoring-guide.md#when-a-rule-does-not-fit-the-dsl).

### `line-scan` (`LineScan`)

Per-line regex scan over a file's raw text — the DSL's lexical matcher.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file_pattern` | regex | required | Path regex a file must match (e.g. `"(?i)\\.(java\|jsp\|jspx\|tag)$"`). |
| `file_exclude_pattern` | regex \| null | `null` | Path regex — a file whose `rel` path matches this is skipped entirely, checked immediately after `file_pattern`. Exists because `file_pattern` is positive-only and the `regex` crate has no lookaround, so `file_pattern` alone cannot express "match this extension but NOT under `scripts/`" — see [Path-exclusion semantics](#path-exclusion-semantics). |
| `require_file` | regex \| null | `null` | Cheap pre-skip: the rule only scans a file whose full text matches this regex. Absent = always scan. |
| `require_file_all` | regex[] | `[]` | Additional pre-skip regexes, **all** of which must match the file text, evaluated in order, short-circuiting on the first miss. Order rare-token-first — see the [authoring guide's performance section](authoring-guide.md#performance-require_filerequire_file_all-rare-token-first). |
| `require_file_absent` | regex[] | `[]` | Negated mirror of `require_file_all`, evaluated right after it: if **any** of these regexes matches the whole file text, the rule skips that file entirely. Encodes "flag X only when there is no Y anywhere in the file" (e.g. `setInterval` with no `clearInterval` anywhere in the same file) — a shape `exclude_pattern` cannot express, since that field only vetoes the matching *line*, not the whole file. |
| `skip_comment_lines` | bool | `false` | Skip lines whose `trim_start()` begins with `//`, `*`, or `/*`. |
| `line_pattern` | regex \| null | `null` | Single flag regex — mutually exclusive with `any` (see below). |
| `any` | `LabeledPattern[]` \| null | `null` | Labeled alternatives; **first match per line wins**, its `label` becomes `data.label`. Takes precedence over `line_pattern` when both are present. |
| `exclude_pattern` | regex \| null | `null` | A line that matches the main pattern is skipped entirely when it **also** matches this regex (e.g. excluding `^\s*import` lines from an `as`-cast scan). |
| `attr_present` | string \| null | `null` | Attribute gate — plain attribute name, **not** a regex and **not** fragment-expanded. Keeps a finding only when this file carries a truthy attribute of that name. See [Attribute gates](#attribute-gates-consuming-a-declaration). |
| `attr_absent` | string \| null | `null` | Same lookup, inverted: keeps a finding only when the file carries **no** truthy value for the attribute. |
| `require_attr_declared` | string \| null | `null` | The rule runs only when that attribute name is declared *somewhere* in the analysis. Nothing declared = the rule emits nothing and the run's `warnings` say so. |
| `snippet_max` | usize | `160` | Truncates the reported snippet (chars, after `line.trim()`). |

`LabeledPattern`: `{ "pattern": "<regex>", "label": "<string>" }`.

A `label` is an identification tag, not a description: the bundled packs keep every label to
`^[a-z0-9]+(-[a-z0-9]+)*$` (`ecb-mode`, not `ECB mode (no diffusion)`) — explanation is the rule's
`message`, which already carries it and would otherwise rot in two places. In `any[]` the label also ships
to consumers verbatim as `data.label`, where it is the only stable "which alternative fired" key a
multi-alternative rule has (`snippet` is raw source text); in `patterns[]`/`absent[]` it is an identifier
`trigger`/`after` resolve by exact string. Scope is rule-local — labels are never typed by a user and are
free to repeat across unrelated rules. This is a house convention machine-checked over the bundled packs
only (`crates/engine/tests/rule_contracts/`'s `dsl_pattern_labels_are_kebab_case`); the pack schema does
not constrain the value, so a third-party pack is free to label differently.

If neither `any` nor `line_pattern` is set, or any regex fails to compile, the rule is skipped
(zero findings) rather than erroring the whole pack.

### `method-scan` (`MethodScan`)

Multi-pattern co-occurrence within a symbol's body span — the DSL's "these patterns appear together in
one function" matcher (e.g. `Runtime.exec` + string concatenation in the same method).

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file_pattern` | regex | required | Path regex. |
| `file_exclude_pattern` | regex \| null | `null` | Same path-negation escape hatch as line-scan's `file_exclude_pattern`, checked immediately after `file_pattern` — see [Path-exclusion semantics](#path-exclusion-semantics). |
| `require_file` | regex \| null | `null` | Same cheap pre-skip as line-scan. |
| `require_file_all` | regex[] | `[]` | Same AND pre-skip as line-scan. |
| `require_file_absent` | regex[] | `[]` | Negated mirror of `require_file_all` — same semantics as line-scan's `require_file_absent`: if **any** of these regexes matches the whole file text, the rule skips that file entirely (e.g. skip a `process.exit(...)` finding in a file that also registers a `process.on('SIG...` signal handler, since a dedicated signal-handling module legitimately calls `process.exit`). |
| `skip_comment_lines` | bool | `false` | Skip comment lines when testing patterns (span-scoped). |
| `patterns` | `LabeledPattern[]` | required | **All** must each match at least one line inside a symbol's span (lines don't need to share a line — "co-occurrence", not "one regex"). |
| `trigger` | string | required | Must equal one `patterns[].label`; that pattern's first (top-down) match anchors the finding's `line`/snippet. A `trigger` naming no real label makes the rule malformed → skipped. |
| `trigger_in_loop` | bool | `false` | Structural containment gate on the trigger pattern only: when `true`, a trigger-pattern line match counts (for both satisfaction and the finding's line) only if that line falls within one of the file's projected `loop_spans` (see below) — i.e. the call is textually INSIDE a loop statement or an array-iteration callback body, not merely co-occurring with loop tokens somewhere in the same function. Non-trigger `patterns`/`absent` entries are unaffected. A file with no projected loop spans can never satisfy the trigger, so the rule is silent there — same graceful-degrade policy as a file with no symbol spans. |
| `after` | string \| null | `null` | Lexical-ORDER gate on the trigger pattern: names another `patterns[].label` that must already have matched **before** a trigger match counts (for both satisfaction and the finding's line). "Before" = an earlier line in the same span, or — on the **same** line — the ordering pattern's FIRST match starting before the trigger pattern's FIRST match. Only those two first-match offsets are compared, never every match: `p.then(r => setX(r))` counts, `setX(v); await f();` does not, and `setX(a); await f(); setY(b);` on one line does NOT count either, because the trigger's first match (`setX`) precedes the boundary even though `setY` follows it (an accepted under-report on dense one-liners). Proves lexical order in the source text, **not** execution order — a trigger in an `else` branch, or one that follows a boundary which only runs conditionally, still counts; a rule using it should say "lexically after". Naming a label no `patterns` entry declares is malformed → the rule is skipped. |
| `after_in_same_function` | bool | `false` | Structural PAIRING gate on `after` (no-op without it): the ordering match must fall **inside** the innermost `function_spans` entry (see below) that contains the trigger — not merely inside the same symbol body span. CONTAINMENT, not span identity: the test is "ordering line ≥ that entry's START line" (it is already at or before the trigger line, which is inside the entry). Identity would be wrong — a line holding both an outer `await` and a merged continuation callback (`await import(m).then((x) => x.f());`) resolves innermost to the callback, which would hide the enclosing function's own boundary from the setter on the next line. Exists because a method-scan span is a *declared symbol's* body — a React component's whole function — so `after` otherwise pairs a setter in one closure with an `await` in an unrelated **sibling** closure. A file with no projected `function_spans` degrades to a **no-op** (every line resolves to "no enclosing function", so all lines count as the same one), leaving the rule's pre-gate behavior intact — the opposite direction from `trigger_in_loop`, and deliberate: this gate only ever *removes* pairings, so "remove nothing" preserves coverage instead of deleting it. |
| `absent` | `LabeledPattern[]` | `[]` | Veto patterns: after every `patterns` entry is satisfied, the finding is dropped if **any** of these also matches a line in the **same span** (encodes "a guard makes this not a violation" — e.g. a `try {` wrapping a read-then-write, or a `$transaction(` wrapper). |
| `snippet_max` | usize | `160` | Same as line-scan. |

Span semantics:
- Spans come from `SourceFile.symbols` (`SourceSymbol.body_start`/`body_end`, **1-based, inclusive**),
  projected by the parser during the same parse pass — never re-derived by the DSL interpreter.
- **Files without spans are silently skipped** for this matcher (no parser support for the file's
  language, or a lexical fallback after a parse failure / oversized file) — `symbols` is simply empty;
  line-scan rules in the same pack still run against that file.
- **Innermost-span priority**: when a file's symbol spans overlap (a class symbol's span strictly
  contains a method sub-symbol's span), only the innermost (leaf) span is evaluated — the outer span is
  dropped whenever another candidate span's range is strictly contained within it. This prevents a
  double-count from a naive "scan every symbol" pass (class span + method span both firing for the same
  evidence). Computed per rule invocation, O(n²) over one file's (small) symbol list.
- Before per-span evaluation, a whole-file necessary-condition pre-skip applies: every `patterns` entry
  must match *somewhere* in the file's full text, or the file is skipped entirely (a strict subsumption
  of the per-span check — see the [authoring guide](authoring-guide.md#performance-require_filerequire_file_all-rare-token-first) for why this mattered for a real hotspot).
- A symbol with no body span (e.g. a `type`/`interface`, or a parser that couldn't project one) is not
  scannable and is skipped.
- **Loop spans** (`trigger_in_loop`'s substrate): alongside `symbols`, the parser projects each file's
  `loop_spans` — 1-based, inclusive line ranges covering every `for`/`for-of`/`for-in`/`while`/`do-while`
  statement (header line included) plus the callback-argument span of an array-iteration call
  (`.map`/`.forEach`/`.filter`/`.reduce`/...; the callback body only, not the whole call expression). Line
  ranges, not byte offsets — a trigger match sharing a line with a loop span's line counts as contained
  even if it is, byte-wise, outside the loop (e.g. a receiver expression on the same line as a one-line
  `.map()` callback). Empty when the parser has no support / falls back lexically, same graceful-degrade
  policy as `symbols`.
- **Function spans** (`after_in_same_function`'s substrate): the parser also projects each file's
  `function_spans` — 1-based, inclusive line ranges, one per function-like node (declaration,
  expression, arrow, class/object method, constructor, accessor). Nested functions overlap; the gate
  resolves the **innermost** span containing a line (greatest start; ties by smallest end, then first
  emitted). One merge is applied at projection time: a function-shaped ARGUMENT of a
  `.then(...)`/`.catch(...)`/`.finally(...)` member call has its span **start** pulled up to that call's
  property-token line, so a promise continuation shares a span with the boundary that schedules it.
  The merge only changes anything when the callback OPENS ON A LATER LINE than the `.then` token — the
  formatting a printer produces once the argument list breaks:

  ```ts
  loadRates().then(     // line 1 — the boundary token
    (d) => {            // line 2 — the callback's own start
      setRates(d);      // line 3
    },
  );
  ```
  Projected span: `[1, 5]`, not `[2, 5]`. Unmerged, `innermost(3)` would start at line 2 and the
  boundary on line 1 would fall outside it, so the pair breaks. In the common one-line spelling
  (`loadRates().then((d) => {`) the token and the callback already share a line and the merge is a
  no-op. Nothing else merges (`.map`, `setTimeout`, `useEffect`, a bare or aliased `then`). Empty when
  the parser has no support — see the field's row above for why the degrade is a no-op rather than
  silence. **TypeScript only today** (`loop_spans` is TypeScript + Go); the coverage matrix lives in
  [`NORMALIZED_AST.md`](../NORMALIZED_AST.md).

  The no-op degrade is decided **per line, not per file**, and that is the contract, not an accident: a
  trigger line inside *no* projected span means "no gate on this line", never "no pair here". A missing
  span is absence of evidence, and this gate only ever deletes pairings the projection *proves* sit in
  different functions — so a silent projection re-admits the pre-gate scope instead of deleting a
  finding on the strength of a fact nobody has. This is reachable with spans **present**: a class body's
  own top level. A class's body span is the scanned unit whenever the class declares no
  method/constructor (a component written purely with property initializers), and a property-initializer
  line there sits inside no function span — so a setter on it still pairs with an `await` from a sibling
  arrow property. An external parser that projects `function_spans` only partially lands in the same
  place.

### `symbol-scan` (`SymbolScan`)

Query over a file's declared symbols (functions/classes/consts/types/interfaces) — for naming-convention
/ banned-export rules line-scan can't express reliably.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file_pattern` | regex | required | Path regex. |
| `kind` | `SourceSymbolKind` \| null | `null` | Restrict to one of `function`, `class`, `const`, `type`, `interface`. |
| `name_pattern` | regex \| null | `null` | Regex on the symbol name — meaning flips under `negate` (below). |
| `exported` | bool \| null | `null` | Restrict to exported (`true`) or non-exported (`false`) symbols. |
| `negate` | bool | `false` | See below. |

All set filters combine with AND. `negate` changes only what `name_pattern` means:
- `negate: false` (default): a symbol must **match** `name_pattern` to fire — "flag names matching this
  banned pattern".
- `negate: true`: a symbol must **not** match `name_pattern` to fire — "flag exported functions NOT
  matching our naming convention".
- `negate: true` with no `name_pattern` set has nothing to negate against, so every symbol passes that
  filter (`kind`/`exported` still apply) — documented behavior, not a rejected configuration: a malformed
  but harmless rule degrades to a plain AND filter rather than producing zero findings unexpectedly.

Finding `data.snippet` is the symbol's name; `line` is the symbol's declaration line.

### `io-scan` (`IoScan`)

Query over the WHOLE TREE's IO facts — evaluated once, post-assemble, over every `IoProvide`/`IoConsume`
the assembled tree carries plus the tree's `AttributeStore`, NOT per file: this is what lets a rule see
facts a single file's raw extraction never has on its own — router-mount/controller-prefix/file-convention
composition, and Java/C#'s whole-corpus passes — for boundary-convention rules (e.g. "every HTTP endpoint
must be versioned under `/api/v[0-9]+/`").

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file_pattern` | regex | required | Path regex — required here too, even though `IoFacts` isn't itself file-shaped, so a matcher still opts into which files it considers. |
| `file_exclude_pattern` | regex \| null | `null` | Path regex — an entry whose `file` matches this is skipped entirely, checked right after `file_pattern` (cheapest gate first, before any attribute lookup or anchor-text fetch). Same escape-hatch rationale as `line-scan`'s field of the same name. |
| `direction` | `"provides"` \| `"consumes"` \| `"any"` | required | Which side(s) of `IoFacts` to scan. |
| `kind` | `IoKind` \| null | `null` | Exact match against an entry's `kind` (e.g. `"http"`, `"db-table"`). |
| `key_pattern` | regex \| null | `null` | Regex on the entry's normalized key — meaning flips under `negate`, same convention as `symbol-scan`. |
| `negate` | bool | `false` | See below. |
| `symbol_pattern` | regex \| null | `null` | Regex on `IoProvide::symbol` — provides-only: a consume never carries a symbol, so it never matches when this is set, and a provide whose `symbol` is unresolved (`None`) never matches either (never-guess). Unlike `key_pattern`, `negate` never flips this field's role. |
| `attr_present` | string \| null | `null` | Plain string, not a regex. Fires only when the tree's `AttributeStore` has a truthy value for `route_attr(entry.kind, entry.key, attr_present)` — an exact `IoKey` match wins over the longest covering `PathScope`. An entry with no resolved key never satisfies this gate. |
| `attr_absent` | string \| null | `null` | Same `route_attr` lookup as `attr_present`, inverted: fires only when there is NO truthy value for the attribute. An entry with no resolved key has nothing to look up, so it always satisfies this gate. |
| `anchor_exclude_pattern` | regex \| null | `null` | Regex against the entry's own source line, fetched via the tree context's anchor-line lookup. Inapplicable when no source text is reachable (e.g. envelope mode has no native source) — the exclusion then simply never applies, never a guessed match. |

- `negate: false`: fires on entries whose key matches `key_pattern`.
- `negate: true`: fires on entries whose key does **not** match `key_pattern` — the "endpoints not under
  `/api/v<N>/`" use case.
- An entry with `key: None` (the adapter couldn't statically resolve it — e.g. a dynamic fetch target)
  never counts as matching `key_pattern`. Under `negate: true` that makes it a hit (an unresolved
  consume is not proven to follow the convention); under `negate: false` it never fires.
- When `key_pattern` is absent entirely, every entry matches (so `negate: true` with no `key_pattern`
  yields no findings — nothing to fail — same "nothing to negate against" convention as `symbol-scan`).
- `symbol_pattern`/`attr_present`/`attr_absent`/`anchor_exclude_pattern` are plain additive AND gates,
  evaluated after `negate` has already resolved `key_pattern`'s role — `negate` itself only ever flips
  `key_pattern`, never these four.

A file that contributes no `IoProvide`/`IoConsume` to the assembled tree simply supplies no entries here
— there is no separate per-file skip step to speak of (unlike method-scan's per-file `symbols` walk):
io-scan iterates the tree's already-assembled `provides` then `consumes` lists directly, each in input
order (the determinism contract). Finding `data` is `{ "snippet": <key or "<unresolved>">, "kind": <kind> }`;
`line` is the entry's own line.

## Attribute gates: consuming a declaration

Some rules rest on a fact the source cannot state — which directory is the config module, which tree is
generated, which routes a middleware guards. Inferring such a fact from names or code shape is guessing,
and a guess that is right half the time is worse than a rule that says it does not know. The attribute
gates let a rule read the project's own **declaration** instead.

The declaration channel is the one every cross-cutting fact already uses: an overlay's per-file
`attributes` (`overlays: [...]` in `zzop.config.jsonc`; `adapterOverlays` for embedders), each entry
`{"target": ..., "key": "<name>", "value": <json>}`. `examples/auth-overlay-adapter` is a working one.

| Matcher | Gate is looked up against | Resolution |
|---|---|---|
| `line-scan` | the scanned file's repo-relative path | exact `{"file": {"path": ...}}` wins, else the longest covering `{"pathScope": {"prefix": ...}}` |
| `io-scan` | the io entry's `(kind, key)` | exact `{"ioKey": ...}` wins, else the longest covering `{"pathScope": {"prefix": ...}}` (route paths) |

A value is "present" when it is truthy — `null`/`false`/`0`/`""` count as absent, so a narrow explicit
`false` can carve one file (or route) out of a scope declared wholesale.

**`require_attr_declared` and the silence it buys.** `attr_absent` alone is inert when nobody declares
the name: every file trivially lacks it, so the rule fires everywhere — which is exactly the guess the
declaration was meant to replace. That is fine for an attribute the *engine* mints (io-scan's
`auth-guarded`: a tree where nothing is guarded really is a tree of unguarded routes) and wrong for one a
*user* declares. `require_attr_declared` closes it: with nothing declared the rule emits nothing, and the
run's `warnings` disclose which rule did not run, which name it needed, how many candidate sites went
unjudged, and how to declare one. A rule with no candidate sites stays quiet — `0` there is a real `0`.

**Where the gate runs, and what that constrains.** Line-scan attribute gates are a whole-tree
**post-filter** applied after the per-file pass, not a check inside it. A per-file finding is cached under
`(content hash, parser fingerprint, scope, ruleset fingerprint)` and the attribute set is none of those:
gating inside the cached unit would freeze a declaration into entries that outlive it, so editing the
declaration would leave stale findings behind. The cache therefore stores ungated findings and the gate is
recomputed every run — the same placement `severityOverrides`/`suppressions` use. Two consequences for a
pack author: an attribute gate can only ever **remove** findings, and its predicate is **whole-file**. A
gate that had to change which lines match could not live here.

## Path-exclusion semantics

`file_exclude_pattern` (on `line-scan` and `method-scan`) exists for one reason: `file_pattern` is
positive-only — one regex naming the files a rule scans — and the `regex` crate (used everywhere in this
DSL) does not support lookaround/lookbehind, so there is no way to write a single `file_pattern` that
means "match this extension, but not under `scripts/`" or "match this extension, but not a `*.test.ts`
file". `file_exclude_pattern` is that escape hatch: a second, independent regex against the same `rel`
path, checked immediately after `file_pattern` passes and before `require_file`/`require_file_all`/the
per-line or per-symbol scan — a match skips the file entirely for that rule. Like every other regex field
in the DSL, a `file_exclude_pattern` that fails to compile skips the whole rule (zero findings), not just
the exclusion.

## Suppress-marker semantics

The inline ok-marker is DERIVED from the rule id — `RuleDef::suppress_marker()` returns `<id>-ok` (rule
`float-money-compare` → `float-money-compare-ok`). It is not authored or stored, so it can never drift out
of the `-ok` convention and is always predictable from the RULE id — note that a finding carries the
PACK-QUALIFIED id, and the marker strips that prefix: `security/hardcoded-secret` suppresses on
`// hardcoded-secret-ok`, never `// security/hardcoded-secret-ok` (the marker regex anchors right after
`//`, so the prefixed form silently matches nothing). It applies to
`line-scan`, `method-scan`, AND `io-scan` findings (not `symbol-scan`, which still has no source-line
concept to anchor a comment against):

- A `line-scan`/`method-scan` finding is suppressed when a `//`-comment naming the marker appears on the
  finding's **own line, or the single line directly above it** — a fixed 1-line lookback window used
  uniformly across every pack. A wider lookback window over-suppresses: a marker aimed at one call can
  silently suppress unrelated, unvetted findings on the lines below it. Place the marker on the finding's
  own line, or directly above it — nowhere further back.
- A comment SHAPED like a marker but not THIS rule's marker suppresses nothing — and is now NAMED in the
  finding's message alongside the marker the rule does honor, instead of failing silently. The check is
  vocabulary-free: it compares against this rule's own derived marker and nothing else, so it reports a
  plain typo (`// as-cst-ok`), another rule's marker (`// n+1-ok`), and an invented word
  (`// legacy-route-ok`) identically — it cannot and does not know which of those names a real rule.
  The accepted shape is a token over `[a-z0-9+]` with `-`-joined segments, ending in `-ok` — lowercase
  only, with `+` in the alphabet so `n+1`-style ids are recognized — standing as the FIRST token of a line
  comment (only whitespace between the leader and the token) and terminated by an ATTACHED `:` or by the
  end of the line (`// as-ok: reason`, `// as-ok`). "Line comment" means exactly the leaders that could
  have SUPPRESSED this finding in the first place: `//` for `line-scan`/`method-scan`, `--` only in a
  `.sql` file, and `//` or `#` for `io-scan` (see the three bullets below) — a leader that never had
  suppression power is never blamed for failing to use it, so a Python `# foo-ok` above a `line-scan`
  finding is silent, exactly as it is for suppression. A `-ok` word inside a sentence is never accused
  (`// half-ok for now, revisit`, `// TODO: not-ok yet`, `// NOT-ok:`), but a comment that is ONLY a
  hyphenated lowercase `-ok` word (`// half-ok`) IS reported — by shape it is indistinguishable from a
  bare marker, and a bare marker is a legal spelling, so there is nothing left to discriminate on.
  Accepted cost. Two deliberate conservative misses: `// as-ok reason` (no colon) and `// as-ok : reason`
  (detached colon) go unreported — a missed disclosure is recoverable, a false typo accusation is not.
- Matches `// <marker>` or `// <marker>: <reason>` — the marker text is regex-escaped before compiling
  (`//\s*{escaped-marker}\b`). Derived markers are always `<kebab-id>-ok` (no regex metacharacters), so the
  escaping is defensive; it stays correct even if an id ever carried a regex-special character.
- For a file whose extension is `.sql` (case-insensitive), a `--`-comment naming the marker suppresses
  identically (`-- <marker>` or `-- <marker>: <reason>`), same lookback window and escaping rules. This is
  gated to `.sql` files only, and to `line-scan`/`method-scan` only: `--` is a line comment in SQL but not
  in JS/TS (`--x` is a decrement there), so no other extension's or matcher's suppression behavior changes.
- An `io-scan` finding anchors at the matched provide/consume's own `file:line` — not a line the matcher
  scanned itself, but the entry's own source location. The marker is honored on that anchor line, or the
  single line directly above it, same 1-line lookback window as `line-scan`/`method-scan`. Recognition
  there is line-comment-**NEUTRAL**: both `// <marker>` and `# <marker>` suppress identically, since an
  io-scan anchor line can come from any provide-producing language (Python's `#` included, not just
  JS/TS/Java/Go/C#'s `//`). `--` is deliberately NOT recognized for `io-scan` — no `.sql` file produces a
  route provide, so the SQL comment dialect stays a `line-scan`/`method-scan`-only concern.
- In envelope mode (no native source text to fetch a line from), `io-scan` suppress markers are honestly
  **inactive**: the anchor-line lookup returns `None`, and a `None` result is never treated as a match —
  the finding fires unsuppressed rather than silently guessing.

## Schema version policy

- `RulePackDef.schema_version` defaults to `1` when the field is absent — every pack shipped before this
  field existed keeps loading unchanged.
- `pack_loader::SUPPORTED_DSL_SCHEMA_VERSION = 1` is the highest version this engine build understands.
  A pack declaring a **higher** version is rejected outright as a per-file `PackLoadError` (surfaced,
  never a panic — one bad/too-new pack does not take down the others in the directory).
  Older-or-equal versions always load: schema evolution is additive-only (new optional matcher fields
  with `#[serde(default)]`), so an old pack's JSON already deserializes correctly against a newer schema.
- Bump `SUPPORTED_DSL_SCHEMA_VERSION` only for a genuinely incompatible schema revision — ordinary new
  optional fields don't need it.

## RegexSet prefilter (pure optimization)

Before evaluating `line-scan` rules, the interpreter builds one `regex::RegexSet` from every `line-scan`
rule's patterns in the pack (`line_pattern` or all `any[].pattern` entries) and scans each file's lines
through it once. A rule with zero set-hits in a file is proven to find nothing under its full per-line
logic (labels, comment-skip, snippets, `require_file`) — every one of the rule's real patterns is in the
set, so this is a correctness-preserving skip, not a heuristic. It changes nothing observable: a
differential test (`prefilter_matches_unoptimized_findings_across_the_moved_java_rules`) asserts the
optimized and unoptimized paths produce byte-for-byte identical findings. `method-scan`/`symbol-scan`/
`io-scan` query different substrates (symbol spans / IO facts, not raw lines) and are not part of the set.

## Finding shape

Every matcher emits `zzop_core::finding::Finding`:

| Field | Value |
|---|---|
| `rule_id` | `"{pack.id}/{rule.id}"` |
| `severity` | The rule's `severity` (or a config override — see `RuleConfig::severity_overrides`). |
| `file` | The matched file's relative path. |
| `line` | 1-based line: the matching line (line-scan), the trigger match's absolute line (method-scan), the symbol's declaration line (symbol-scan), or the IO entry's own line (io-scan). |
| `message` | The rule's `message`, verbatim. |
| `data` | Matcher-specific JSON: `{"snippet"}` or `{"snippet","label"}` (line-scan); `{"snippet","method"}` (method-scan, `method` = the enclosing symbol's name); `{"snippet"}` = the symbol name (symbol-scan); `{"snippet","kind"}` (io-scan). |
