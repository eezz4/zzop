//! `Matcher::CallScan` interpreter contracts, pinned against a HAND-SUPPLIED `SourceFile::call_sites` —
//! deliberately independent of every real producer: which languages project the channel is
//! `capability_matrix`'s claim, and where a producer's sites come from is that producer's own claim to
//! prove. What is pinned here is what the interpreter DOES with them, which is the half a producer
//! cannot test.
//!
//! Three of these are the invalidation set for the channel itself, and each answers a different way the
//! wiring could be quietly wrong rather than absent:
//!   1. an EMPTY channel must produce silence — if it produced findings, the fact would not be what the
//!      rule reads and the matcher would be a text scan in disguise;
//!   2. a hand-placed site must FIRE — if it did not, every producer would land into a channel that
//!      goes nowhere, and the failure would look like a parser bug;
//!   3. `in_loop` must DROP a site outside every `loop_spans` entry — the gate's whole value is the
//!      per-iteration claim, and a gate that passed everything would make that claim a guess.

use crate::finding::Finding;
use crate::{CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ};

use super::test_support::rule_pack;
use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

/// A `call-scan` rule over every `.ts` file, matching one `kind`. `extra` splices further matcher keys in
/// (`"callee_pattern": ...`, `"in_loop": true`).
fn call_pack(kind: &str, extra: &str) -> RulePackDef {
    rule_pack(&format!(
        r#"{{"id":"r","severity":"warning","message":"m",
           "matcher":{{"type":"call-scan","file_pattern":"\\.ts$","kind":"{kind}"{extra}}}}}"#
    ))
}

fn site(kind: &str, line: u32, callee: &str) -> CallSite {
    CallSite {
        kind: kind.to_string(),
        line,
        callee: callee.to_string(),
        algorithm: None,
    }
}

fn scan(
    pack: &RulePackDef,
    rel: &str,
    text: &str,
    call_sites: Vec<CallSite>,
    loop_spans: Vec<(u32, u32)>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        rel: rel.into(),
        text: text.into(),
        symbols: Vec::new(),
        io: None,
        loop_spans,
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites,

        string_literals: Vec::new(),
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(pack, &ctx)
}

const SRC: &str =
    "function f() {\n  console.error('boom');\n}\nfor (const x of xs) {\n  console.error(x);\n}\n";

#[test]
fn an_empty_channel_is_silence_even_when_the_text_says_otherwise() {
    // The source text contains `console.error` twice. A `line-scan` rule would fire; this one must not,
    // because the FACT is what it reads. This is the channel's degrade direction made observable: absence
    // is under-reporting, never a claim that the file is clean.
    let found = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
        "a.ts",
        SRC,
        Vec::new(),
        Vec::new(),
    );
    assert!(
        found.is_empty(),
        "a file with no projected call sites must produce no call-scan findings, whatever its text \
         says: {found:?}"
    );
}

#[test]
fn a_hand_placed_site_fires_and_carries_its_callee_and_kind() {
    let found = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
        "a.ts",
        SRC,
        vec![site(CALL_KIND_CONSOLE_WRITE, 2, "console.error")],
        Vec::new(),
    );
    assert_eq!(found.len(), 1, "expected exactly one finding: {found:?}");
    assert_eq!(found[0].line, 2, "the finding anchors on the site's line");
    let data = found[0]
        .data
        .as_ref()
        .expect("call-scan findings carry data");
    assert_eq!(data["callee"], "console.error");
    assert_eq!(data["kind"], CALL_KIND_CONSOLE_WRITE);
    assert_eq!(
        data["snippet"], "console.error('boom');",
        "the snippet is the site's own source line, trimmed"
    );
}

