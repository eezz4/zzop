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
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use zzop_core::{
    ClassShapeFragment, ConsumeBodyShape, FileProjection, ImportBinding, IoConsume, IoFacts,
    IoProvide, NonIdempotentKind, NormalizedEnvelope, ProcedureRouterEntry,
    ProcedureRouterFragment, ProvideBodyField, ProvideBodyShape, ReExport, RouterMountEntry,
    RouterMountFragment, SourceSymbol, SourceSymbolKind, WriteSite, NORMALIZED_AST_FORMAT,
};

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

// ---------------------------------------------------------------------------------------------
// REQUIRED-ness and NULLABILITY parity — see the module doc's "Beyond key-NAME presence" section.
// ---------------------------------------------------------------------------------------------

/// Drop-one-key probing (REQUIRED direction): for every key in `map`, remove just that key and
/// attempt `serde_json::from_value::<T>` on the result — failure means `T`'s `Deserialize` impl
/// actually needs that key present, i.e. it is required.
fn probe_required_keys<T: serde::de::DeserializeOwned>(
    map: &serde_json::Map<String, Value>,
) -> HashSet<String> {
    let mut required = HashSet::new();
    for key in map.keys() {
        let mut probe = map.clone();
        probe.remove(key);
        if serde_json::from_value::<T>(Value::Object(probe)).is_err() {
            required.insert(key.clone());
        }
    }
    required
}

/// Null-substitution probing (NULLABLE direction): for every key in `map`, set its value to JSON
/// `null` and attempt `serde_json::from_value::<T>` on the result — success means that key's value
/// may legitimately be `null` (an `Option<T>` Rust field).
fn probe_nullable_keys<T: serde::de::DeserializeOwned>(
    map: &serde_json::Map<String, Value>,
) -> HashSet<String> {
    let mut nullable = HashSet::new();
    for key in map.keys() {
        let mut probe = map.clone();
        probe.insert(key.clone(), Value::Null);
        if serde_json::from_value::<T>(Value::Object(probe)).is_ok() {
            nullable.insert(key.clone());
        }
    }
    nullable
}

/// A definition's (or a definition variant's) declared `required` array, or the empty set when the
/// key is absent entirely — absence is itself a claim ("nothing here is required"), not "unknown".
fn schema_required_set(def_or_variant: &Value) -> HashSet<String> {
    def_or_variant
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| {
                    v.as_str()
                        .unwrap_or_else(|| panic!("required entries must be strings, got: {v}"))
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether one schema property declaration marks itself nullable, under this schema's own
/// conventions (see module doc): a `type` array containing `"null"` (primitives, inline enums), or a
/// `"oneOf"` alternative of `{"type": "null"}` (the `$ref`-to-object-type case, since draft-07 ignores
/// keywords sibling to a bare `$ref`).
fn schema_property_is_nullable(prop_schema: &Value) -> bool {
    let type_is_nullable = prop_schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|arr| arr.iter().any(|t| t == "null"));
    let one_of_is_nullable = prop_schema
        .get("oneOf")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter()
                .any(|alt| alt.get("type").and_then(Value::as_str) == Some("null"))
        });
    type_is_nullable || one_of_is_nullable
}

/// The subset of `schema_props` that [`schema_property_is_nullable`] considers nullable.
fn schema_nullable_set(schema_props: &serde_json::Map<String, Value>) -> HashSet<String> {
    schema_props
        .iter()
        .filter(|(_, v)| schema_property_is_nullable(v))
        .map(|(k, _)| k.clone())
        .collect()
}

/// Legible bidirectional diff shared by the required-ness and nullability checks: `computed` is what
/// probing the Rust type found true, `declared` is what the schema claims. Any asymmetry is drift.
fn assert_set_parity(
    pointer: &str,
    def_name: &str,
    dimension: &str,
    computed: &HashSet<String>,
    declared: &HashSet<String>,
) {
    let mut missing_from_schema: Vec<&String> = computed.difference(declared).collect();
    missing_from_schema.sort();
    let mut stale_in_schema: Vec<&String> = declared.difference(computed).collect();
    stale_in_schema.sort();
    assert!(
        missing_from_schema.is_empty() && stale_in_schema.is_empty(),
        "{pointer}: definitions.{def_name}'s {dimension} does not match what probing the real Rust \
         type shows. Rust behavior implies {dimension} for (but the schema does not declare) \
         {missing_from_schema:?}; the schema declares {dimension} for (but Rust probing disagrees) \
         {stale_in_schema:?}."
    );
}

