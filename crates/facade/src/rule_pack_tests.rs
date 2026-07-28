//! Unit tests for `validate_rule_pack_json` (`crate::rule_pack`) — the pre-load, structure-only
//! rule-pack check — plus the rule-pack JSON Schema's own parity with `zzop_core::dsl::def`'s
//! matcher types (see the FIELD-AXIS PARITY PIN section at the bottom of this file).

use std::collections::BTreeSet;

use crate::validate_rule_pack_json;

/// A real bundled pack, byte-for-byte (the same embed `zzop-config`/`zzop-mcp` ship) — every
/// shipped pack must self-evidently pass its own pre-load validator.
const BUNDLED_SECURITY_PACK: &str = include_str!("../../../rules/dsl/security/security.json");

/// The authored JSON Schema for the rule-pack shape (embedded by `zzop-mcp` as
/// `zzop://contract/rule-pack-schema`).
const RULE_PACK_SCHEMA: &str = include_str!("../../../docs/contracts/rule-pack.schema.json");

fn report(pack_json: &str) -> serde_json::Value {
    serde_json::from_str(&validate_rule_pack_json(pack_json)).expect("report is valid JSON")
}

#[test]
fn a_bundled_pack_validates_clean() {
    let v = report(BUNDLED_SECURITY_PACK);
    assert_eq!(v["valid"], true, "got: {v}");
    assert_eq!(v["issues"].as_array().expect("issues array").len(), 0);
}

#[test]
fn every_bundled_pack_validates_clean() {
    // The embed IS the shipped bundle — all of it must pass the validator it now fronts.
    for (rel, source) in zzop_config::BUNDLED_PACK_SOURCES {
        let v = report(source);
        assert_eq!(
            v["valid"], true,
            "bundled pack {rel} failed validation: {v}"
        );
    }
}

#[test]
fn unparseable_input_reports_invalid_without_erring() {
    let v = report("{ not json");
    assert_eq!(v["valid"], false);
    assert!(!v["issues"].as_array().unwrap().is_empty());
}

#[test]
fn an_array_root_is_named_instead_of_a_field_type_mismatch() {
    // A blind field test fed a JSON ARRAY as `packJson` and got serde's struct-from-sequence fallback
    // error ("invalid type: integer `1`, expected a string ...") — a field-level message that masks the
    // real problem (the root itself is the wrong shape).
    let v = report("[1,2,3]");
    assert_eq!(v["valid"], false);
    assert_eq!(
        v["issues"],
        serde_json::json!(["expected a JSON object rule pack, got an array"]),
        "got: {v}"
    );
}

#[test]
fn a_missing_required_field_is_a_named_issue() {
    // Drop `rules` — the loader's serde judgment, verbatim.
    let v = report(r#"{"id": "p"}"#);
    assert_eq!(v["valid"], false);
    let issues = v["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.as_str().unwrap().contains("missing field `rules`")),
        "got: {v}"
    );
}

#[test]
fn a_too_new_schema_version_is_a_named_issue() {
    let v = report(r#"{"id": "p", "schema_version": 999, "rules": []}"#);
    assert_eq!(v["valid"], false);
    assert!(
        v["issues"][0]
            .as_str()
            .unwrap()
            .contains("newer DSL schema"),
        "got: {v}"
    );
}

#[test]
fn a_non_compiling_regex_is_a_named_issue() {
    let broken = BUNDLED_SECURITY_PACK.replacen(r#""(?i)\\.(ts|tsx)$""#, r#""(?i)\\.(ts|tsx$""#, 1);
    assert_ne!(broken, BUNDLED_SECURITY_PACK, "the replace must have hit");
    let v = report(&broken);
    assert_eq!(v["valid"], false, "got: {v}");
    let issue = v["issues"][0].as_str().unwrap();
    assert!(issue.contains("`file_pattern`"), "got: {v}");
    assert!(issue.contains("never fire"), "got: {v}");
}

#[test]
fn an_unknown_fragment_reference_is_a_named_issue() {
    // `${does-not-exist}` names neither this pack's own `fragments` map (empty here) nor the shared
    // bundled set — `RulePackDef::expand_fragments` must fail the load exactly like a bad regex does,
    // and `validate_rule_pack_json` (which shares `parse_dsl_pack` with the real loader) must surface it.
    let v = report(
        r#"{"id": "p", "rules": [
            {"id": "r1", "severity": "info", "message": "m",
             "matcher": {"type": "line-scan", "file_pattern": "\\.ts$",
                         "file_exclude_pattern": "${does-not-exist}", "line_pattern": "TODO"}}
        ]}"#,
    );
    assert_eq!(v["valid"], false, "got: {v}");
    let issue = v["issues"][0].as_str().unwrap();
    assert!(issue.contains("unknown fragment"), "got: {v}");
    assert!(issue.contains("does-not-exist"), "got: {v}");
}

#[test]
fn a_pack_local_fragment_referencing_a_shared_name_resolves_clean() {
    // `${test-paths}` is a shared bundled fragment name (see `zzop_core::dsl::fragments`) — a pack that
    // references it without declaring its own `fragments` entry must validate clean, proving the
    // validator resolves against the shared set, not just a pack's own local map.
    let v = report(
        r#"{"id": "p", "rules": [
            {"id": "r1", "severity": "info", "message": "m",
             "matcher": {"type": "line-scan", "file_pattern": "\\.ts$",
                         "file_exclude_pattern": "${test-paths}", "line_pattern": "TODO"}}
        ]}"#,
    );
    assert_eq!(v["valid"], true, "got: {v}");
    assert_eq!(v["issues"].as_array().unwrap().len(), 0);
}

