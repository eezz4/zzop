//! The pin that lets other documents stop copying rule regexes: `zzop explain <rule-id>` must report
//! EVERY field of the matcher a rule declares — the positive scope (`file_pattern`, `require_file*`,
//! `line_pattern`/`patterns`/`key_pattern`/`name_pattern`, …) as well as the exclusions.
//!
//! Nine verbatim copies of `file_pattern` regexes were deleted from rule messages,
//! `docs/rules/catalog.md` and `site/rules.html` on 2026-08-01 on the strength of `explain` being the
//! canonical answer instead. That trade only holds while the answer is actually printed, and "a human
//! will notice if it stops" is exactly the assumption those copies falsified. So the field list is not
//! restated here: it is read out of the matcher structs' own SOURCE TEXT, the same technique
//! `crate::rule_pack_tests` uses for its schema parity pin and `packages/mcp/tests/surface_prose.rs`
//! uses for the CLI usage string. A field ADDED to a matcher and answered by no block fails this test
//! until `explain` answers for it; a field REMOVED from the output fails it immediately.
//!
//! Deliberately a name-level pin, not a byte-level one. The line for a given field may be reworded (the
//! `negate` lines name what they invert, `snippet_max` carries its "gates nothing" label) without
//! breaking this, but no field may go unmentioned. Value-level pins against the REAL shipped packs live
//! in `packages/cli-bin/tests/cli.rs`, where the regexes are the ones users actually read.

use super::{explain_over, Corpus};
use zzop_core::parse_dsl_pack;

/// The matcher shapes as TEXT — read at TEST TIME from the filesystem, `def/matcher.rs` first, then
/// EVERY `.rs` file under `def/matcher/` in filename order. DERIVED, never hand-listed: this used to
/// be a `concat!` of three `include_str!`s whose doc claimed "a struct in an unlisted file makes
/// [`struct_field_names`] panic … rather than silently vouching" — false for the only way a struct
/// actually arrives (in a NEW file), which is how `LiteralScan` sat outside this pin for one batch
/// while its sibling guard (`crate::rule_pack_tests::def_source`, same derivation, same incident)
/// was already fixed. That sibling has since been widened again, to the WHOLE `def/` tree, because
/// the directory pair below is still an enumeration and the pack ENVELOPE fell outside it; this one
/// keeps the narrower sweep deliberately, since its subject really is the matcher shapes (`explain`
/// renders matcher fields, and the envelope structs have no fixture to be missing). `struct_field_names` panics only for structs someone ASKS about, and
/// [`minimal_fixtures`] is that asker — so the fixture list is itself pinned against the derived
/// struct set by [`every_matcher_struct_in_the_sources_has_a_minimal_fixture`].
fn matcher_source() -> &'static str {
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src/dsl/def");
        let root = base.join("matcher.rs");
        let mut source = std::fs::read_to_string(&root)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()));
        let dir = base.join("matcher");
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|entry| entry.expect("readable dir entry").path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
            .collect();
        files.sort();
        for file in files {
            source.push_str(
                &std::fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display())),
            );
        }
        source
    })
}

/// Every `pub` field name of one matcher struct, read off [`matcher_source`]. A narrow line parser, not
/// a Rust grammar: these are flat structs of `pub name: Type,` lines, and anything else panics with the
/// offending line instead of returning a short list (which would weaken the pin to nothing).
fn struct_field_names(struct_name: &str) -> Vec<String> {
    let header = format!("pub struct {struct_name} {{");
    let start = matcher_source().find(&header).unwrap_or_else(|| {
        panic!("`{header}` not found in the embedded matcher sources — was it renamed or moved?")
    });
    let body = &matcher_source()[start + header.len()..];
    let end = body
        .find("\n}")
        .unwrap_or_else(|| panic!("`{header}`'s body is not terminated by a `}}` at column 0"));

    let mut names = Vec::new();
    for raw in body[..end].lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("///") {
            continue;
        }
        if let Some(attr) = line.strip_prefix("#[") {
            assert!(
                !attr.contains("rename") && !attr.contains("flatten") && !attr.contains("alias"),
                "`{struct_name}` has a field whose JSON name is no longer its Rust name ({line}) — \
                 `explain` prints JSON names, so teach this pin the mapping before re-enabling it"
            );
            continue;
        }
        let Some(decl) = line.strip_prefix("pub ") else {
            panic!("unparsable line in `{struct_name}`'s body: {line}");
        };
        let (name, _ty) = decl
            .split_once(": ")
            .unwrap_or_else(|| panic!("unparsable field declaration in `{struct_name}`: {line}"));
        names.push(name.to_string());
    }
    assert!(
        !names.is_empty(),
        "parsed zero fields out of `{struct_name}` — the parser is broken, not the struct"
    );
    names
}