/// Combined required-ness + nullability check for one struct-shaped definition: `sample` is that
/// type's fully-populated Rust value, `schema_def` is the matching `definitions.<name>` entry (or the
/// schema root, which has the same `required`/`properties` shape).
fn assert_required_and_nullable_parity<T>(
    pointer: &str,
    def_name: &str,
    sample: &T,
    schema_def: &Value,
) where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let produced = serde_json::to_value(sample).expect("sample must serialize");
    let map = produced.as_object().unwrap_or_else(|| {
        panic!("{pointer}: sample must serialize to an object, got: {produced}")
    });

    let computed_required = probe_required_keys::<T>(map);
    let declared_required = schema_required_set(schema_def);
    assert_set_parity(
        pointer,
        def_name,
        "required-ness",
        &computed_required,
        &declared_required,
    );

    let computed_nullable = probe_nullable_keys::<T>(map);
    let declared_nullable = schema_nullable_set(props(schema_def));
    assert_set_parity(
        pointer,
        def_name,
        "nullability",
        &computed_nullable,
        &declared_nullable,
    );
}

/// Same as [`assert_required_and_nullable_parity`], but for one variant of an externally-tagged enum
/// (`ProcedureRouterEntry`/`RouterMountEntry`): `sample` serializes as `{ "<tag>": { ...inner... } }`, and
/// probing removes/nulls keys inside `inner` while re-wrapping in the tag on each attempt.
fn assert_variant_required_and_nullable_parity<T>(
    pointer: &str,
    def_name: &str,
    tag: &str,
    sample: &T,
    schema_variant_def: &Value,
) where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let produced = serde_json::to_value(sample).expect("variant sample must serialize");
    let outer = produced.as_object().unwrap_or_else(|| {
        panic!("{pointer}: variant sample must serialize to an object, got: {produced}")
    });
    let inner = outer
        .get(tag)
        .and_then(Value::as_object)
        .unwrap_or_else(|| {
            panic!("{pointer}: expected tag '{tag}' in serialized variant: {outer:?}")
        });

    let mut computed_required = HashSet::new();
    let mut computed_nullable = HashSet::new();
    for key in inner.keys() {
        let mut probe_removed = inner.clone();
        probe_removed.remove(key);
        if serde_json::from_value::<T>(serde_json::json!({ tag: Value::Object(probe_removed) }))
            .is_err()
        {
            computed_required.insert(key.clone());
        }

        let mut probe_null = inner.clone();
        probe_null.insert(key.clone(), Value::Null);
        if serde_json::from_value::<T>(serde_json::json!({ tag: Value::Object(probe_null) }))
            .is_ok()
        {
            computed_nullable.insert(key.clone());
        }
    }

    let declared_required = schema_required_set(schema_variant_def);
    assert_set_parity(
        pointer,
        def_name,
        "required-ness",
        &computed_required,
        &declared_required,
    );

    let declared_nullable = schema_nullable_set(props(schema_variant_def));
    assert_set_parity(
        pointer,
        def_name,
        "nullability",
        &computed_nullable,
        &declared_nullable,
    );
}

// ---------------------------------------------------------------------------------------------
// ENUM value-set parity — see the module doc's "Beyond key-NAME presence" section.
// ---------------------------------------------------------------------------------------------

/// `SourceSymbolKind`'s wire form — an EXHAUSTIVE match (no wildcard arm), so adding a new variant to
/// `SourceSymbolKind` breaks compilation here until this function AND `ALL_SOURCE_SYMBOL_KINDS` below
/// are updated. Independent of `SourceSymbolKind`'s own `#[serde(rename_all = "lowercase")]` — this
/// re-derives the wire form by hand so a rename on the Rust side is caught too, not just a new
/// variant.
fn source_symbol_kind_wire(kind: SourceSymbolKind) -> &'static str {
    match kind {
        SourceSymbolKind::Function => "function",
        SourceSymbolKind::Class => "class",
        SourceSymbolKind::Const => "const",
        SourceSymbolKind::Type => "type",
        SourceSymbolKind::Interface => "interface",
    }
}