#[test]
fn the_rule_pack_schema_parses_and_names_every_severity() {
    let schema: serde_json::Value =
        serde_json::from_str(RULE_PACK_SCHEMA).expect("rule-pack schema must be valid JSON");
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    // Severity vocabulary must match `zzop_core::Severity`'s lowercase serde form.
    for sev in ["critical", "warning", "info"] {
        assert!(
            RULE_PACK_SCHEMA.contains(&format!("\"{sev}\"")),
            "missing severity {sev}"
        );
    }
    // The matcher discriminator vocabulary is pinned per-kind, against the Rust struct it tags, by
    // `matcher_field_axis_matches_the_schema` below — not by a loose substring sweep here.
}

// ---------------------------------------------------------------------------------------------
// FIELD-AXIS PARITY PIN
// ---------------------------------------------------------------------------------------------
//
// WHY THIS EXISTS. `docs/contracts/rule-pack.schema.json` is a HAND-AUTHORED mirror of
// `zzop_core::dsl::def`'s matcher structs, and `zzop-summary` embeds it verbatim
// (`crates/summary/src/contracts.rs`) to serve as the MCP resource `zzop://contract/rule-pack-schema`.
// An external pack author reads THAT, not the Rust source. Because the schema declares
// `additionalProperties: true` everywhere, a field the schema forgot breaks no validator — it just
// makes the knob invisible, and an author who cannot see e.g. an ordering gate builds an
// order-asserting rule out of plain co-occurrence instead. Silent, and exactly the failure the
// audit found (`after` / `after_in_same_function` shipped in the Rust type and in
// docs/rules/dsl-reference.md, but never reached the schema). Before this pin, the only test over
// the schema checked the four matcher-kind strings and three severity strings — the FIELD axis was
// unguarded, which is why the miss passed a green build.
//
// WHY THE MIRROR IS NOT SIMPLY GENERATED. Generating the schema from the Rust types (schemars or
// similar) would delete this whole drift class, and was rejected on cost: (a) it adds a derive
// dependency to `zzop-core` purely for a doc artifact; (b) the schema's value is almost entirely
// its AUTHOR-FACING prose, deliberately different in audience from the Rust doc comments — the
// schema says "${NAME} fragment reference supported", the Rust doc says "see
// `crate::dsl::string_mask`" — so generation would either drop the descriptions or force the Rust
// docs to be rewritten for pack authors; (c) the hand-authored envelope (`$id`, the TOLERANCE
// CONTRACT `$comment`, the `oneOf` tag layout) has no generator equivalent. The mirror stays
// authored; this pin makes the drift loud instead.
//
// HOW THE RUST SIDE IS DERIVED — one source, never a copied list. There is no reflection, and the
// matcher structs derive `Deserialize` only (no `Serialize`), so the envelope-schema parity trick
// of serializing a fully-populated sample and diffing its keys (see
// `crates/core/tests/envelope_schema_parity/`) is unavailable here. Instead the field axis is read
// straight out of `def/matcher.rs`'s SOURCE TEXT, embedded at compile time — the same technique
// `packages/mcp/tests/surface_prose.rs` uses on the CLI's `USAGE` const. That keeps the field list
// in exactly ONE place (the struct definitions themselves); this file never restates it.
//
// Serialization names, not Rust names, are the contract: a field-level `#[serde(rename = "...")]`
// is honoured, and `alias`/`flatten`/struct-level `rename_all` — none present today — fail loudly
// rather than being silently mis-mapped.

