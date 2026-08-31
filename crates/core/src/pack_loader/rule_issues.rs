//! The dead-rule census: every way a pack's rule can PARSE fine and still never be able to fire.
//!
//! Split out of `pack_loader.rs` (300-line cap) and kept as one function because both defect classes it
//! reports — an uncompilable regex and a structurally empty matcher — have the same consequence and the
//! same remedy. Two callers share it: `validate-rule-pack` (pre-scan, one pack) and the engine's
//! `uncompilable_rule_warnings` (per run, every loaded pack), which is why the ids are pack-qualified.

use crate::dsl::{for_each_pattern_field, Matcher, RulePackDef};

/// Every rule in `pack` that can never fire, as one issue string each (deterministic: rule order, then
/// field order within the matcher, with structural lines after the regex lines of the same rule).
///
/// Two defect classes, one list:
/// - a regex-typed field that does not compile — the exact judgment the DSL interpreter applies at eval
///   time (`regex::Regex::new(p)` failing), where the interpreter's contract is to silently no-op the
///   affected rule (see `dsl::line_scan`/`method_scan`/`ir_scan`/`call_scan`) rather than panic;
/// - a matcher with nothing to match — a line-scan declaring neither `line_pattern` nor `any`, or a
///   method-scan whose `trigger` names a label no `patterns` entry declares.
///
/// A pack with either issue still LOADS; it just carries a rule that can never fire, which is exactly
/// what a pack author wants told before shipping it.
///
/// ## The regex half is DERIVED, never listed
/// Which fields carry a pattern is answered by exactly one thing in this crate,
/// `dsl::def::pattern_fields::for_each_pattern_field` — the same walk
/// `RulePackDef::expand_fragments` drives — whose arms destructure every matcher struct with no `..`
/// rest pattern, so a field added to a matcher is a COMPILE ERROR there until someone classifies it as a
/// pattern or as not-a-pattern. This function used to keep its own hand-written twin of that list, and
/// the twin did what a hand list always does: it went stale silently. Measured on this tree before the
/// change, four fields the interpreter regex-compiles at eval time were never compile-checked here —
/// `CallScan::algorithm_pattern`, `CallScan::line_pattern`, `CallScan::line_exclude_pattern` and
/// `LineScan::prev_line_exclude_pattern` — so `validate-rule-pack` answered `{"valid":true,"issues":[]}`
/// for a pack whose call-scan carried `"line_exclude_pattern": "(unclosed"` and whose rule
/// (`dsl::call_scan`'s `compile_opt` returning `None`) could never evaluate one site. Deriving removes
/// the second list rather than lengthening it.
///
/// ORDERING PRECONDITION: every caller reaches a pack that has already been through
/// `RulePackDef::expand_fragments` (`parse_dsl_pack` runs it at the text boundary; `zzop-facade`'s
/// `base_engine_config` runs it for inline `packDefs`), so the values walked here are resolved regex
/// text. An unexpanded `${NAME}` reference is NOT a valid regex, so a census running before expansion
/// would report every fragment-using rule as broken — pinned by
/// `pack_regex_issues_sees_expanded_fragments_because_parse_expands_first`.
///
/// Fields that are deliberately NOT regexes need no skip list here either: the attribute gates
/// (`attr_present`/`attr_absent`/`require_attr_declared`), the exact-match call kinds (`kind`,
/// `line_call_kind`, `require_call_kind`), the `patterns[]` labels (`trigger`, `after`) and the numeric
/// policy values (`snippet_max`, `entropy_min`) are excluded by the walker's own `field: _` arms, each
/// with its reason on the line. One place decides, and it is the place a new field must pass through.
pub fn pack_regex_issues(pack: &RulePackDef) -> Vec<String> {
    let mut issues = Vec::new();
    for rule in &pack.rules {
        // PACK-QUALIFIED, always: these strings reach an analysis-wide `warnings` array spanning every
        // loaded pack (`zzop_engine`'s `uncompilable_rule_warnings`), where a bare id is ambiguous — two
        // packs may each carry a `sql-injection`, and two broken ones would emit byte-identical lines,
        // leaving no way to name the offender in `disabledRules`. The single-pack caller
        // (`validate_rule_pack`) loses nothing from the id shape every other rule surface uses.
        let rule_id = format!("{}/{}", pack.id, rule.id);
        // The walker mutates in place (it is the fragment-substitution mechanism) and this function has
        // shared access, so the rule is cloned to be walked. The alternative — a `&`-only twin of the
        // walker — would mean a SECOND exhaustive match over every matcher struct, which is the exact
        // duplication this change exists to delete. The cost is the only reason that trade is acceptable
        // and it is not close: cloning a `RuleDef` is a handful of `String` copies, while the very next
        // thing this loop does to each of those strings is `regex::Regex::new`, a full pattern compile
        // orders of magnitude more expensive. Nothing is written back — the clone dies here.
        let mut walked = rule.clone();
        let _ = for_each_pattern_field::<std::convert::Infallible>(&mut walked, &mut |field, p| {
            if let Err(err) = regex::Regex::new(p) {
                issues.push(format!(
                    "rule \"{rule_id}\": `{field}` is not a valid regex (the rule would silently never fire): {err}"
                ));
            }
            Ok(())
        });
        // STRUCTURAL findings, after every regex line of the same rule so the order stays stable.
        issues.extend(structural_issues(&rule.matcher, &rule_id));
    }
    issues
}