const ALL_SOURCE_SYMBOL_KINDS: [SourceSymbolKind; 5] = [
    SourceSymbolKind::Function,
    SourceSymbolKind::Class,
    SourceSymbolKind::Const,
    SourceSymbolKind::Type,
    SourceSymbolKind::Interface,
];

/// `NonIdempotentKind`'s wire form — see [`source_symbol_kind_wire`]'s doc for why this is a
/// standalone exhaustive match (independent of `NonIdempotentKind::as_str`, which is production code
/// this test should not simply re-assert).
fn non_idempotent_kind_wire(kind: NonIdempotentKind) -> &'static str {
    match kind {
        NonIdempotentKind::Create => "create",
        NonIdempotentKind::AtomicAccumulate => "atomic-accumulate",
        NonIdempotentKind::Counter => "counter",
    }
}

const ALL_NON_IDEMPOTENT_KINDS: [NonIdempotentKind; 3] = [
    NonIdempotentKind::Create,
    NonIdempotentKind::AtomicAccumulate,
    NonIdempotentKind::Counter,
];

/// Bidirectional enum-value-set check for one schema `enum` property backed by a real Rust enum:
/// every schema enum value must deserialize into `T`, and every Rust variant (as enumerated by the
/// exhaustive `wire` mapping) must serialize into a value the schema declares. `schema_enum_prop` is
/// the property's own schema object (i.e. `...properties.kind`, not the whole definition);
/// `all_variants` must list every `T` variant — [`source_symbol_kind_wire`]'s doc explains how the
/// exhaustive match keeps that list honest.
fn assert_enum_parity<T, F>(pointer: &str, schema_enum_prop: &Value, all_variants: &[T], wire: F)
where
    T: Copy + serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
    F: Fn(T) -> &'static str,
{
    let schema_values: HashSet<String> = schema_enum_prop
        .get("enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{pointer}: expected an `enum` array in {schema_enum_prop}"))
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    for v in &schema_values {
        assert!(
            serde_json::from_value::<T>(Value::String(v.clone())).is_ok(),
            "{pointer}: schema enum value {v:?} does not deserialize into the Rust enum — stale \
             schema entry, or the Rust side renamed/removed the variant."
        );
    }

    let mut rust_values: HashSet<String> = HashSet::new();
    for &variant in all_variants {
        let expected_wire = wire(variant);
        let serialized = serde_json::to_value(variant).expect("variant must serialize");
        assert_eq!(
            serialized,
            Value::String(expected_wire.to_string()),
            "{pointer}: variant {variant:?} serialized differently than its exhaustive wire mapping \
             expects — the mapping function and the real #[serde(...)] attribute have drifted apart"
        );
        rust_values.insert(expected_wire.to_string());
    }

    let mut missing_from_schema: Vec<&String> = rust_values.difference(&schema_values).collect();
    missing_from_schema.sort();
    let mut stale_in_schema: Vec<&String> = schema_values.difference(&rust_values).collect();
    stale_in_schema.sort();
    assert!(
        missing_from_schema.is_empty() && stale_in_schema.is_empty(),
        "{pointer}: enum value set drift — Rust produces (but the schema's `enum` omits) \
         {missing_from_schema:?}; the schema's `enum` declares (but no Rust variant produces) \
         {stale_in_schema:?}."
    );
}

/// One fully-populated `ProvideBodyField` — every `Option` `Some`, nothing to default.
fn sample_provide_body_field() -> ProvideBodyField {
    ProvideBodyField {
        name: "email".to_string(),
        optional: true,
    }
}

