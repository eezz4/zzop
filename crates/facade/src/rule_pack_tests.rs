//! Unit tests for `validate_rule_pack_json` (`crate::rule_pack`) — the pre-load, structure-only
//! rule-pack check — plus the rule-pack JSON Schema's own parity with `zzop_core::dsl::def`'s
//! types (see the FIELD-AXIS PARITY PIN section at the bottom of this file).

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
    // `pack_field_axis_matches_the_schema` below — not by a loose substring sweep here.
}

// ---------------------------------------------------------------------------------------------
// FIELD-AXIS PARITY PIN
// ---------------------------------------------------------------------------------------------
//
// WHY THIS EXISTS. `docs/contracts/rule-pack.schema.json` is a HAND-AUTHORED mirror of
// `zzop_core::dsl::def`'s serde structs, and `zzop-summary` embeds it verbatim
// (`crates/summary/src/contracts.rs`) to serve as the MCP resource `zzop://contract/rule-pack-schema`.
// An external pack author reads THAT, not the Rust source. Because the schema declares
// `additionalProperties: true` everywhere, a field the schema forgot breaks no validator — it just
// makes the knob invisible, and an author who cannot see e.g. an ordering gate builds an
// order-asserting rule out of plain co-occurrence instead. Silent, and exactly the failure the
// audit found (`after` / `after_in_same_function` shipped in the Rust type and in
// docs/rules/dsl-reference.md, but never reached the schema). Before this pin, the only test over
// the schema checked the matcher-kind strings and three severity strings — the FIELD axis was
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
// pack types derive `Deserialize` only (no `Serialize`), so the envelope-schema parity trick
// of serializing a fully-populated sample and diffing its keys (see
// `crates/core/tests/envelope_schema_parity/`) is unavailable here. Instead the field axis is read
// straight out of the structs' SOURCE TEXT, read from the filesystem at test time — the WHOLE
// `crates/core/src/dsl/def/` tree, swept recursively, never file-listed (see `def_source`'s doc for
// why the file set itself must be derived: a hand-kept subject list is the same silent-drift class
// one level up, and it has now bitten twice). That keeps the field list in exactly ONE place (the
// struct definitions themselves); this file never restates it.
//
// Serialization names, not Rust names, are the contract: a field-level `#[serde(rename = "...")]`
// is honoured, `#[serde(skip)]` fields are excluded (they are never on the wire), and
// `alias`/`flatten`/struct-level `rename_all` — none present today — fail loudly rather than being
// silently mis-mapped.

/// `zzop_core::dsl::def`'s pack shapes, as TEXT — read at TEST TIME from the filesystem
/// (`CARGO_MANIFEST_DIR`-relative; crates are plain workspace siblings): EVERY `.rs` file anywhere
/// under `crates/core/src/dsl/def/`, concatenated in sorted-path order.
///
/// DERIVED, never hand-listed — and the subject set has had to be widened twice, both times for the
/// same reason, which is why it is now the whole directory rather than anything narrower:
///
/// 1. It began as a `concat!` of three `include_str!`s, each file named by hand. When `LiteralScan`
///    landed in a NEW file (`def/matcher/literal_scan.rs`), nothing here failed: every mechanism the
///    doc pointed at fires only for structs the list already knows about (`parse_struct_fields`
///    panics on a missing header only when someone ASKS for that struct; the coverage pin can only
///    see structs in the text it was given), so a whole file outside the list was invisible to both.
/// 2. The fix swept `def/matcher.rs` + `def/matcher/`, which was still an enumeration — it just
///    spelled the subject set in DIRECTORIES instead of files. On 2026-08-12 `RuleDef` gained an
///    `axis` field whose type moved into a new `def/axis.rs`, and the ENVELOPE structs
///    (`RulePackDef`, `RuleDef` in `def/mod.rs`) had never been in scope at all — so the shipped
///    schema documented five of `RuleDef`'s six fields and this pin was green. The v0.30.0 release
///    audit found it by reading the schema, not by running anything.
///
/// The lesson both incidents teach is the repo's own rule about guards whose subject set is written
/// down: the enumeration cannot grow when the topic does. The subject is now "the serde types that
/// deserialize a rule pack", and the directory that holds them IS that set, so a new file — or a
/// new struct in an existing one — joins this pin by existing. The only remaining hand-step is the
/// one that cannot be derived: naming the schema node each struct mirrors, in
/// [`pack_field_axis_matches_the_schema`], and the coverage pin below is what makes skipping that
/// step impossible.
///
/// Reads panic loudly (a moved/renamed directory must break this pin, not soften it).
fn def_source() -> &'static str {
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let entries = std::fs::read_dir(dir)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
            for entry in entries {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    collect(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src/dsl/def");
        let mut files = Vec::new();
        collect(&base, &mut files);
        files.sort();
        assert!(
            files.len() >= 4,
            "swept only {} .rs file(s) out of {} — the directory moved or the walk broke, and every \
             comparison below would then be against a nearly empty text",
            files.len(),
            base.display()
        );
        let mut source = String::new();
        for file in files {
            source.push_str(
                &std::fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display())),
            );
        }
        source
    })
}

