//! REQUIRED-ness and NULLABILITY parity probing — see the target doc in `main.rs`, "Beyond
//! key-NAME presence" section.

use std::collections::HashSet;

use serde_json::Value;

use crate::props;

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
pub(crate) fn assert_required_and_nullable_parity<T>(
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
pub(crate) fn assert_variant_required_and_nullable_parity<T>(
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