fn sample_provide_body_shape() -> ProvideBodyShape {
    ProvideBodyShape {
        sub_key: Some("user".to_string()),
        dto_ref: Some("CreateUserDto".to_string()),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

fn sample_consume_body_shape() -> ConsumeBodyShape {
    ConsumeBodyShape {
        keys: vec!["user".to_string(), "user.email".to_string()],
        complete_at: vec!["".to_string(), "user".to_string()],
    }
}

fn sample_write_site() -> WriteSite {
    WriteSite {
        file: "src/user.service.ts".to_string(),
        line: 42,
        sink: "prisma.user.update".to_string(),
        kind: Some(NonIdempotentKind::Create),
    }
}

fn sample_symbol() -> SourceSymbol {
    SourceSymbol {
        id: "src/user.service.ts#createUser".to_string(),
        file: "src/user.service.ts".to_string(),
        name: "createUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 10,
        exported: true,
        is_default: true,
        body_start: Some(11),
        body_end: Some(20),
        write_sites: vec![sample_write_site()],
    }
}

fn sample_import_binding() -> ImportBinding {
    ImportBinding {
        specifier: "../shared/prisma".to_string(),
        original: "default".to_string(),
        deferred: true,
        type_only: true,
    }
}

fn sample_re_export() -> ReExport {
    ReExport {
        specifier: "./bar".to_string(),
        original: "Bar".to_string(),
        local_alias: "BarAlias".to_string(),
        type_only: true,
    }
}

fn sample_io_provide() -> IoProvide {
    IoProvide {
        kind: "http".to_string(),
        key: "GET /users/{}".to_string(),
        file: "src/user.controller.ts".to_string(),
        line: 15,
        symbol: Some("createUser".to_string()),
        body: Some(sample_provide_body_shape()),
    }
}

fn sample_io_consume() -> IoConsume {
    IoConsume {
        kind: "http".to_string(),
        key: Some("GET /users/{}".to_string()),
        file: "src/user.client.ts".to_string(),
        line: 25,
        raw: Some("axios.get(url)".to_string()),
        method: Some("GET".to_string()),
        body: Some(sample_consume_body_shape()),
        client: Some("axios".to_string()),
    }
}

fn sample_class_shape_fragment() -> ClassShapeFragment {
    ClassShapeFragment {
        name: "CreateUserDto".to_string(),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

fn sample_trpc_leaf() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Leaf {
        key: "get".to_string(),
        verb: "QUERY".to_string(),
        line: 3,
    }
}

fn sample_trpc_ref() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Ref {
        key: "sub".to_string(),
        ident: "subRouter".to_string(),
        specifier: Some("./sub".to_string()),
    }
}

fn sample_trpc_nested() -> ProcedureRouterEntry {
    ProcedureRouterEntry::Nested {
        key: "nested".to_string(),
        entries: vec![sample_trpc_leaf()],
    }
}

fn sample_trpc_fragment() -> ProcedureRouterFragment {
    ProcedureRouterFragment {
        name: "appRouter".to_string(),
        entries: vec![sample_trpc_leaf(), sample_trpc_ref(), sample_trpc_nested()],
    }
}

fn sample_mount_verb() -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: "POST".to_string(),
        path: "/setup".to_string(),
        handler: Some("handler".to_string()),
        line: 7,
    }
}

fn sample_mount_mount() -> RouterMountEntry {
    RouterMountEntry::Mount {
        prefix: "/two-factor".to_string(),
        ident: "twoFactorRoute".to_string(),
        specifier: Some("./two-factor".to_string()),
    }
}

fn sample_router_mount_fragment() -> RouterMountFragment {
    RouterMountFragment {
        name: "auth".to_string(),
        entries: vec![sample_mount_verb(), sample_mount_mount()],
    }
}

/// One fully-populated `FileProjection` — every optional/`#[serde(default)]` field non-default so
/// every one of them actually appears in the serialized JSON.
fn sample_file_projection() -> FileProjection {
    let mut imports = std::collections::BTreeMap::new();
    imports.insert("prisma".to_string(), sample_import_binding());

    let mut const_map_fragment = std::collections::HashMap::new();
    const_map_fragment.insert("USERS_TABLE".to_string(), "users".to_string());

    FileProjection {
        path: "src/user.controller.ts".to_string(),
        loc: 100,
        symbols: vec![sample_symbol()],
        imports,
        re_exports: vec![sample_re_export()],
        dynamic_imports: vec!["./lazy".to_string()],
        used_names: vec!["createUser".to_string()],
        const_map_fragment,
        procedure_router_fragments: vec![sample_trpc_fragment()],
        router_mount_fragments: vec![sample_router_mount_fragment()],
        class_shape_fragments: vec![sample_class_shape_fragment()],
        io: IoFacts {
            provides: vec![sample_io_provide()],
            consumes: vec![sample_io_consume()],
        },
        loop_spans: vec![(10, 20)],
        degraded: true,
        is_entry: true,
        attributes: Vec::new(),
    }
}