/// `zzop_core::dsl::def`'s matcher shapes, as TEXT. Embedded at compile time by relative path
/// (crates are plain workspace siblings), so editing the structs re-runs this pin.
///
/// TWO files, concatenated: `MethodScan` lives in the `def/matcher/method_scan.rs` submodule (the parent
/// file hit the repo's per-file line cap and that struct's field docs were four fifths of it). Every
/// file holding a matcher struct MUST be listed here — a struct in an unlisted file makes this pin fail
/// loud (`parse_struct_fields` panics on a missing header, and the struct-coverage pin below sees a short
/// list), never silently unguarded, which is the failure mode this whole section exists to prevent.
const MATCHER_SOURCE: &str = concat!(
    include_str!("../../core/src/dsl/def/matcher.rs"),
    include_str!("../../core/src/dsl/def/matcher/method_scan.rs"),
);

/// One `pub` field of a matcher struct, as declared in [`MATCHER_SOURCE`].
struct RustField {
    /// The name this field carries in JSON — `#[serde(rename = "...")]` when present, else the
    /// Rust identifier.
    json_name: String,
    /// The declared type is `Option<...>`: serde accepts an explicit `null` AND a missing key
    /// (serde's derive defaults every `Option` field to `None`), so the schema must declare it
    /// nullable and must NOT list it as required.
    optional: bool,
    /// The field carries `#[serde(default)]` / `#[serde(default = "...")]` — omittable, so not
    /// required.
    has_default: bool,
}

/// Extracts `#[serde(rename = "...")]`'s value from a field's attribute line.
fn serde_rename(attr: &str) -> Option<String> {
    let after = attr.split_once("rename = \"")?.1;
    Some(after.split_once('"')?.0.to_string())
}

