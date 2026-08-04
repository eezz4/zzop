//! `Matcher::LiteralScan` interpreter contracts, pinned against a HAND-SUPPLIED
//! `SourceFile::string_literals` — deliberately independent of every real producer, exactly as
//! `tests_call_scan` is for its channel: which languages project it is `capability_matrix`'s claim;
//! what the interpreter DOES with a projected entry is the half a producer cannot test.
//!
//! The invalidation set, one test per way the wiring could be quietly wrong rather than absent:
//!   1. an EMPTY channel must produce silence even when the raw text spells a secret — otherwise the
//!      matcher would be a text scan in disguise;
//!   2. a hand-placed entry must FIRE — otherwise every producer lands into a channel that goes nowhere;
//!   3. `entropy_min` must DROP a low-entropy entry — the gate IS the rule's judgment;
//!   4. `skip_value_equals_name` must DROP the sentinel shape and ONLY the exact-equality shape.

use crate::finding::Finding;
use crate::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

use super::test_support::rule_pack;
use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

/// A `literal-scan` rule over every `.ts` file. `extra` splices further matcher keys in
/// (`"entropy_min": 80.0`, `"skip_value_equals_name": true`, ...).
fn literal_pack(extra: &str) -> RulePackDef {
    rule_pack(&format!(
        r#"{{"id":"r","severity":"warning","message":"m",
           "matcher":{{"type":"literal-scan","file_pattern":"\\.ts$"{extra}}}}}"#
    ))
}

fn entry(name: &str, line: u32, value: &str) -> BoundStringLiteral {
    BoundStringLiteral {
        name: name.to_string(),
        line,
        value_hash: value_hash_hex(value),
        entropy: shannon_entropy_bits(value),
    }
}

fn scan(
    pack: &RulePackDef,
    rel: &str,
    text: &str,
    string_literals: Vec<BoundStringLiteral>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        rel: rel.into(),
        text: text.into(),
        symbols: Vec::new(),
        io: None,
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(pack, &ctx)
}

const SRC: &str = "const apiKey = 'correct-horse-battery-staple';\nconst kind = 'refresh_token';\n";

#[test]
fn an_empty_channel_is_silence_even_when_the_text_spells_a_secret() {
    let found = scan(&literal_pack(""), "a.ts", SRC, Vec::new());
    assert!(
        found.is_empty(),
        "a file with no projected literals must produce no literal-scan findings, whatever its text \
         says: {found:?}"
    );
}

#[test]
fn a_projected_entry_fires_and_carries_name_and_entropy_not_the_hash() {
    let found = scan(
        &literal_pack(r#","name_pattern":"(?i)key""#),
        "a.ts",
        SRC,
        vec![entry("apiKey", 1, "correct-horse-battery-staple")],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 1);
    let data = found[0].data.as_ref().expect("data");
    assert_eq!(data["name"], "apiKey");
    assert!(data["entropy"].as_f64().expect("entropy") > 90.0);
    // The no-value contract extends to the finding, in BOTH forms. Hash: a 64-bit unsalted hash of
    // a REAL secret is dictionary-crackable. Snippet: the literal's own source line carries the
    // value verbatim — a `snippet` here once put the plaintext secret into
    // `.zzop/cache/findings/*.json`, stdout and MCP replies (H1). `data` is exactly
    // `{name, entropy}` so no future field can smuggle the value back without turning this red.
    assert!(
        data.get("valueHash").is_none() && data.get("value_hash").is_none(),
        "the value hash must not ride the finding: {data}"
    );
    assert!(
        data.get("snippet").is_none(),
        "the source-line snippet must not ride a literal-scan finding: {data}"
    );
    assert_eq!(
        data.as_object().expect("object").keys().collect::<Vec<_>>(),
        ["entropy", "name"],
        "literal-scan finding data is exactly {{name, entropy}}: {data}"
    );
    assert!(
        !serde_json::to_string(&found[0])
            .expect("json")
            .contains("correct-horse-battery-staple"),
        "no serialized form of the finding may carry the literal's value"
    );
}

#[test]
fn the_entropy_floor_drops_a_low_entropy_entry_and_keeps_a_passphrase() {
    let pack = literal_pack(r#","entropy_min":80.0"#);
    let sites = vec![
        entry("password", 1, "correct-horse-battery-staple"), // 97.9 bits
        entry("password", 2, "test-key"),                     // ~20 bits
        entry("secret", 3, "PlaceholderSecretValue"),         // 75.7 bits — the decoy landmark
    ];
    let found = scan(&pack, "a.ts", SRC, sites);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 1);
}

#[test]
fn value_equals_name_is_vetoed_exactly_and_only_exactly() {
    let pack = literal_pack(r#","skip_value_equals_name":true"#);
    let sites = vec![
        entry("refresh_token", 1, "refresh_token"), // the sentinel — must be silent
        entry("refresh_token", 2, "Refresh_Token"), // case differs = NOT equal, hash cannot fold case
    ];
    let found = scan(&pack, "a.ts", SRC, sites);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 2);
}

#[test]
fn name_exclude_pattern_vetoes_mock_shaped_names() {
    // The boundary-anchored form the shipped pack uses (name-adapted twin of `hardcoded-secret`'s
    // value-side veto), not a bare substring: `latestToken` and `attestationSecret` EMBED `test`
    // without any name boundary and must keep firing (N1 — a substring veto silenced both).
    let pack = literal_pack(concat!(
        r#","name_exclude_pattern":"(^|[-_])(?i:mock|test|fake)([-_]|\\d|[A-Z]|$)"#,
        r#"|[a-z](Mock|Test|Fake)([-_]|\\d|[A-Z]|$)""#
    ));
    let sites = vec![
        entry("mockApiKey", 1, "correct-horse-battery-staple"), // name-start boundary — vetoed
        entry("apiKey", 2, "correct-horse-battery-staple"),     // clean name — fires
        entry("latestToken", 3, "correct-horse-battery-staple"), // boundary-adjacent — fires
        entry("attestationSecret", 4, "correct-horse-battery-staple"), // boundary-adjacent — fires
        entry("myTestToken", 5, "correct-horse-battery-staple"), // camelCase interior — vetoed
        entry("test_secret", 6, "correct-horse-battery-staple"), // separator boundary — vetoed
    ];
    let found = scan(&pack, "a.ts", SRC, sites);
    let lines: Vec<u32> = found.iter().map(|f| f.line).collect();
    assert_eq!(lines, vec![2, 3, 4], "{found:?}");
}

#[test]
fn a_suppress_marker_above_the_line_suppresses_via_slash_or_hash() {
    let text = "# zzop-r-ok: vetted\npassword = 'correct-horse-battery-staple'\n";
    let found = scan(
        &literal_pack(""),
        "a.ts",
        text,
        vec![entry("password", 2, "correct-horse-battery-staple")],
    );
    assert!(
        found.is_empty(),
        "hash-leader marker must suppress: {found:?}"
    );
}
