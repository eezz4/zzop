//! Parity guard for `docs/adapters/envelope.schema.json` against the real Rust serde types it
//! claims to mirror field-for-field (`crates/core/src/normalized.rs`'s `NormalizedEnvelope`/
//! `FileProjection`, and everything reachable from them: `SourceSymbol`/`WriteSite`,
//! `ImportBinding`/`ReExport`, `IoFacts`/`IoProvide`/`IoConsume`/`ProvideBodyShape`/
//! `ProvideBodyField`/`ConsumeBodyShape`, `ClassShapeFragment`, `ProcedureRouterFragment`/
//! `ProcedureRouterEntry`, `RouterMountFragment`/`RouterMountEntry`).
//!
//! ## Method
//! Build ONE fully-populated `NormalizedEnvelope` sample — every `Option` is `Some`, every
//! `Vec`/`Map` non-empty, recursively, so every `skip_serializing_if`-gated field actually appears
//! in the serialized JSON — then walk the serialized `serde_json::Value` and, at each object,
//! compare its live key set against the schema definition mapped to that exact JSON pointer, in
//! BOTH directions:
//!   - produced keys not in the schema definition's `properties`  -> the schema is MISSING a field
//!     (this is what the original audit item found: `class_shape_fragments`, `loop_spans`,
//!     `IoProvide.body`, `IoConsume.body`/`client`, and the four missing definitions).
//!   - schema `properties` never produced by the fully-populated sample -> the schema has a STALE
//!     field (a rename, a removed field, or a typo) — modulo the alias allowlist below.
//!
//! The JSON-pointer -> schema-definition mapping is spelled out explicitly at each call site
//! (rather than walked generically) so a future drift failure points straight at the one field
//! that moved, not a generic tree-diff.
//!
//! ## Beyond key-NAME presence: required-ness, nullability, enum value sets
//! The bidirectional key-set diff above only seals that the schema and the Rust types agree on WHICH
//! keys exist at each position. The schema's headline promise goes further — "required-ness, and
//! nullability... mirror the Rust serde types field-for-field" — and its enum value sets
//! (`sourceSymbol.kind`, `writeSite.kind`) are a promise too. Three more machine-sealed dimensions,
//! each with its own probing method:
//!   - REQUIRED: drop-one-key probing. Starting from the one fully-populated sample object, remove
//!     each key in turn and attempt `serde_json::from_value::<T>` for the Rust type at that position —
//!     deserialization failure ⇔ that key is truly required. Compared against the schema's own
//!     `required` array (or the empty set, when the definition omits `required` entirely — that is
//!     itself a claim of "nothing required").
//!   - NULLABLE: null-substitution probing. For each key, set its value to JSON `null` in a clone of
//!     the sample and attempt `serde_json::from_value::<T>` again — success ⇔ that key's value may be
//!     `null` (an `Option<T>` field, regardless of whether it also carries `#[serde(default)]`, which
//!     governs OMITTABILITY, an orthogonal axis from nullability). Compared against the schema's own
//!     nullability declaration for that property — this schema's convention is a `type` array
//!     containing `"null"` (primitives) or, for a `$ref`'d object type, `"oneOf": [{"$ref": ...},
//!     {"type": "null"}]` (draft-07 ignores keywords sibling to a bare `$ref`, so a nullable $ref
//!     cannot be expressed as `$ref` + `type` — it needs `oneOf`). Two properties documented
//!     nullability only in prose before this test existed (`ioProvide.body`, `ioConsume.body`) and have
//!     been promoted to the `oneOf` form so the machine-readable declaration matches reality; see this
//!     file's test-batch report for the fix.
//!   - ENUM VALUE SETS: `sourceSymbol.kind` (`SourceSymbolKind`) and `writeSite.kind`
//!     (`Option<NonIdempotentKind>`) are the only schema `enum` properties backed by a real Rust enum
//!     type (swept: `procedureRouterEntry.Leaf.verb` also has a schema `enum`, but
//!     `ProcedureRouterEntry::Leaf::verb` is a plain `String` field in Rust, not an enum — nothing to seal
//!     there). Checked bidirectionally: every schema enum value must deserialize into the Rust enum,
//!     and every Rust variant (enumerated via an EXHAUSTIVE match with no wildcard arm — a new variant
//!     then breaks compilation, forcing this file to be updated) must serialize into a value the schema
//!     declares.
//!
//! ## Aliases are handled deliberately, not generically
//! `serde(alias = "...")` (e.g. `SourceSymbol::is_default`'s `is_default` alias for the canonical
//! `isDefault`) affects DESERIALIZATION only — it is never present in serialized output, so it can
//! never show up as a "produced key". This schema's convention (see `envelope.schema.json`'s
//! `is_default`/`body_start`/`body_end`/`loop_spans` property descriptions) is to document every
//! alias in the CANONICAL property's `description` text rather than mint a second schema property
//! for it — so today no schema `properties` entry is itself alias-only, and `ALIAS_ONLY_SCHEMA_KEYS`
//! below is empty. It stays as an explicit, named allowlist (rather than the stale-key check simply
//! being skipped) so a future schema edit that DOES add an alias-named property has one obvious,
//! deliberate place to declare it — instead of the parity check silently weakening everywhere.
mod enum_parity;
mod key_parity;
mod probes;
mod required_nullable;
mod samples;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

