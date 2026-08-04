# rules/ — rule path split

Rules are split native vs dsl by **"does it need to see the whole IR graph at once?"** (the nature of the work, not
whether the rule is common or environment-specific).

## `rules/native/` — native rules (Rust crates)
- **Criterion**: whole-graph analysis (must see all nodes/trees at once) — not expressible in the DSL.
- **Form**: Rust, one crate per rule family, statically linked into the **engine** (`crates/engine`, not
  `core` — `core` stays rule-agnostic; see `crates/core/Cargo.toml` vs `crates/engine/Cargo.toml`). Full
  native speed, shares IR memory directly, oxlint-style single traversal.
- **Distribution**: bundled in the engine's prebuilds (5 platforms). Changing one requires rebuilding.
- **Examples**: `rules-graph` (circular, unreachable, dead-candidates, unimported-export), `rules-http`
  (single-tree HTTP/route rules: duplicate-route, route-shadowing, mutating-route-no-auth,
  unprovided-consume, plus the call-graph-BFS scanners unsafe-read-endpoint and non-idempotent-write),
  `rules-cross-layer` (the multi-tree `cross-layer/*` rules joining HTTP/DB/tRPC IO facts across
  trees), and `rules-schema`
  (the Prisma structural rules + the usage-aware checks, each a registered `schema/*` id of its own
  behind the `schema-structural`/`schema-usage` family gates). Per-crate counts are not written here —
  [`docs/rules/catalog.md`](../docs/rules/catalog.md)'s totals are machine-checked against the loaded
  rule set, and a second hand-kept copy would drift from them. Seams, criticality,
  scores, health, and recommendations are **not** rules — they're scores computed in `crates/metrics`,
  registered via that crate's own `register_native_analyses` (see "Adding a rule" below), and only ride the
  same `RuleConfig` disable/suppress/severity-override id space as native rules do. Layer-violations/feature-envy are a roadmap item
  (see `docs/rules/catalog.md#roadmap`) — no crate exists for them yet; a placeholder `rules-architecture`
  crate was removed since it carried no code, and gets recreated only when that work actually starts.

## `rules/dsl/` — declarative DSL rule packs (JSON data)
- **Criterion**: self-contained detection such as pattern matching (lexical/pattern-based scanners).
- **Form**: `<id>.json` — interpreted **natively** by the `core::dsl` interpreter. The JSON pack itself is
  data, not a crate. Each first-party pack lives in its own folder, `rules/dsl/<pack>/<pack>.json`, with
  the pack's end-to-end tests co-located right next to it as `rules/dsl/<pack>/<pack>.rs` (packs shipping at
  least one rule only — stub packs have no tests yet). `zzop_core::pack_loader::load_dsl_packs`
  (`crates/core/src/pack_loader.rs`) scans BOTH this depth-1 "pack folder" layout and a flat
  `<dir>/<id>.json` layout in the same call — a caller-supplied `packsDir` (third-party packs) is free to
  stay flat; nesting is purely organizational, never required.
- **`zzop-rule-packs` crate** (`rules/Cargo.toml`, sibling to this README): a thin, code-free crate that
  exists ONLY to give each pack folder's `<pack>.rs` a `cargo test` target (one `[[test]]` entry per pack,
  `path = "dsl/<pack>/<pack>.rs"`). It carries no rule data and no interpreter logic — that stays in
  `zzop-core` (loading/schema) and `zzop-engine` (evaluation), both of which it depends on as
  dev-dependencies. `crates/engine/tests/rule_contracts/` machine-checks that this crate's `[[test]]`
  list stays in sync with the pack folders on disk.
- **Distribution**: the DSL packs (`rules/dsl/`) are **compile-time-embedded** into the `zzop-mcp` binary
  (the single runtime form since the npm distribution was removed, 2026-07-20), so they ride the binary
  rather than a separately-versioned package. **Platform-independent** (data, so prebuilds are unaffected).
  Build-free replacement. Hosts with no pack directory on disk receive them as data via the `packDefs`
  request field; a `packsDir` still loads packs off disk when present. `<pack>.rs` native rule files are
  compiled in, not shipped as data.
- **Extensibility**: same DSL schema for first-party and third-party — a user can drop in a JSON rule.
- **Why DSL over WASM?** Redistribution is needed regardless, so the DSL gives the same build-free / platform-independent benefits while wasmtime, the ABI, the boundary cost, and the ~3x slowdown all disappear. (Biome GritQL / ast-grep / Semgrep model.)
- **Status**: every shipped pack lives at `rules/dsl/<pack>/<pack>.json` and every one of them carries
  rules — there are no `"rules": []` stubs left. Java security-concern rules live in `security` (concern-named, not
  language-named), including `cmd-injection` (a `method-scan` co-occurrence of `exec`/`ProcessBuilder`
  with string concatenation — no Java CST needed after all). Full pack/rule list:
  [`docs/rules/catalog.md`](../docs/rules/catalog.md).

### DSL matchers (`core::dsl::Matcher`)
- Shipped: `line-scan`, `method-scan`, `symbol-scan`, `io-scan`, each with a growing set of v2/v3 fields
  (`require_file_all`, `exclude_pattern`, `absent`, `file_exclude_pattern`, ...) added as
  real packs needed them. Full field-by-field semantics: [`docs/rules/dsl-reference.md`](../docs/rules/dsl-reference.md).
- Roadmap: a `graph` matcher for structural/whole-IR queries the current scanners can't express.
- Rules the DSL cannot express -> `rules/native/`.

> Both layers share one config surface (`core::registry`'s `RuleConfig`) — `disabled_rules`,
> `severity_overrides`, `suppressions`, all keyed by the same id space (a native analysis id, a pack id,
> or a `"<pack>/<rule>"` id).
> "Native" is only where a rule is compiled, not "always runs": a native analysis is disabled by id just
> like a pack rule is.

## Adding a rule touches only `rules/`

The kernel (`crates/core`) and the engine's orchestration code are rule-vocabulary-free by construction —
`crates/core/src/registry.rs` exposes only a generic, id-agnostic mechanism
(`register_native_analysis_stub`), never a specific rule id. Two meta-tests
(`crates/engine/tests/rule_contracts/`) machine-enforce this: `no_dsl_id_collides_with_a_native_analysis_id`
plus id-hygiene checks for DSL, and `kernel_core_carries_no_native_analysis_id_string_literal` for native.
Concretely, adding either kind of rule never requires editing `crates/core` or `crates/engine`'s
orchestration logic:

- **A native rule**: implement the body in its owning crate (`rules/native/rules-graph`,
  `rules/native/rules-http`, `rules/native/rules-cross-layer`, or `rules/native/rules-schema` — or a new
  sibling crate for a new rule family), add its id to that
  crate's own `register_native_analyses` function (id only — the finding's severity is set where the
  finding is built, so there is no second copy to drift), and add tests in the same crate. `zzop_engine::register_all_native`
  (`crates/engine/src/lib.rs`) composes every owning crate's `register_native_analyses` — it already
  depends on all of them, so a new crate only needs one line added there. `docs/rules/catalog.md`'s totals
  and per-id table need updating too (machine-checked by the `rule_contracts` meta-test's catalog-sync tests).
- **A DSL rule**: add a rule entry to a pack's `<pack>.json` (or a new pack folder) under `rules/dsl/`, plus
  a co-located `<pack>.rs` end-to-end test. No Rust code changes anywhere — `zzop_core::load_dsl_packs`
  discovers packs from disk.

In both cases `crates/core`/`crates/engine`'s own source is untouched — only `rules/` (and
`docs/rules/catalog.md`) changes.