/// One `pub` field of a pack-definition struct, as declared in [`def_source`]'s text.
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

/// Whether a field's attribute block carries `#[serde(skip)]` — the field is not on the wire at
/// all, so the schema must NOT document it (`RulePackDef::regex_cache` is the one today).
///
/// Matched as a whole serde word, never as a substring: `skip_serializing_if` (`MethodScan::after`
/// carries it) says nothing about deserialization, and reading it as `skip` would silently drop a
/// real authored field out of this pin's subject set — the same false-green shape this whole
/// section exists to prevent.
fn has_serde_skip(attr: &str) -> bool {
    attr.match_indices("skip").any(|(i, m)| {
        attr[i + m.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
    })
}

/// Reads one `pub struct <name> { ... }` block out of [`def_source`]'s text and returns its
/// wire-visible `pub` fields. Deliberately a narrow line parser, not a Rust grammar: the pack
/// definition sources are flat files of plain struct declarations, and any shape this cannot read
/// panics with the offending line instead of silently returning a short field list (which would
/// weaken the pin to nothing).
fn parse_struct_fields(struct_name: &str) -> Vec<RustField> {
    let source = def_source();
    let header = format!("pub struct {struct_name} {{");
    let start = source.find(&header).unwrap_or_else(|| {
        panic!(
            "`{header}` not found in any .rs file under crates/core/src/dsl/def/ — was the struct \
             renamed or moved outside that directory? This parity pin must be updated with it."
        )
    });

    // Struct-level serde attributes sit in the contiguous attribute/doc block just above the
    // header. `rename_all` there would rewrite EVERY field's JSON name, which this parser does not
    // model — refuse rather than compare the wrong names.
    let attrs_above: String = source[..start]
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

    let body = &source[start + header.len()..];
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
        if has_serde_skip(&pending_attr) {
            pending_attr.clear();
            continue;
        }
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

/// Compares one Rust struct against the schema node that mirrors it, on all three axes the
/// schema's own header promises ("field names, required-ness, and defaults below mirror the Rust
/// serde types field-for-field"): key NAMES both directions, REQUIRED-ness, NULLABILITY.
///
/// `def_name` names the node under `definitions`; `None` targets the schema ROOT, which is what
/// `RulePackDef` mirrors — the pack envelope is not a `definitions` entry, and treating "the root
/// is not a definition" as a reason to leave it unpinned is how `RuleDef::axis` shipped
/// undocumented (see [`def_source`]'s incident 2).
///
/// `tag` is the `#[serde(tag = "type")]` discriminator value for the matcher kinds — a JSON
/// key with no Rust struct field behind it, so it is excluded from the field diff and instead
/// checked as a required `const` property. `None` for the untagged shapes.
fn assert_field_axis_parity(struct_name: &str, def_name: Option<&str>, tag: Option<&str>) {
    let schema: serde_json::Value =
        serde_json::from_str(RULE_PACK_SCHEMA).expect("rule-pack schema must be valid JSON");
    let at = def_name.map_or_else(
        || "the schema root".to_string(),
        |d| format!("definitions.{d}"),
    );
    let def = match def_name {
        Some(d) => schema
            .get("definitions")
            .and_then(|defs| defs.get(d))
            .unwrap_or_else(|| panic!("rule-pack schema is missing {at}")),
        None => &schema,
    };
    let props = def
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("{at} has no `properties` object"));
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
            .unwrap_or_else(|| panic!("{at} must declare the `type` tag"));
        assert_eq!(
            ty.get("const").and_then(serde_json::Value::as_str),
            Some(tag_value),
            "{at}.properties.type must be `const: \"{tag_value}\"` — the kebab-case serde tag \
             `zzop_core::dsl::def::Matcher` dispatches {struct_name} on"
        );
        assert!(
            declared_required.contains("type"),
            "{at} must list the `type` tag as required"
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
        "{struct_name} accepts field(s) {missing:?} that {at}.properties does NOT document. \
         `additionalProperties: true` means no validator breaks — the knob is simply INVISIBLE to \
         every pack author who reads the schema (including over MCP as \
         zzop://contract/rule-pack-schema, served from a copy baked into the shipped binary). Add \
         them to docs/contracts/rule-pack.schema.json."
    );
    let stale: Vec<&&str> = schema_keys.difference(&rust_keys).collect();
    assert!(
        stale.is_empty(),
        "{at}.properties documents field(s) {stale:?} that {struct_name} has no field for — a \
         rename, a removed field, or a typo. Packs authored against them load and are silently \
         ignored."
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
        "{at}.required disagrees with {struct_name}. The schema's own header states the rule: a \
         field is required exactly when its Rust field has neither #[serde(default)] nor an Option \
         type."
    );

    for f in &fields {
        let prop = props
            .get(&f.json_name)
            .unwrap_or_else(|| panic!("{at}.properties.{} missing", f.json_name));
        assert_eq!(
            schema_property_is_nullable(prop),
            f.optional,
            "{at}.properties.{} declares nullability the Rust field disagrees with — an Option<T> \
             field must include \"null\" in its `type` array (or its `enum` list), and a non-Option \
             field must not.",
            f.json_name
        );
    }
}

/// Which schema node each pack-definition struct mirrors — `(struct, definitions entry, serde tag)`.
///
/// The ONE hand-written thing in this section, because it is the one thing no sweep can derive: the
/// schema's node names are authored English (`lineScan`, `rule`), not a function of the Rust name.
/// It is a MAPPING and never a subject list — the subject set is [`def_source`]'s directory sweep,
/// and `every_struct_in_the_pack_definition_source_has_a_schema_mirror` below asserts this table
/// covers exactly that set. A struct with no row here therefore fails the build instead of being
/// quietly skipped, which is the property the previous shape lacked: the coverage pin used to
/// compare the swept structs against a SECOND hand list of names, so deleting a row from the parity
/// call site went unnoticed as long as the name list still matched.
///
/// `None` for the definitions entry targets the schema ROOT, which is what the pack envelope
/// mirrors. `None` for the tag means the shape carries no `#[serde(tag = "type")]` discriminator.
///
/// The ENVELOPE rows (`RulePackDef`, `RuleDef`) were added on 2026-08-13 by the v0.30.0 release
/// audit. Their absence is why `RuleDef::axis` — declared by EVERY shipped rule and enforced by
/// `rule_contracts::rule_axis::every_shipped_rule_declares_its_axis` — reached a release candidate
/// with no property in the schema baked into the binary.
const SCHEMA_MIRRORS: &[(&str, Option<&str>, Option<&str>)] = &[
    ("RulePackDef", None, None),
    ("RuleDef", Some("rule"), None),
    ("LineScan", Some("lineScan"), Some("line-scan")),
    ("MethodScan", Some("methodScan"), Some("method-scan")),
    ("SymbolScan", Some("symbolScan"), Some("symbol-scan")),
    ("IoScan", Some("ioScan"), Some("io-scan")),
    ("CallScan", Some("callScan"), Some("call-scan")),
    ("LiteralScan", Some("literalScan"), Some("literal-scan")),
    ("LabeledPattern", Some("labeledPattern"), None),
    ("PackExport", Some("packExport"), None),
];

/// The whole point: every wire-visible `pub` field of every pack-definition struct, its
/// required-ness, and its nullability must be visible in the schema an external pack author
/// receives.
#[test]
fn pack_field_axis_matches_the_schema() {
    for (struct_name, def_name, tag) in SCHEMA_MIRRORS {
        assert_field_axis_parity(struct_name, *def_name, *tag);
    }
}

/// Guards the pin itself: a NEW struct in the pack-definition sources must gain a
/// [`SCHEMA_MIRRORS`] row, or its fields go unguarded exactly the way `after`/`after_in_same_function`
/// did, and `axis` did after them. Both directions — a struct with no row is unguarded, and a row
/// naming a struct that no longer exists would make `parse_struct_fields` panic with a confusing
/// message about a missing header.
///
/// Derived from the same directory-swept source text as the field parser ([`def_source`]), so a
/// struct in a brand-new `def/**/*.rs` file surfaces here with NO edit anywhere.
#[test]
fn every_struct_in_the_pack_definition_source_has_a_schema_mirror() {
    let source = def_source();
    let declared: BTreeSet<&str> = source
        .match_indices("pub struct ")
        .map(|(i, m)| {
            let rest = &source[i + m.len()..];
            &rest[..rest
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .expect("a struct name must be followed by a delimiter")]
        })
        .collect();
    assert!(
        declared.len() >= 7,
        "swept only {} struct(s) out of crates/core/src/dsl/def/ — the `pub struct` needle stopped \
         matching and this pin would vouch for a table it barely read",
        declared.len()
    );
    let mirrored: BTreeSet<&str> = SCHEMA_MIRRORS.iter().map(|(s, _, _)| *s).collect();
    assert_eq!(
        declared,
        mirrored,
        "crates/core/src/dsl/def/ and SCHEMA_MIRRORS disagree about which structs exist. Unmirrored \
         (their fields are invisible to the parity pin, and so may be invisible to every pack \
         author): {:?}. Mirrored but gone from the sources (a rename or a deletion): {:?}.",
        declared.difference(&mirrored).collect::<Vec<_>>(),
        mirrored.difference(&declared).collect::<Vec<_>>()
    );
}

/// The VALUE axis of `RuleAxis`, which the field axis above cannot reach: `pack_field_axis_...`
/// proves the schema has an `axis` property, not that it offers the right two words or the right
/// default. Both halves of that are hand-copied prose in the schema, and both are load-bearing —
/// the vocabulary is what a third-party author types, and the default is the difference between
/// "this rule reports a bug" and "this rule reports a preference" for every rule that says nothing.
///
/// Derived from `def/axis.rs`'s source text via [`def_source`], the same way the shapes are: the
/// kebab spelling is COMPUTED from each variant name, so the `rename_all = "kebab-case"` attribute
/// that makes that computation correct is asserted rather than assumed (a switch to snake_case
/// would otherwise leave this comparing against a spelling nothing accepts), and the schema's
/// `default` is checked against whichever variant carries `#[default]` rather than against the word
/// "defect".
#[test]
fn the_rule_axis_vocabulary_and_default_match_the_schema() {
    let source = def_source();
    let header = "pub enum RuleAxis {";
    let start = source
        .find(header)
        .expect("`pub enum RuleAxis {` must exist under crates/core/src/dsl/def/");
    assert!(
        source[..start]
            .lines()
            .rev()
            .take(3)
            .any(|l| l.contains("rename_all = \"kebab-case\"")),
        "`RuleAxis` no longer carries #[serde(rename_all = \"kebab-case\")] directly above it — \
         every wire spelling below is DERIVED from a variant name on that assumption, and without \
         it this test compares the schema against words nothing deserializes."
    );

    let body = &source[start + header.len()..];
    let end = body.find("\n}").expect("`RuleAxis`'s body must terminate");
    let mut variants: Vec<String> = Vec::new();
    let mut default_variant: Option<String> = None;
    let mut pending_default = false;
    for raw in body[..end].lines() {
        let line = raw.trim();
        if line.starts_with("///") || line.is_empty() {
            // Doc comments precede attributes, so a doc line can only mean "a new variant starts
            // here" — drop anything staged for the previous one.
            pending_default = false;
            continue;
        }
        if line.starts_with("#[") {
            pending_default |= line.contains("default");
            continue;
        }
        let name = line.trim_end_matches(',');
        assert!(
            name.chars().all(|c| c.is_alphanumeric()),
            "unparsable line in `RuleAxis`'s body (expected a doc comment, an attribute, or a \
             bare variant): {line}"
        );
        let mut kebab = String::new();
        for (i, ch) in name.char_indices() {
            if ch.is_uppercase() {
                if i > 0 {
                    kebab.push('-');
                }
                kebab.extend(ch.to_lowercase());
            } else {
                kebab.push(ch);
            }
        }
        if std::mem::take(&mut pending_default) {
            default_variant = Some(kebab.clone());
        }
        variants.push(kebab);
    }
    assert!(
        variants.len() >= 2,
        "parsed {} variant(s) out of `RuleAxis` — the parser is broken, not the enum, and a \
         comparison against a near-empty set would pass for the wrong reason",
        variants.len()
    );

    let schema: serde_json::Value =
        serde_json::from_str(RULE_PACK_SCHEMA).expect("rule-pack schema must be valid JSON");
    let axis = &schema["definitions"]["rule"]["properties"]["axis"];
    let schema_values: Vec<String> = axis["enum"]
        .as_array()
        .expect("definitions.rule.properties.axis must declare an `enum` list")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("axis enum entries must be strings")
                .to_string()
        })
        .collect();
    assert_eq!(
        variants, schema_values,
        "definitions.rule.properties.axis.enum does not match `RuleAxis`'s variants in kebab-case. \
         A word the schema omits is one no third-party author will ever write; a word it invents is \
         one the loader rejects."
    );
    assert_eq!(
        axis["default"].as_str().map(str::to_string),
        default_variant,
        "definitions.rule.properties.axis.default disagrees with the variant `RuleAxis` marks \
         #[default]. That value is what EVERY rule which omits the field silently loads as, so a \
         stale word here misreports the axis of every undeclared rule in every third-party pack."
    );
}