/// Schema properties that are DELIBERATELY alias-only (never produced by any serialization) — see
/// this file's module doc for why this is empty today and why it exists as a named allowlist
/// anyway. Checked only in the schema -> type ("stale in schema") direction.
const ALIAS_ONLY_SCHEMA_KEYS: &[&str] = &[];

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/adapters/envelope.schema.json")
}

fn load_schema() -> Value {
    let text = fs::read_to_string(schema_path()).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {e}\nThis test resolves the schema path via CARGO_MANIFEST_DIR, \
             so it must be run from within the workspace checkout.",
            schema_path().display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", schema_path().display()))
}

/// `schema.definitions.<name>` — panics with the definition name on a typo/removal, since every
/// call site below names one deliberately.
fn def<'a>(schema: &'a Value, name: &str) -> &'a Value {
    schema
        .get("definitions")
        .and_then(|d| d.get(name))
        .unwrap_or_else(|| panic!("schema is missing definitions.{name}"))
}

/// A definition's (or a definition variant's) `properties` object — the key set this test diffs
/// against a produced JSON object's own keys.
fn props(def_or_variant: &Value) -> &serde_json::Map<String, Value> {
    def_or_variant
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("expected a `properties` object, got: {def_or_variant}"))
}

fn obj<'a>(v: &'a Value, pointer: &str) -> &'a serde_json::Map<String, Value> {
    v.as_object()
        .unwrap_or_else(|| panic!("{pointer}: expected a JSON object, got: {v}"))
}

fn field<'a>(v: &'a Value, key: &str, pointer: &str) -> &'a Value {
    v.get(key)
        .unwrap_or_else(|| panic!("{pointer}: expected key '{key}', got: {v}"))
}

fn idx<'a>(v: &'a Value, i: usize, pointer: &str) -> &'a Value {
    v.get(i)
        .unwrap_or_else(|| panic!("{pointer}: expected index {i}, got: {v}"))
}