#[test]
fn in_loop_drops_a_site_outside_every_loop_span() {
    // Two identical sites, one at line 2 (module body) and one at line 5 (inside the 4..6 loop span).
    let sites = vec![
        site(CALL_KIND_CONSOLE_WRITE, 2, "console.error"),
        site(CALL_KIND_CONSOLE_WRITE, 5, "console.error"),
    ];
    let found = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, r#","in_loop":true"#),
        "a.ts",
        SRC,
        sites.clone(),
        vec![(4, 6)],
    );
    assert_eq!(
        found.iter().map(|f| f.line).collect::<Vec<_>>(),
        vec![5],
        "only the site inside a projected loop span survives `in_loop`: {found:?}"
    );

    // ... and with NO loop spans projected at all — the language-coverage gap case — the gate silences
    // both, rather than degrading to "every line counts". That is the direction `trigger_in_loop` already
    // takes, and the reason the capability matrix makes `in_loop` require `loop_spans` too.
    let none = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, r#","in_loop":true"#),
        "a.ts",
        SRC,
        sites,
        Vec::new(),
    );
    assert!(
        none.is_empty(),
        "with no loop spans projected, an `in_loop` rule must be silent, not permissive: {none:?}"
    );
}

#[test]
fn kind_is_an_exact_match_and_callee_pattern_narrows_within_it() {
    let sites = vec![
        site(CALL_KIND_CONSOLE_WRITE, 2, "console.error"),
        site(CALL_KIND_CONSOLE_WRITE, 5, "console.log"),
        site(CALL_KIND_ENV_READ, 2, "process.env.HOME"),
    ];
    let all = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
        "a.ts",
        SRC,
        sites.clone(),
        Vec::new(),
    );
    assert_eq!(
        all.len(),
        2,
        "`kind` selects the family exactly — the env-read site is another family: {all:?}"
    );

    let narrowed = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, r#","callee_pattern":"\\.error$""#),
        "a.ts",
        SRC,
        sites,
        Vec::new(),
    );
    assert_eq!(
        narrowed
            .iter()
            .map(|f| f.data.as_ref().unwrap()["callee"]
                .as_str()
                .unwrap()
                .to_string())
            .collect::<Vec<_>>(),
        vec!["console.error".to_string()],
        "`callee_pattern` matches the raw spelling, which is the only place the level token lives"
    );
}

#[test]
fn file_pattern_and_file_exclude_pattern_gate_the_file_before_any_site_is_read() {
    let sites = vec![site(CALL_KIND_CONSOLE_WRITE, 2, "console.error")];
    let wrong_ext = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
        "a.py",
        SRC,
        sites.clone(),
        Vec::new(),
    );
    assert!(
        wrong_ext.is_empty(),
        "file_pattern gates the file: {wrong_ext:?}"
    );

    let excluded = scan(
        &call_pack(
            CALL_KIND_CONSOLE_WRITE,
            r#","file_exclude_pattern":"^scripts/""#,
        ),
        "scripts/a.ts",
        SRC,
        sites,
        Vec::new(),
    );
    assert!(
        excluded.is_empty(),
        "file_exclude_pattern is the path-negation escape hatch: {excluded:?}"
    );
}

#[test]
fn a_suppress_marker_works_in_both_a_slash_and_a_hash_comment() {
    // The marker is derived as `zzop-<rule id>-ok`; this pack's rule id is `r`. Both leaders are honored
    // because the channel is multi-language by construction — a Python `#` comment must suppress exactly
    // like a TypeScript `//` one.
    for text in [
        "x();\nconsole.error('boom'); // zzop-r-ok\n",
        "x();\nconsole.error('boom')  # zzop-r-ok\n",
    ] {
        let found = scan(
            &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
            "a.ts",
            text,
            vec![site(CALL_KIND_CONSOLE_WRITE, 2, "console.error")],
            Vec::new(),
        );
        assert!(
            found.is_empty(),
            "marker must suppress in {text:?}: {found:?}"
        );
    }
}

#[test]
fn a_site_whose_line_the_text_cannot_supply_still_fires_with_an_empty_snippet() {
    // Envelope mode is the real instance: `SourceFile::text` is empty there. The site is the evidence;
    // the line text is only a courtesy for the snippet and the marker window, so its absence must not
    // silently delete a finding.
    let found = scan(
        &call_pack(CALL_KIND_CONSOLE_WRITE, ""),
        "a.ts",
        "",
        vec![site(CALL_KIND_CONSOLE_WRITE, 7, "console.error")],
        Vec::new(),
    );
    assert_eq!(found.len(), 1, "expected the finding to survive: {found:?}");
    assert_eq!(found[0].line, 7);
    assert_eq!(found[0].data.as_ref().unwrap()["snippet"], "");
}