/// The non-regex half: a matcher that compiles perfectly and still cannot ever produce a finding.
/// Reported by the same function because a validator answering `{"valid": true}` for a rule that can
/// never fire is worse than no validator — the author ships believing it was checked.
///
/// Only two matcher kinds have such a shape, and the four that do not are documented below rather than
/// left to silence: `CallScan`, `LiteralScan`, `SymbolScan` and `IoScan` all go dead through a VOCABULARY
/// mismatch (a `kind` no producer emits, a language whose parser projects no literals), which is not
/// decidable from the pack alone and is caught one level out instead.
fn structural_issues(matcher: &Matcher, rule_id: &str) -> Vec<String> {
    match matcher {
        // With neither `line_pattern` nor `any` there is nothing to match, `eval_line_scan` returns
        // immediately, and the rule is as dead as a broken regex makes one.
        Matcher::LineScan(m) if m.line_pattern.is_none() && m.any.is_none() => vec![format!(
            "rule \"{rule_id}\": declares neither `line_pattern` nor `any` — nothing to match, so the rule can never fire"
        )],
        // Same class: `trigger` naming a label no `patterns` entry declares makes `eval_method_scan` bail
        // before it can ever report.
        Matcher::MethodScan(m) if !m.patterns.iter().any(|p| p.label == m.trigger) => {
            let trigger = &m.trigger;
            vec![format!(
                "rule \"{rule_id}\": `trigger` names label {trigger:?}, which no `patterns` entry declares — the rule can never fire"
            )]
        }
        // No structural class for the rest, and deliberately so. A call-scan with no `kind` and no
        // `callee_pattern` still matches every projected site in the selected files — a broad rule rather
        // than a dead one; likewise a literal-scan with no `name_pattern` and no `entropy_min`. The way
        // THOSE matchers go dead is a `kind` no producer emits or a language whose parser projects no
        // `string_literals`, and neither is decidable from the pack alone: a kind is an open vocabulary,
        // so a spelling this build never heard of may still be a family some Mode-B adapter fills. Caught
        // one level out instead — by `RULE_READ_CALL_KINDS` and the contract test binding it to the
        // shipped rules (`rule_contracts/call_kind_readers.rs`), and by the capability matrix's declared
        // table — which is where a build DOES know both halves.
        _ => Vec::new(),
    }
}
