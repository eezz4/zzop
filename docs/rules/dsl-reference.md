# DSL rule pack reference

Normative schema for `rules/dsl/*.json`. Source of truth: `crates/core/src/dsl.rs` (interpreter) and
`crates/core/src/pack_loader.rs` (loader/schema-version gate). Every field below is read directly from
those files — if they diverge, the Rust source wins.

See also: [authoring-guide.md](authoring-guide.md) (how to write a pack), [catalog.md](catalog.md) (what
ships today). A machine-readable JSON Schema for this shape ships at
[../contracts/rule-pack.schema.json](../contracts/rule-pack.schema.json), and `zzop pack validate
<path>` (CLI) / the `validate_rule_pack` MCP tool check a pack file against the loader's own load-time
judgments — structure only, never rule quality — before you ship it.

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
| `framework` | string | `"any"` | Declared target environment (`"any"` \| `"react"` \| `"prisma"` \| ...). Currently informational: it is carried on `RuleMeta.framework`, but no engine code path filters on it today — `RuleMeta::applies_to` (crates/core/src/registry.rs) ignores its target argument and gates only on `enabled`. The per-file pre-filter that does run is path-based (`pack_loader::applies_to`, over each rule's `file_pattern`), not framework-based. |
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
malformed JSON body or an unsupported `schema_version`, never a rule that silently never fires. `zzop pack
validate`/the `validate_rule_pack` MCP tool surface either as an ordinary issue.

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
| `suppress_marker` | string \| null | `null` | Inline ok-marker name — see [Suppress-marker semantics](#suppress-marker-semantics). |

**Do not hand-write a disable hint in `message`.** At runtime the engine appends one more sentence to
every DSL finding's `message`, after whatever you write: `` Disable via config `rules: { "<pack>/<rule>": "off" }` (embedders: `disabled_rules`) `` (`zzop_core::disable_hint`, appended by
`crates/engine/src/pipeline/findings.rs::append_disable_hints`) — the exact same fragment native findings
carry, built from the one shared helper. Write the cause, the fix, and your rule's own `suppress_marker`
name in `message`; a hand-written "disable via config ..." sentence renders TWICE. See
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
| `snippet_max` | usize | `160` | Truncates the reported snippet (chars, after `line.trim()`). |

`LabeledPattern`: `{ "pattern": "<regex>", "label": "<string>" }`.

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

`RuleDef.suppress_marker` (e.g. `"n+1-ok"`) applies to `line-scan`, `method-scan`, AND `io-scan` findings
(not `symbol-scan`, which still has no source-line concept to anchor a comment against):

- A `line-scan`/`method-scan` finding is suppressed when a `//`-comment naming the marker appears on the
  finding's **own line, or the single line directly above it** — a fixed 1-line lookback window used
  uniformly across every pack. A wider lookback window over-suppresses: a marker aimed at one call can
  silently suppress unrelated, unvetted findings on the lines below it. Place the marker on the finding's
  own line, or directly above it — nowhere further back.
- Matches `// <marker>` or `// <marker>: <reason>` — the marker text is regex-escaped before compiling
  (`//\s*{escaped-marker}\b`), so a marker containing regex metacharacters (`n+1-ok`'s `+`) matches
  literally, not as regex syntax.
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