/// Renders one fabricated single-rule pack through the real lookup. Fabricated rather than bundled
/// because no shipped pack uses `symbol-scan`, and because the pin must hold for a rule that sets only
/// the REQUIRED keys — that is the case where a renderer could most plausibly print nothing.
fn rendered(pack_json: &str, full_id: &str) -> String {
    let packs = vec![parse_dsl_pack(pack_json).expect("fixture pack must parse")];
    explain_over(&packs, &[], full_id, Corpus::Bundled).expect("fixture rule must resolve")
}

/// A minimal rule of each matcher kind — only the keys serde requires. Each is paired with the Rust
/// struct name whose field list it must fully account for.
fn minimal_fixtures() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "LineScan",
            "p/line",
            r#"{"id": "p", "rules": [{"id": "line", "severity": "info", "message": "m",
              "matcher": {"type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "TODO"}}]}"#
                .to_string(),
        ),
        (
            "MethodScan",
            "p/method",
            r#"{"id": "p", "rules": [{"id": "method", "severity": "info", "message": "m",
              "matcher": {"type": "method-scan", "file_pattern": "\\.ts$", "trigger": "t",
              "patterns": [{"label": "t", "pattern": "exec\\("}]}}]}"#
                .to_string(),
        ),
        (
            "SymbolScan",
            "p/symbol",
            r#"{"id": "p", "rules": [{"id": "symbol", "severity": "info", "message": "m",
              "matcher": {"type": "symbol-scan", "file_pattern": "\\.tsx$"}}]}"#
                .to_string(),
        ),
        (
            "IoScan",
            "p/io",
            r#"{"id": "p", "rules": [{"id": "io", "severity": "info", "message": "m",
              "matcher": {"type": "io-scan", "file_pattern": "\\.ts$", "direction": "provides"}}]}"#
                .to_string(),
        ),
        (
            "CallScan",
            "p/call",
            r#"{"id": "p", "rules": [{"id": "call", "severity": "info", "message": "m",
              "matcher": {"type": "call-scan", "file_pattern": "\\.ts$"}}]}"#
                .to_string(),
        ),
        (
            "LiteralScan",
            "p/literal",
            r#"{"id": "p", "rules": [{"id": "literal", "severity": "info", "message": "m",
              "matcher": {"type": "literal-scan", "file_pattern": "\\.ts$"}}]}"#
                .to_string(),
        ),
    ]
}

/// The fixture list above is a hand list, and a hand list is exactly what let `LiteralScan` sit
/// outside this pin for one batch (see [`matcher_source`]'s doc). This meta-pin closes that axis:
/// every `pub struct` in the derived matcher sources must have a fixture, except `LabeledPattern`,
/// whose two fields (`label`, `pattern`) are not a matcher of their own — they ride inside
/// `MethodScan`'s `patterns` and are rendered (and pinned) through that fixture's output.
#[test]
fn every_matcher_struct_in_the_sources_has_a_minimal_fixture() {
    let mut declared: Vec<String> = matcher_source()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub struct "))
        .filter_map(|rest| rest.split_once(' ').map(|(name, _)| name.to_string()))
        .collect();
    declared.sort();
    let mut covered: Vec<String> = minimal_fixtures()
        .into_iter()
        .map(|(name, _, _)| name.to_string())
        .collect();
    covered.push("LabeledPattern".to_string()); // rides MethodScan's `patterns` — see doc above
    covered.sort();
    assert_eq!(
        declared, covered,
        "a matcher struct exists in the sources with no minimal fixture (or a fixture names a \
         struct that no longer exists) — add the fixture so its fields join the explain pin"
    );
}

