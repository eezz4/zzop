//! ENUM value-set parity — see the target doc in `main.rs`, "Beyond key-NAME presence" section.

use std::collections::HashSet;

use serde_json::Value;

use zzop_core::{NonIdempotentKind, SourceSymbolKind};

use crate::{def, load_schema};

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