fn sample_envelope() -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "test-adapter/1".to_string(),
        source: "test-source".to_string(),
        files: vec![sample_file_projection()],
    }
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

/// The main parity guard: serializes one fully-populated `NormalizedEnvelope`, then walks it
/// exactly (JSON pointer commented at each step) against the schema definition that describes that
/// exact position.
#[test]
fn envelope_schema_matches_normalized_envelope_field_for_field() {
    let schema = load_schema();
    let sample = serde_json::to_value(sample_envelope()).expect("sample envelope must serialize");

    // $ (root) -> top-level `properties` (NormalizedEnvelope has no optional fields at all).
    let root_props = props(&schema);
    assert_parity("$", "<root>", &sample, root_props);

    // $.files[0] -> definitions.fileProjection
    let file0 = idx(field(&sample, "files", "$"), 0, "$.files");
    assert_parity(
        "$.files[0]",
        "fileProjection",
        file0,
        props(def(&schema, "fileProjection")),
    );

    // $.files[0].symbols[0] -> definitions.sourceSymbol
    let symbol0 = idx(
        field(file0, "symbols", "$.files[0]"),
        0,
        "$.files[0].symbols",
    );
    assert_parity(
        "$.files[0].symbols[0]",
        "sourceSymbol",
        symbol0,
        props(def(&schema, "sourceSymbol")),
    );

    // $.files[0].symbols[0].writeSites[0] -> definitions.writeSite (wire name is camelCase
    // writeSites — SourceSymbol is #[serde(rename_all = "camelCase")]).
    let write_site0 = idx(
        field(symbol0, "writeSites", "$.files[0].symbols[0]"),
        0,
        "$.files[0].symbols[0].writeSites",
    );
    assert_parity(
        "$.files[0].symbols[0].writeSites[0]",
        "writeSite",
        write_site0,
        props(def(&schema, "writeSite")),
    );

    // $.files[0].imports.prisma -> definitions.importBinding (ImportMap is keyed by localName;
    // the schema models this via `additionalProperties: { $ref: importBinding }`, not a `properties`
    // entry — take the one value we inserted).
    let imports_obj = obj(field(file0, "imports", "$.files[0]"), "$.files[0].imports");
    let import_binding0 = imports_obj
        .get("prisma")
        .expect("sample must carry the 'prisma' import binding");
    assert_parity(
        "$.files[0].imports.prisma",
        "importBinding",
        import_binding0,
        props(def(&schema, "importBinding")),
    );

    // $.files[0].re_exports[0] -> definitions.reExport
    let re_export0 = idx(
        field(file0, "re_exports", "$.files[0]"),
        0,
        "$.files[0].re_exports",
    );
    assert_parity(
        "$.files[0].re_exports[0]",
        "reExport",
        re_export0,
        props(def(&schema, "reExport")),
    );

    // $.files[0].class_shape_fragments[0] -> definitions.classShapeFragment
    let class_shape0 = idx(
        field(file0, "class_shape_fragments", "$.files[0]"),
        0,
        "$.files[0].class_shape_fragments",
    );
    assert_parity(
        "$.files[0].class_shape_fragments[0]",
        "classShapeFragment",
        class_shape0,
        props(def(&schema, "classShapeFragment")),
    );
    // $.files[0].class_shape_fragments[0].fields[0] -> definitions.provideBodyField (shared type)
    let class_shape_field0 = idx(
        field(
            class_shape0,
            "fields",
            "$.files[0].class_shape_fragments[0]",
        ),
        0,
        "$.files[0].class_shape_fragments[0].fields",
    );
    assert_parity(
        "$.files[0].class_shape_fragments[0].fields[0]",
        "provideBodyField",
        class_shape_field0,
        props(def(&schema, "provideBodyField")),
    );

    // $.files[0].io -> definitions.ioFacts
    let io0 = field(file0, "io", "$.files[0]");
    assert_parity(
        "$.files[0].io",
        "ioFacts",
        io0,
        props(def(&schema, "ioFacts")),
    );

    // $.files[0].io.provides[0] -> definitions.ioProvide
    let provide0 = idx(
        field(io0, "provides", "$.files[0].io"),
        0,
        "$.files[0].io.provides",
    );
    assert_parity(
        "$.files[0].io.provides[0]",
        "ioProvide",
        provide0,
        props(def(&schema, "ioProvide")),
    );
    // $.files[0].io.provides[0].body -> definitions.provideBodyShape
    let provide_body0 = field(provide0, "body", "$.files[0].io.provides[0]");
    assert_parity(
        "$.files[0].io.provides[0].body",
        "provideBodyShape",
        provide_body0,
        props(def(&schema, "provideBodyShape")),
    );
    // $.files[0].io.provides[0].body.fields[0] -> definitions.provideBodyField
    let provide_body_field0 = idx(
        field(provide_body0, "fields", "$.files[0].io.provides[0].body"),
        0,
        "$.files[0].io.provides[0].body.fields",
    );
    assert_parity(
        "$.files[0].io.provides[0].body.fields[0]",
        "provideBodyField",
        provide_body_field0,
        props(def(&schema, "provideBodyField")),
    );

    // $.files[0].io.consumes[0] -> definitions.ioConsume
    let consume0 = idx(
        field(io0, "consumes", "$.files[0].io"),
        0,
        "$.files[0].io.consumes",
    );
    assert_parity(
        "$.files[0].io.consumes[0]",
        "ioConsume",
        consume0,
        props(def(&schema, "ioConsume")),
    );
    // $.files[0].io.consumes[0].body -> definitions.consumeBodyShape
    let consume_body0 = field(consume0, "body", "$.files[0].io.consumes[0]");
    assert_parity(
        "$.files[0].io.consumes[0].body",
        "consumeBodyShape",
        consume_body0,
        props(def(&schema, "consumeBodyShape")),
    );

    // $.files[0].procedure_router_fragments[0] -> definitions.procedureRouterFragment
    let trpc_fragment0 = idx(
        field(file0, "procedure_router_fragments", "$.files[0]"),
        0,
        "$.files[0].procedure_router_fragments",
    );
    assert_parity(
        "$.files[0].procedure_router_fragments[0]",
        "procedureRouterFragment",
        trpc_fragment0,
        props(def(&schema, "procedureRouterFragment")),
    );
    // $.files[0].procedure_router_fragments[0].entries[*] -> definitions.procedureRouterEntry (externally
    // tagged enum: Leaf/Ref/Nested). The fixture carries all three variants; Nested additionally
    // recurses into its own `entries[0]` (a Leaf) to exercise the recursive shape once.
    let trpc_entries = field(
        trpc_fragment0,
        "entries",
        "$.files[0].procedure_router_fragments[0]",
    )
    .as_array()
    .expect("entries must be an array");
    let trpc_entry_def = def(&schema, "procedureRouterEntry");
    let mut trpc_tags_seen = Vec::new();
    for (i, entry) in trpc_entries.iter().enumerate() {
        let pointer = format!("$.files[0].procedure_router_fragments[0].entries[{i}]");
        let (tag, inner) = assert_variant_tag_known(&pointer, entry, trpc_entry_def);
        trpc_tags_seen.push(tag);
        let variant_props = props(
            field(trpc_entry_def, "properties", &pointer)
                .get(tag)
                .unwrap(),
        );
        assert_parity(
            &format!("{pointer}.{tag}"),
            &format!("procedureRouterEntry.{tag}"),
            inner,
            variant_props,
        );
        if tag == "Nested" {
            let nested_entry0 = idx(
                field(inner, "entries", &pointer),
                0,
                &format!("{pointer}.entries"),
            );
            let nested_pointer = format!("{pointer}.entries[0]");
            let (nested_tag, nested_inner) =
                assert_variant_tag_known(&nested_pointer, nested_entry0, trpc_entry_def);
            trpc_tags_seen.push(nested_tag);
            let nested_variant_props = props(
                field(trpc_entry_def, "properties", &nested_pointer)
                    .get(nested_tag)
                    .unwrap(),
            );
            assert_parity(
                &format!("{nested_pointer}.{nested_tag}"),
                &format!("procedureRouterEntry.{nested_tag}"),
                nested_inner,
                nested_variant_props,
            );
        }
    }
    assert_all_variants_covered(
        "$.files[0].procedure_router_fragments[0].entries[*]",
        trpc_entry_def,
        &trpc_tags_seen,
    );

    // $.files[0].router_mount_fragments[0] -> definitions.routerMountFragment
    let mount_fragment0 = idx(
        field(file0, "router_mount_fragments", "$.files[0]"),
        0,
        "$.files[0].router_mount_fragments",
    );
    assert_parity(
        "$.files[0].router_mount_fragments[0]",
        "routerMountFragment",
        mount_fragment0,
        props(def(&schema, "routerMountFragment")),
    );
    // $.files[0].router_mount_fragments[0].entries[*] -> definitions.routerMountEntry (Verb/Mount)
    let mount_entries = field(
        mount_fragment0,
        "entries",
        "$.files[0].router_mount_fragments[0]",
    )
    .as_array()
    .expect("entries must be an array");
    let mount_entry_def = def(&schema, "routerMountEntry");
    let mut mount_tags_seen = Vec::new();
    for (i, entry) in mount_entries.iter().enumerate() {
        let pointer = format!("$.files[0].router_mount_fragments[0].entries[{i}]");
        let (tag, inner) = assert_variant_tag_known(&pointer, entry, mount_entry_def);
        mount_tags_seen.push(tag);
        let variant_props = props(
            field(mount_entry_def, "properties", &pointer)
                .get(tag)
                .unwrap(),
        );
        assert_parity(
            &format!("{pointer}.{tag}"),
            &format!("routerMountEntry.{tag}"),
            inner,
            variant_props,
        );
    }
    assert_all_variants_covered(
        "$.files[0].router_mount_fragments[0].entries[*]",
        mount_entry_def,
        &mount_tags_seen,
    );
}