/// THE pin. Every `pub` field of every matcher struct must appear as its own `<name>: ` line, on a rule
/// that declares only the required keys. Anchored on the newline so `require_file` cannot be vouched
/// for by `require_file_all`'s line — the substring relation that would otherwise let a whole family
/// go unprinted while this stayed green.
#[test]
fn every_matcher_field_is_reachable_from_explain() {
    let mut offenders = Vec::new();
    for (struct_name, full_id, pack_json) in minimal_fixtures() {
        let out = rendered(&pack_json, full_id);
        for field in struct_field_names(struct_name) {
            if !out.contains(&format!("\n{field}: ")) {
                offenders.push(format!("{struct_name}.{field}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`zzop explain` reports nothing for these matcher fields, so no document can point at it as \
         the canonical answer for them: {offenders:#?}"
    );
}

/// The half the field-name sweep above cannot see: that the VALUE printed is the rule's own, not a
/// placeholder. `file_pattern` is checked most closely because it is the field the deleted copies
/// carried and the one "would this rule look at my file?" turns on.
#[test]
fn the_positive_scope_prints_the_rules_own_values_not_placeholders() {
    let out = rendered(
        r#"{"id": "p", "rules": [{"id": "line", "severity": "info", "message": "m",
          "matcher": {"type": "line-scan", "file_pattern": "(?i)\\.java$",
          "require_file": "javax\\.crypto", "require_file_all": ["Cipher", "getInstance"],
          "line_pattern": "ECB", "skip_comment_lines": true}}]}"#,
        "p/line",
    );
    for expected in [
        "\nfile_pattern: (?i)\\.java$",
        "\nrequire_file: javax\\.crypto",
        "\nrequire_file_all: Cipher, getInstance",
        "\nline_pattern: ECB",
        "\nany: no",
        "\nskip_comment_lines: yes",
        "\nstrip_string_literals: no",
    ] {
        assert!(
            out.contains(expected),
            "missing `{expected}` in the scope block: {out}"
        );
    }
}

/// The labelled-alternative shape, and the mirror of the case above: a rule using `any` instead of
/// `line_pattern` must print the labels AND patterns it actually scans for, with the unused sibling
/// honestly `no` rather than absent.
#[test]
fn a_line_scan_using_any_prints_every_labeled_alternative() {
    let out = rendered(
        r#"{"id": "p", "rules": [{"id": "line", "severity": "info", "message": "m",
          "matcher": {"type": "line-scan", "file_pattern": "\\.ts$",
          "any": [{"label": "sink", "pattern": "innerHTML"},
                  {"label": "write", "pattern": "document\\.write"}]}}]}"#,
        "p/line",
    );
    assert!(
        out.contains("\nany: sink=innerHTML, write=document\\.write"),
        "got: {out}"
    );
    assert!(out.contains("\nline_pattern: no"), "got: {out}");
}

/// `negate` never prints as a bare boolean — see `super::scope`'s header. A reader who sees
/// `key_pattern: <re>` and `negate: yes` must not conclude the rule fires ON that regex, which is what
/// a bare `yes` invites. The degenerate case (nothing to negate) is stated too, since the struct
/// documents it as "every entry passes" rather than "nothing passes".
#[test]
fn negate_names_the_field_it_inverts_instead_of_printing_a_bare_boolean() {
    let inverted = rendered(
        r#"{"id": "p", "rules": [{"id": "io", "severity": "info", "message": "m",
          "matcher": {"type": "io-scan", "file_pattern": "\\.ts$", "direction": "provides",
          "key_pattern": "^/api/v[0-9]+/", "negate": true}}]}"#,
        "p/io",
    );
    assert!(
        inverted.contains("\nnegate: yes (fires when key_pattern does NOT match)"),
        "got: {inverted}"
    );

    let nothing_to_invert = rendered(
        r#"{"id": "p", "rules": [{"id": "io", "severity": "info", "message": "m",
          "matcher": {"type": "io-scan", "file_pattern": "\\.ts$", "direction": "any",
          "negate": true}}]}"#,
        "p/io",
    );
    assert!(
        nothing_to_invert.contains("\nnegate: yes (no key_pattern to invert"),
        "got: {nothing_to_invert}"
    );
}

/// An unset FILTER prints `any`, not the `no` an unset pattern prints: `kind: no` would read as a rule
/// that can never fire, when an unset `kind` means every kind qualifies. Both spellings live in one
/// output here so the distinction is pinned rather than assumed.
#[test]
fn an_unrestricted_filter_prints_any_while_an_unset_pattern_prints_no() {
    let out = rendered(
        r#"{"id": "p", "rules": [{"id": "symbol", "severity": "info", "message": "m",
          "matcher": {"type": "symbol-scan", "file_pattern": "\\.tsx$"}}]}"#,
        "p/symbol",
    );
    assert!(out.contains("\nkind: any"), "got: {out}");
    assert!(out.contains("\nexported: any"), "got: {out}");
    assert!(out.contains("\nname_pattern: no"), "got: {out}");
}