/// Reads one `pub struct <name> { ... }` block out of [`MATCHER_SOURCE`] and returns its `pub`
/// fields. Deliberately a narrow line parser, not a Rust grammar: `def/matcher.rs` is one flat file
/// of plain struct declarations, and any shape this cannot read panics with the offending line
/// instead of silently returning a short field list (which would weaken the pin to nothing).
fn parse_struct_fields(struct_name: &str) -> Vec<RustField> {
    let header = format!("pub struct {struct_name} {{");
    let start = MATCHER_SOURCE.find(&header).unwrap_or_else(|| {
        panic!(
            "`{header}` not found in crates/core/src/dsl/def/matcher.rs — was the struct renamed \
             or moved? This parity pin must be updated with it."
        )
    });

    // Struct-level serde attributes sit in the contiguous attribute/doc block just above the
    // header. `rename_all` there would rewrite EVERY field's JSON name, which this parser does not
    // model — refuse rather than compare the wrong names.
    let attrs_above: String = MATCHER_SOURCE[..start]
        .lines()
        .rev()
        .take_while(|l| {
            let t = l.trim_start();
            t.starts_with("#[") || t.starts_with("///")
        })
        .collect();
    assert!(
        !attrs_above.contains("rename_all"),
        "{struct_name} carries a struct-level `rename_all` — every field's JSON name is rewritten \
         and this parity pin's name derivation is no longer valid. Teach `parse_struct_fields` the \
         casing rule before re-enabling."
    );

    let body = &MATCHER_SOURCE[start + header.len()..];
    let end = body.find("\n}").unwrap_or_else(|| {
        panic!("`{header}`'s body is not terminated by a `}}` at column 0 — cannot parse")
    });

    let mut fields = Vec::new();
    let mut pending_attr = String::new();
    for raw in body[..end].lines() {
        let line = raw.trim();
        if line.starts_with("///") || line.is_empty() {
            // Doc comments precede attributes, so a doc line can only mean "a new field starts
            // here" — drop anything staged for the previous one.
            pending_attr.clear();
            continue;
        }
        if line.starts_with("#[") {
            pending_attr.push_str(line);
            continue;
        }
        let Some(decl) = line.strip_prefix("pub ") else {
            panic!(
                "unparsable line in `{struct_name}`'s body (expected a doc comment, an attribute, \
                 or a `pub name: Type,` field): {line}"
            );
        };
        assert!(
            !pending_attr.contains("alias") && !pending_attr.contains("flatten"),
            "`{struct_name}` has a field with serde `alias`/`flatten` ({pending_attr}) — the JSON \
             name set is no longer one-to-one with the Rust field set. Decide how the schema \
             documents it (the envelope schema's convention: document aliases in the canonical \
             property's description) and teach this pin."
        );
        let (name, ty) = decl
            .split_once(": ")
            .unwrap_or_else(|| panic!("unparsable field declaration in `{struct_name}`: {line}"));
        fields.push(RustField {
            json_name: serde_rename(&pending_attr).unwrap_or_else(|| name.to_string()),
            optional: ty.trim_end_matches(',').starts_with("Option<"),
            has_default: pending_attr.contains("default"),
        });
        pending_attr.clear();
    }
    assert!(
        !fields.is_empty(),
        "parsed zero fields out of `{struct_name}` — the parser is broken, not the struct"
    );
    fields
}

/// Whether one schema property declares itself nullable, under this schema's conventions: a `type`
/// array containing `"null"`, or an inline `enum` list containing JSON `null` (how
/// `symbolScan.kind` spells `Option<SourceSymbolKind>`).
fn schema_property_is_nullable(prop: &serde_json::Value) -> bool {
    let list_has_null = |key: &str| {
        prop.get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|arr| arr.iter().any(serde_json::Value::is_null))
    };
    prop.get("type")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|arr| arr.iter().any(|t| t == "null"))
        || list_has_null("enum")
}