/// The required-ness + nullability parity guard — see the module doc's "Beyond key-NAME presence"
/// section for method. One call per schema definition (16 total), plus the schema root, plus one call
/// per externally-tagged enum VARIANT (`procedureRouterEntry`'s Leaf/Ref/Nested, `routerMountEntry`'s
/// Verb/Mount — each variant has its own independent `required`/`properties`, so each needs its own
/// probe).
#[test]
fn envelope_schema_required_and_nullability_matches_rust_types() {
    let schema = load_schema();

    assert_required_and_nullable_parity("$", "<root>", &sample_envelope(), &schema);

    assert_required_and_nullable_parity(
        "$.files[0]",
        "fileProjection",
        &sample_file_projection(),
        def(&schema, "fileProjection"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].symbols[0]",
        "sourceSymbol",
        &sample_symbol(),
        def(&schema, "sourceSymbol"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].symbols[0].writeSites[0]",
        "writeSite",
        &sample_write_site(),
        def(&schema, "writeSite"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].imports.prisma",
        "importBinding",
        &sample_import_binding(),
        def(&schema, "importBinding"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].re_exports[0]",
        "reExport",
        &sample_re_export(),
        def(&schema, "reExport"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].class_shape_fragments[0]",
        "classShapeFragment",
        &sample_class_shape_fragment(),
        def(&schema, "classShapeFragment"),
    );

    // IoFacts has no dedicated `sample_*` builder (it's assembled inline in `sample_file_projection`);
    // build the same shape here so the probe has a real, fully-populated `IoFacts` value.
    let io_facts_sample = IoFacts {
        provides: vec![sample_io_provide()],
        consumes: vec![sample_io_consume()],
    };
    assert_required_and_nullable_parity(
        "$.files[0].io",
        "ioFacts",
        &io_facts_sample,
        def(&schema, "ioFacts"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0]",
        "ioProvide",
        &sample_io_provide(),
        def(&schema, "ioProvide"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0].body",
        "provideBodyShape",
        &sample_provide_body_shape(),
        def(&schema, "provideBodyShape"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0].body.fields[0]",
        "provideBodyField",
        &sample_provide_body_field(),
        def(&schema, "provideBodyField"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.consumes[0]",
        "ioConsume",
        &sample_io_consume(),
        def(&schema, "ioConsume"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.consumes[0].body",
        "consumeBodyShape",
        &sample_consume_body_shape(),
        def(&schema, "consumeBodyShape"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0]",
        "procedureRouterFragment",
        &sample_trpc_fragment(),
        def(&schema, "procedureRouterFragment"),
    );

    let trpc_entry_def = def(&schema, "procedureRouterEntry");
    let trpc_variant_def = |tag: &str| {
        field(
            trpc_entry_def,
            "properties",
            "$.definitions.procedureRouterEntry",
        )
        .get(tag)
        .unwrap_or_else(|| panic!("definitions.procedureRouterEntry.properties.{tag} must exist"))
    };
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Leaf]",
        "procedureRouterEntry.Leaf",
        "Leaf",
        &sample_trpc_leaf(),
        trpc_variant_def("Leaf"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Ref]",
        "procedureRouterEntry.Ref",
        "Ref",
        &sample_trpc_ref(),
        trpc_variant_def("Ref"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Nested]",
        "procedureRouterEntry.Nested",
        "Nested",
        &sample_trpc_nested(),
        trpc_variant_def("Nested"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0]",
        "routerMountFragment",
        &sample_router_mount_fragment(),
        def(&schema, "routerMountFragment"),
    );

    let mount_entry_def = def(&schema, "routerMountEntry");
    let mount_variant_def = |tag: &str| {
        field(
            mount_entry_def,
            "properties",
            "$.definitions.routerMountEntry",
        )
        .get(tag)
        .unwrap_or_else(|| panic!("definitions.routerMountEntry.properties.{tag} must exist"))
    };
    assert_variant_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0].entries[Verb]",
        "routerMountEntry.Verb",
        "Verb",
        &sample_mount_verb(),
        mount_variant_def("Verb"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0].entries[Mount]",
        "routerMountEntry.Mount",
        "Mount",
        &sample_mount_mount(),
        mount_variant_def("Mount"),
    );
}