/// `snippet_max` is reported, and reported as the one field that gates nothing — the alternative
/// (dropping it) would leave a reader unable to tell "this rule has no snippet cap" from "this command
/// does not say". The two kinds that carry NO snippet must not grow the line: reporting a field a
/// matcher does not have is the same fabrication in the other direction.
#[test]
fn snippet_max_is_printed_labelled_as_gating_nothing_and_only_where_it_exists() {
    for (id, json) in [
        (
            "p/line",
            r#"{"id": "p", "rules": [{"id": "line", "severity": "info", "message": "m",
              "matcher": {"type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "TODO",
              "snippet_max": 80}}]}"#,
        ),
        (
            "p/method",
            r#"{"id": "p", "rules": [{"id": "method", "severity": "info", "message": "m",
              "matcher": {"type": "method-scan", "file_pattern": "\\.ts$", "trigger": "t",
              "snippet_max": 80, "patterns": [{"label": "t", "pattern": "exec\\("}]}}]}"#,
        ),
    ] {
        let out = rendered(json, id);
        assert!(
            out.contains("\nsnippet_max: 80 (snippet truncation only — never decides whether"),
            "{id}: got: {out}"
        );
    }

    let io = rendered(
        r#"{"id": "p", "rules": [{"id": "io", "severity": "info", "message": "m",
          "matcher": {"type": "io-scan", "file_pattern": "\\.ts$", "direction": "provides"}}]}"#,
        "p/io",
    );
    assert!(
        !io.contains("snippet_max"),
        "io-scan carries no snippet_max — printing one would fabricate a field: {io}"
    );
}

/// The one gate that is a RULE field rather than a matcher field, so the source-derived sweep above
/// (which reads the matcher structs) cannot see it. Reported for EVERY matcher kind, in both
/// states, because "no line" and "not gated" must never look the same to a reader — and because the
/// value decides whether a rule judges `#[cfg(test)]` code, which is not something `explain` may leave
/// a user to infer. Both spellings are pinned so removing either branch is red.
#[test]
fn the_rule_level_test_region_gate_is_reported_in_both_states_for_every_matcher_kind() {
    for (_struct_name, full_id, pack_json) in minimal_fixtures() {
        let gated = rendered(&pack_json, full_id);
        assert!(
            gated.contains("\nscan_test_regions: no (a finding on a line a parser proved"),
            "{full_id}: the default (gated) state must be stated outright: {gated}"
        );
        // Same rule, one key added — the field is on the rule envelope, so this works for every kind.
        let opted_out = rendered(
            &pack_json.replace(
                r#""severity": "info""#,
                r#""severity": "info", "scan_test_regions": true"#,
            ),
            full_id,
        );
        assert!(
            opted_out
                .contains("\nscan_test_regions: yes (a credential at rest is committed either way"),
            "{full_id}: the opt-out must be visible and must say why it exists: {opted_out}"
        );
    }
}

/// `require_file_absent` is the one member of the `require_file*` family that only ever REMOVES, so it
/// is reported with the exclusions rather than with the positive pre-skips. Pinned because a reader
/// skimming the scope block would otherwise read it as "the file must contain this".
#[test]
fn require_file_absent_is_reported_on_the_exclusion_side_with_its_real_value() {
    let out = rendered(
        r#"{"id": "p", "rules": [{"id": "line", "severity": "info", "message": "m",
          "matcher": {"type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "setInterval",
          "require_file_absent": ["clearInterval"]}}]}"#,
        "p/line",
    );
    assert!(
        out.contains("\nrequire_file_absent: clearInterval"),
        "got: {out}"
    );
    let scope_end = out
        .find("\nexclude_pattern:")
        .expect("exclusion block must exist");
    let absent_at = out
        .find("\nrequire_file_absent:")
        .expect("the field must be printed");
    assert!(
        absent_at > scope_end,
        "require_file_absent must sit in the exclusion block, not among the positive pre-skips: {out}"
    );
}
