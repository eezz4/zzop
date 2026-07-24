//! The dead-rule census: every way a pack's rule can PARSE fine and still never be able to fire.
//!
//! Split out of `pack_loader.rs` (300-line cap) and kept as one function because both defect classes it
//! reports — an uncompilable regex and a structurally empty matcher — have the same consequence and the
//! same remedy. Two callers share it: `validate-rule-pack` (pre-scan, one pack) and the engine's
//! `uncompilable_rule_warnings` (per run, every loaded pack), which is why the ids are pack-qualified.

use crate::dsl::{Matcher, RulePackDef};

/// Every rule in `pack` that can never fire, as one issue string each (deterministic: rule order, then
/// field order within the matcher, with structural lines after the regex lines of the same rule).
///
/// Two defect classes, one list:
/// - a regex-typed field that does not compile — the exact judgment the DSL interpreter applies at eval
///   time (`regex::Regex::new(p)` failing), where the interpreter's contract is to silently no-op the
///   affected rule (see `dsl::line_scan`/`method_scan`/`ir_scan`) rather than panic;
/// - a matcher with nothing to match — a line-scan declaring neither `line_pattern` nor `any`, or a
///   method-scan whose `trigger` names a label no `patterns` entry declares.
///
/// A pack with either issue still LOADS; it just carries a rule that can never fire, which is exactly
/// what a pack author wants told before shipping it.
pub fn pack_regex_issues(pack: &RulePackDef) -> Vec<String> {
    let mut issues = Vec::new();
    for rule in &pack.rules {
        // PACK-QUALIFIED, always: these strings reach an analysis-wide `warnings` array spanning every
        // loaded pack (`zzop_engine`'s `uncompilable_rule_warnings`), where a bare id is ambiguous — two
        // packs may each carry a `sql-injection`, and two broken ones would emit byte-identical lines,
        // leaving no way to name the offender in `disabledRules`. The single-pack caller
        // (`validate_rule_pack`) loses nothing from the id shape every other rule surface uses.
        let rule_id = format!("{}/{}", pack.id, rule.id);
        // Structural findings collect separately: `check` below mutably borrows `issues` for the whole
        // `match`, so a second borrow inside an arm cannot compile.
        let mut structural: Vec<String> = Vec::new();
        let mut check = |field: &str, pattern: &str| {
            if let Err(err) = regex::Regex::new(pattern) {
                issues.push(format!(
                    "rule \"{rule_id}\": `{field}` is not a valid regex (the rule would silently never fire): {err}"
                ));
            }
        };
        match &rule.matcher {
            Matcher::LineScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.require_file {
                    check("require_file", p);
                }
                for p in &m.require_file_all {
                    check("require_file_all", p);
                }
                for p in &m.require_file_absent {
                    check("require_file_absent", p);
                }
                if let Some(p) = &m.line_pattern {
                    check("line_pattern", p);
                }
                for lp in m.any.iter().flatten() {
                    check("any[].pattern", &lp.pattern);
                }
                if let Some(p) = &m.exclude_pattern {
                    check("exclude_pattern", p);
                }
                if let Some(p) = &m.file_exclude_pattern {
                    check("file_exclude_pattern", p);
                }
                // STRUCTURAL, not regex: with neither `line_pattern` nor `any` there is nothing to
                // match, `eval_line_scan` returns immediately, and the rule is as dead as a broken regex
                // makes one. Reported here because this function is what `validate-rule-pack` runs — a
                // validator answering `{"valid": true}` for a rule that can never fire is worse than no
                // validator: the author ships believing it was checked.
                if m.line_pattern.is_none() && m.any.is_none() {
                    structural.push(format!(
                        "rule \"{rule_id}\": declares neither `line_pattern` nor `any` — nothing to match, so the rule can never fire"
                    ));
                }
            }
            Matcher::MethodScan(m) => {
                check("file_pattern", &m.file_pattern);
                // Same structural class as line-scan's above: `trigger` naming a label no `patterns`
                // entry declares makes `eval_method_scan` bail before it can ever report.
                if !m.patterns.iter().any(|p| p.label == m.trigger) {
                    let trigger = &m.trigger;
                    structural.push(format!(
                        "rule \"{rule_id}\": `trigger` names label {trigger:?}, which no `patterns` entry declares — the rule can never fire"
                    ));
                }
                if let Some(p) = &m.require_file {
                    check("require_file", p);
                }
                for p in &m.require_file_all {
                    check("require_file_all", p);
                }
                for p in &m.require_file_absent {
                    check("require_file_absent", p);
                }
                for lp in &m.patterns {
                    check("patterns[].pattern", &lp.pattern);
                }
                for lp in &m.absent {
                    check("absent[].pattern", &lp.pattern);
                }
                if let Some(p) = &m.file_exclude_pattern {
                    check("file_exclude_pattern", p);
                }
            }
            Matcher::SymbolScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.name_pattern {
                    check("name_pattern", p);
                }
            }
            Matcher::IoScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.file_exclude_pattern {
                    check("file_exclude_pattern", p);
                }
                if let Some(p) = &m.key_pattern {
                    check("key_pattern", p);
                }
                if let Some(p) = &m.symbol_pattern {
                    check("symbol_pattern", p);
                }
                if let Some(p) = &m.anchor_exclude_pattern {
                    check("anchor_exclude_pattern", p);
                }
                // `attr_present`/`attr_absent` are plain attribute-key strings, not regexes — never
                // checked here (see `IoScan`'s doc).
            }
        }
        // `check`'s borrow of `issues` ends at its last use above (NLL), so the structural lines can
        // join the same list here — after every regex line for this rule, keeping the order stable.
        issues.extend(structural);
    }
    issues
}