/// `sourceSymbol.kind`'s enum value set, bidirectionally sealed against `SourceSymbolKind` — see the
/// module doc's "Beyond key-NAME presence" section.
#[test]
fn source_symbol_kind_enum_matches_schema_bidirectionally() {
    let schema = load_schema();
    let kind_prop = def(&schema, "sourceSymbol")
        .get("properties")
        .and_then(|p| p.get("kind"))
        .expect("definitions.sourceSymbol.properties.kind must exist");
    assert_enum_parity(
        "$.files[0].symbols[0].kind",
        kind_prop,
        &ALL_SOURCE_SYMBOL_KINDS,
        source_symbol_kind_wire,
    );
}

/// `writeSite.kind`'s enum value set (excluding its `null` alternative, which the nullability check
/// above already covers), bidirectionally sealed against `NonIdempotentKind`. Sweep note: this and
/// `sourceSymbol.kind` are the only schema `enum` properties backed by a real Rust enum type —
/// `procedureRouterEntry.Leaf.verb` also declares a schema `enum`
/// (`["QUERY","MUTATION","SUBSCRIPTION"]`), but `ProcedureRouterEntry::Leaf::verb` is a plain `String`
/// field in Rust (`crates/core/src/fragments.rs`), not an enum, so there is no Rust variant set to
/// seal it against; that schema `enum` is documentation-only today. No other schema property in this
/// file declares `enum`.
#[test]
fn write_site_kind_enum_matches_schema_bidirectionally() {
    let schema = load_schema();
    let kind_prop = def(&schema, "writeSite")
        .get("properties")
        .and_then(|p| p.get("kind"))
        .expect("definitions.writeSite.properties.kind must exist");
    assert_enum_parity(
        "$.files[0].symbols[0].writeSites[0].kind",
        kind_prop,
        &ALL_NON_IDEMPOTENT_KINDS,
        non_idempotent_kind_wire,
    );
}