/// Compares one Rust matcher struct against the schema definition that mirrors it, on all three
/// axes the schema's own header promises ("field names, required-ness, and defaults below mirror
/// the Rust serde types field-for-field"): key NAMES both directions, REQUIRED-ness, NULLABILITY.
///
/// `tag` is the `#[serde(tag = "type")]` discriminator value for the four matcher kinds — a JSON
/// key with no Rust struct field behind it, so it is excluded from the field diff and instead
/// checked as a required `const` property. `None` for `LabeledPattern`, which is untagged.
fn assert_field_axis_parity(struct_name: &str, def_name: &str, tag: Option<&str>) {
    let schema: serde_json::Value =
        serde_json::from_str(RULE_PACK_SCHEMA).expect("rule-pack schema must be valid JSON");
    let def = schema
        .get("definitions")
        .and_then(|d| d.get(def_name))
        .unwrap_or_else(|| panic!("rule-pack schema is missing definitions.{def_name}"));
    let props = def
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("definitions.{def_name} has no `properties` object"));
    let declared_required: BTreeSet<&str> = def
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .map(|v| v.as_str().expect("`required` entries must be strings"))
                .collect()
        })
        .unwrap_or_default();

    if let Some(tag_value) = tag {
        let ty = props
            .get("type")
            .unwrap_or_else(|| panic!("definitions.{def_name} must declare the `type` tag"));
        assert_eq!(
            ty.get("const").and_then(serde_json::Value::as_str),
            Some(tag_value),
            "definitions.{def_name}.properties.type must be `const: \"{tag_value}\"` — the \
             kebab-case serde tag `zzop_core::dsl::def::Matcher` dispatches {struct_name} on"
        );
        assert!(
            declared_required.contains("type"),
            "definitions.{def_name} must list the `type` tag as required"
        );
    }

    let fields = parse_struct_fields(struct_name);
    let rust_keys: BTreeSet<&str> = fields.iter().map(|f| f.json_name.as_str()).collect();
    let schema_keys: BTreeSet<&str> = props
        .keys()
        .map(String::as_str)
        .filter(|k| !(tag.is_some() && *k == "type"))
        .collect();

    let missing: Vec<&&str> = rust_keys.difference(&schema_keys).collect();
    assert!(
        missing.is_empty(),
        "{struct_name} accepts field(s) {missing:?} that definitions.{def_name}.properties does \
         NOT document. `additionalProperties: true` means no validator breaks — the knob is simply \
         INVISIBLE to every pack author who reads the schema (including over MCP as \
         zzop://contract/rule-pack-schema). Add them to docs/contracts/rule-pack.schema.json."
    );
    let stale: Vec<&&str> = schema_keys.difference(&rust_keys).collect();
    assert!(
        stale.is_empty(),
        "definitions.{def_name}.properties documents field(s) {stale:?} that {struct_name} has no \
         field for — a rename, a removed field, or a typo. Packs authored against them load and \
         are silently ignored."
    );

    let rust_required: BTreeSet<&str> = fields
        .iter()
        .filter(|f| !f.has_default && !f.optional)
        .map(|f| f.json_name.as_str())
        .collect();
    let schema_required: BTreeSet<&str> = declared_required
        .iter()
        .copied()
        .filter(|k| !(tag.is_some() && *k == "type"))
        .collect();
    assert_eq!(
        rust_required, schema_required,
        "definitions.{def_name}.required disagrees with {struct_name}. The schema's own header \
         states the rule: a field is required exactly when its Rust field has neither \
         #[serde(default)] nor an Option type."
    );

    for f in &fields {
        let prop = props
            .get(&f.json_name)
            .unwrap_or_else(|| panic!("definitions.{def_name}.properties.{} missing", f.json_name));
        assert_eq!(
            schema_property_is_nullable(prop),
            f.optional,
            "definitions.{def_name}.properties.{} declares nullability the Rust field disagrees \
             with — an Option<T> field must include \"null\" in its `type` array (or its `enum` \
             list), and a non-Option field must not.",
            f.json_name
        );
    }
}

/// The whole point: every `pub` field of every matcher struct, its required-ness, and its
/// nullability must be visible in the schema an external pack author receives.
#[test]
fn matcher_field_axis_matches_the_schema() {
    assert_field_axis_parity("LineScan", "lineScan", Some("line-scan"));
    assert_field_axis_parity("MethodScan", "methodScan", Some("method-scan"));
    assert_field_axis_parity("SymbolScan", "symbolScan", Some("symbol-scan"));
    assert_field_axis_parity("IoScan", "ioScan", Some("io-scan"));
    assert_field_axis_parity("LabeledPattern", "labeledPattern", None);
}

/// Guards the pin itself: a FIFTH struct added to the matcher sources must be added to
/// [`matcher_field_axis_matches_the_schema`] above, or its fields go unguarded exactly the way
/// `after`/`after_in_same_function` did. Derived from the same source text, so it cannot go stale
/// silently.
///
/// The expected list is in [`MATCHER_SOURCE`] concatenation order, so it is file order first and
/// in-file order second — `MethodScan` sits last because it is the one struct that lives in the
/// submodule, not because anything about it changed.
#[test]
fn every_struct_in_the_matcher_source_is_covered_by_the_parity_pin() {
    let declared: Vec<&str> = MATCHER_SOURCE
        .match_indices("pub struct ")
        .map(|(i, m)| {
            let rest = &MATCHER_SOURCE[i + m.len()..];
            &rest[..rest
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .expect("a struct name must be followed by a delimiter")]
        })
        .collect();
    assert_eq!(
        declared,
        [
            "LineScan",
            "LabeledPattern",
            "SymbolScan",
            "IoScan",
            "MethodScan"
        ],
        "a matcher source file gained, lost, or reordered a struct — or a struct moved into a file \
         `MATCHER_SOURCE` does not include. Add the new one to \
         `matcher_field_axis_matches_the_schema` (with its schema definition), and any new file to \
         `MATCHER_SOURCE`, before updating this list."
    );
}