/// The core bidirectional key-set diff — see module doc. `pointer` and `def_name` are purely for a
/// legible failure message (the JSON pointer into the sample, and the schema definition it maps
/// to).
fn assert_parity(
    pointer: &str,
    def_name: &str,
    produced: &Value,
    schema_props: &serde_json::Map<String, Value>,
) {
    let produced_keys: HashSet<&str> = obj(produced, pointer).keys().map(String::as_str).collect();
    let schema_keys: HashSet<&str> = schema_props.keys().map(String::as_str).collect();

    let mut missing_from_schema: Vec<&str> =
        produced_keys.difference(&schema_keys).copied().collect();
    missing_from_schema.sort_unstable();
    assert!(
        missing_from_schema.is_empty(),
        "{pointer}: field(s) {missing_from_schema:?} are serialized by the Rust type but are NOT \
         documented in docs/adapters/envelope.schema.json's definitions.{def_name}.properties — \
         the schema is missing them (contract-breaking: it no longer describes reality)."
    );

    let mut stale_in_schema: Vec<&str> = schema_keys
        .difference(&produced_keys)
        .copied()
        .filter(|k| !ALIAS_ONLY_SCHEMA_KEYS.contains(k))
        .collect();
    stale_in_schema.sort_unstable();
    assert!(
        stale_in_schema.is_empty(),
        "{pointer}: docs/adapters/envelope.schema.json's definitions.{def_name}.properties \
         documents {stale_in_schema:?}, but the fully-populated Rust sample never serializes \
         them (stale schema field — a rename, a removed field, or a typo). If this is a genuine \
         serde `alias` (input-only, never emitted), add it to ALIAS_ONLY_SCHEMA_KEYS at the top \
         of this file with a comment explaining which type/field it aliases."
    );
}

/// Bidirectional check for one instance of an externally-tagged Rust enum (`ProcedureRouterEntry`,
/// `RouterMountEntry`): a produced instance is `{ "<Variant>": { ...fields... } }` — exactly one
/// key. Checked per-call in the FORWARD direction only (the tag must be a known schema variant);
/// the REVERSE direction (every schema variant gets exercised by at least one sample) is checked
/// once at the end via [`assert_all_variants_covered`], since no single instance can cover every
/// variant.
fn assert_variant_tag_known<'a>(
    pointer: &str,
    produced: &'a Value,
    schema_def: &Value,
) -> (&'a str, &'a Value) {
    let o = obj(produced, pointer);
    assert_eq!(
        o.len(),
        1,
        "{pointer}: an externally-tagged enum instance must serialize as exactly one key, got: {produced}"
    );
    let (tag, inner) = o.iter().next().unwrap();
    let variants = props(schema_def);
    assert!(
        variants.contains_key(tag.as_str()),
        "{pointer}: produced tag '{tag}' is not one of the variants schema declares: {:?}",
        variants.keys().collect::<Vec<_>>()
    );
    (tag.as_str(), inner)
}

fn assert_all_variants_covered(pointer: &str, schema_def: &Value, seen_tags: &[&str]) {
    let schema_variants: HashSet<&str> = props(schema_def).keys().map(String::as_str).collect();
    let seen: HashSet<&str> = seen_tags.iter().copied().collect();
    let mut uncovered: Vec<&str> = schema_variants.difference(&seen).copied().collect();
    uncovered.sort_unstable();
    assert!(
        uncovered.is_empty(),
        "{pointer}: schema declares variant(s) {uncovered:?} that this test's fixture never \
         exercises — either the schema has a stale variant or the fixture needs a sample for it."
    );
}

#[test]
fn schema_definitions_cover_exactly_the_expected_type_set() {
    let schema = load_schema();
    let defs = schema
        .get("definitions")
        .and_then(Value::as_object)
        .expect("schema must have a `definitions` object");
    let actual: HashSet<&str> = defs.keys().map(String::as_str).collect();
    let expected: HashSet<&str> = [
        "fileProjection",
        "sourceSymbol",
        "writeSite",
        "importBinding",
        "reExport",
        "ioFacts",
        "ioProvide",
        "ioConsume",
        "provideBodyField",
        "provideBodyShape",
        "consumeBodyShape",
        "classShapeFragment",
        "procedureRouterFragment",
        "procedureRouterEntry",
        "routerMountFragment",
        "routerMountEntry",
        "attribute",
        "entityRef",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        actual, expected,
        "schema `definitions` block gained or lost a definition — update this test's `expected` \
         set deliberately if the change is intentional, and add/remove the matching parity checks \
         below."
    );
}
