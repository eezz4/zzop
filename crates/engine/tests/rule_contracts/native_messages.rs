//! Contract 3: native message contract — pragmatic grep proxy, not a semantic proof (see docs below).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{collect_rs_files, load_all_packs, native_dir, native_metas};

/// Every `rules/native/*/src/**/*.rs` file, recursively (native rule crates nest modules, e.g.
/// `rules-graph/src/cross_layer/*.rs` — a non-recursive `*/src/*.rs` glob would miss those).
fn native_rs_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&native_dir(), &mut out);
    out.sort();
    out
}

/// Native findings are built in code (`Finding { rule_id: "...", message: format!(...), .. }`), not read
/// from declarative pack data the way contract 2 above inspects DSL `message` fields directly — there is no
/// single field this test can statically evaluate the way it can `RuleDef::message`. Instead this is a
/// pragmatic grep: read every native rule source file, and if the file contains a literal `rule_id: "`
/// token (i.e. it constructs at least one `Finding` for a hardcoded rule id), assert the SAME file also
/// EITHER contains the literal substring `disabled_rules` somewhere, OR calls the shared
/// `zzop_core::finding::disable_hint(` builder.
///
/// The two-leg OR is not incidental: a 2026-07-10 audit found the disable-hint fragment
/// (`` `rules: { "<id>": "off" }` (embedders: `disabled_rules`) ``) hand-written at ~34 native message call
/// sites, drifted at 31 of them, plus one plain-string (non-`format!`) site that shipped a literal `{{`
/// because a mechanical format!-escaping sweep assumed every site was inside a `format!` call. Every site
/// was converted to call `disable_hint`, the single builder that fragment now has (see its own doc comment
/// and unit tests in `crates/core/src/finding.rs`) — so the literal `disabled_rules` string itself moved
/// OUT of most native rule files and into that one function's `format!` body. A file whose message text is
/// spliced around `disable_hint`'s output (e.g. `rules/native/rules-http/src/route_shadowing.rs`, whose
/// sentence reads "...disable {tail}" rather than "...Disable via config...") may carry neither `rule_id: "`
/// co-located `disabled_rules` text NOR the literal substring at all anymore — only the `disable_hint(` call
/// site — so a check requiring the literal string alone would now false-positive on every converted file.
/// The `disabled_rules` leg stays (not just `disable_hint(`) because a file's own `#[cfg(test)]` module
/// legitimately still asserts `message.contains("disabled_rules")` as a regression pin, and a hand-authored
/// file that never adopts the helper but still spells out the convention correctly should not be forced to.
///
/// **What this proves**: a file that builds at least one `Finding` via a literal `rule_id: "..."`
/// assignment also EITHER names `disabled_rules` OR calls `disable_hint(` somewhere in its own source — in
/// every rule module audited while writing this test, that satisfies the "how to exclude" leg of the
/// finding's message, either directly (pre-sweep sites) or indirectly through the shared builder
/// (post-sweep sites, whose own `disable_hint` unit tests in `crates/core/src/finding.rs` pin the
/// `disabled_rules` mention at the one source of truth).
///
/// **What this CANNOT prove** (documented per the task's "keep it pragmatic" instruction, not silently
/// assumed):
/// - That the `disabled_rules` mention (or `disable_hint(` call) is actually inside the live
///   `Finding::message` value reaching the user, as opposed to a doc comment describing the convention or a
///   `#[cfg(test)]`-only assertion/import. This is a file-level co-occurrence check, not an AST-level check
///   tying either substring to a specific `Finding` construction site.
/// - That a rule id built dynamically (a variable, a format! expansion, a shared constructor in a
///   different file) is caught at all — only the literal token `rule_id: "` is detected, so a native rule
///   authored in an unusual shape can slip past this test silently.
/// - Anything about DSL packs or JS quick-rules — out of scope here; contract 2 above covers DSL directly,
///   since DSL `message` IS declarative data this crate can inspect precisely.
///
/// A failure here is a strong, actionable signal (the flagged file almost certainly ships a finding with no
/// exclude-hint), but is not a certainty — read the flagged file before assuming the fix is "add one
/// sentence to a format! string" or "call disable_hint."
#[test]
fn native_rule_files_that_build_findings_mention_disabled_rules() {
    let mut offenders = Vec::new();
    for path in native_rs_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("rule_id: \"")
            && !text.contains("disabled_rules")
            && !text.contains("disable_hint(")
        {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "native rule source files construct a Finding (literal `rule_id: \"...\"`) but never mention \
         `disabled_rules` and never call `disable_hint(` anywhere in the same file — the finding's message \
         likely omits the \"how to exclude\" hint every other native rule includes (see this test's own doc \
         comment for exactly what this check can/cannot prove): {offenders:#?}"
    );
}

/// Every literal `disable_hint("<id>")` argument in shipped native/engine source (a) names a KNOWN id
/// (native analysis id or `"<pack>/<rule>"` DSL id) and (b), when the same file also constructs findings
/// via literal `rule_id: "..."`, matches one of THOSE ids. The test above proves a hint exists; this one
/// proves the hint is not a lie — a hint naming a stale or copy-pasted-from-another-rule id sends the user
/// to disable the wrong thing, and the config entry they add becomes a silent no-op (the exact class the
/// unknown-disabled/override/suppression warnings were built to catch — this seals it at the SOURCE).
/// Same pragmatic-grep caveats as above: only literal `disable_hint("...")` and `rule_id: "..."` tokens
/// are seen; a dynamically built hint or id is invisible here. A 2026-07-13 sweep measured 0 violations
/// across 33 files / 35 literal call shapes; this pins that state.
#[test]
fn disable_hint_literal_args_are_known_ids_matching_the_files_own_findings() {
    fn quoted_after(text: &str, needle: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = text;
        while let Some(pos) = rest.find(needle) {
            let after = &rest[pos + needle.len()..];
            match after.find('"') {
                Some(end) => {
                    out.insert(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
        out
    }

    let mut known: BTreeSet<String> = native_metas().iter().map(|m| m.id.clone()).collect();
    for pack in load_all_packs() {
        for rule in &pack.rules {
            known.insert(format!("{}/{}", pack.id, rule.id));
        }
    }

    let mut files = native_rs_files();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );

    let mut offenders = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let hints = quoted_after(&text, "disable_hint(\"");
        if hints.is_empty() {
            continue;
        }
        let emitted = quoted_after(&text, "rule_id: \"");
        for hint in hints {
            if !known.contains(&hint) {
                offenders.push(format!(
                    "{}: disable_hint(\"{hint}\") names no known rule/analysis id",
                    path.display()
                ));
            } else if !emitted.is_empty() && !emitted.contains(&hint) {
                offenders.push(format!(
                    "{}: disable_hint(\"{hint}\") but this file's findings carry rule_id {:?}",
                    path.display(),
                    emitted.iter().collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "disable_hint(...) call sites whose literal id argument is stale, mistyped, or belongs to a \
         different rule than the file emits — the hint would send users to disable the wrong id (a silent \
         config no-op): {offenders:#?}"
    );
}
