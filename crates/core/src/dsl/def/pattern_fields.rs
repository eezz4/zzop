//! The ONE enumeration of "which matcher fields carry a pattern" — walked by
//! [`RulePackDef::expand_fragments`](super::RulePackDef::expand_fragments) to resolve `${NAME}` refs, and
//! by the inline-value census (`crate::dsl::tests_inline_census`) to enumerate what it must triage.
//!
//! # Why this is a function and not three lists
//!
//! Three sites used to answer "which fields bear a pattern", by hand, in three places: the four match
//! arms inside `expand_fragments`, `tests_fragments::byte_identity::pattern_bearing_field_values` (whose
//! own doc promised "the EXACT same field set" and had no way to keep that promise), and — as of the
//! census — a third would have been born. A hand list goes stale on the day a field is added, and it goes
//! stale SILENTLY: the new field simply is not walked, so no fragment resolves in it, no census row is
//! emitted for it, and every guard reads green. Measured on this very tree, before this file existed:
//! `IoScan::symbol_pattern` and `IoScan::anchor_exclude_pattern` are both regex fields, both shipped, and
//! NEITHER was in `expand_fragments`'s list nor in `byte_identity`'s twin of it —
//! `rules/dsl/http/http.json`'s `anchor_exclude_pattern` carried a live inline vocabulary
//! (`dev|debug|internal|env|guard|isProduction|isLocal|NODE_ENV`) that no reader of either list could
//! see. Both fields are walked here, which closes that as a side effect of removing the duplication.
//!
//! # What makes it derived rather than listed
//!
//! Every arm below destructures its matcher struct with NO `..` rest pattern. A field added to `LineScan`
//! / `MethodScan` / `SymbolScan` / `IoScan` / `CallScan` therefore fails to COMPILE here until someone
//! writes it down as one of the two things it can be — a pattern (visited) or not (`field: _`, with the
//! reason on the line). The `match` over [`Matcher`] has no wildcard arm for the same reason, one level
//! up: a new matcher kind cannot be added without being given a field list — which is exactly how
//! `CallScan`'s three pattern fields arrived here rather than being noticed later. This is §5.5 of the
//! working agreements (derive, never enumerate) applied to a struct's fields rather than to a directory
//! listing.
//!
//! # What is deliberately NOT a pattern field
//!
//! The attribute gates (`attr_present`, `attr_absent`, `require_attr_declared` on `LineScan` and
//! `CallScan`; `attr_present`, `attr_absent` on `IoScan`) are plain attribute KEYS looked up in the
//! `AttributeStore` — never regex-compiled (`pack_regex_issues` skips them by name, see each field's own
//! doc). Visiting them would make them fragment-expandable, which would mean a `${NAME}` in one silently
//! became an attribute key spelled as a regex. `MethodScan::trigger` and `LabeledPattern::label` are
//! LABELS — they name a `patterns[]` entry, they are compared by equality, and a regex there would match
//! nothing. Everything else on those structs is a bool or a `usize`.

use super::matcher::{
    CallScan, IoScan, LabeledPattern, LineScan, LiteralScan, MethodScan, SymbolScan,
};
use super::{Matcher, RuleDef};

/// The callback [`for_each_pattern_field`] hands each pattern field to: its FIELD NAME (spelled the way
/// the JSON pack spells it, with `[]` for the repeated shapes) and a mutable handle on the value.
///
/// `&'static str` for the name is load-bearing for the census: a field name is a fixed spelling, so a
/// caller can key a committed snapshot on it without allocating or worrying about lifetime.
type PatternVisit<'a, E> = &'a mut dyn FnMut(&'static str, &mut String) -> Result<(), E>;

fn opt<E>(
    field: &'static str,
    value: &mut Option<String>,
    visit: PatternVisit<'_, E>,
) -> Result<(), E> {
    match value {
        Some(v) => visit(field, v),
        None => Ok(()),
    }
}

fn each<E>(
    field: &'static str,
    values: &mut [String],
    visit: PatternVisit<'_, E>,
) -> Result<(), E> {
    for v in values.iter_mut() {
        visit(field, v)?;
    }
    Ok(())
}

fn labeled<E>(
    field: &'static str,
    patterns: &mut [LabeledPattern],
    visit: PatternVisit<'_, E>,
) -> Result<(), E> {
    for lp in patterns.iter_mut() {
        visit(field, &mut lp.pattern)?;
    }
    Ok(())
}

/// Visits every pattern-bearing field of `rule`'s matcher, in a fixed (declaration) order, short-circuiting
/// on the first `Err` the callback returns. See this module's header for why the field set is expressed as
/// exhaustive destructuring rather than as a list.
pub(crate) fn for_each_pattern_field<E>(
    rule: &mut RuleDef,
    visit: PatternVisit<'_, E>,
) -> Result<(), E> {
    match &mut rule.matcher {
        Matcher::LineScan(LineScan {
            file_pattern,
            require_file,
            require_file_all,
            require_file_absent,
            line_pattern,
            any,
            exclude_pattern,
            prev_line_exclude_pattern,
            file_exclude_pattern,
            // Not patterns — see this module's header. Named individually rather than swallowed by `..`
            // so that adding a field to `LineScan` is a compile error here, not a silent omission.
            skip_comment_lines: _,
            strip_string_literals: _,
            // A call KIND, compared by equality against `CallSite::kind` — never a regex.
            line_call_kind: _,
            attr_present: _,
            attr_absent: _,
            require_attr_declared: _,
            snippet_max: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("require_file", require_file, visit)?;
            each("require_file_all", require_file_all, visit)?;
            each("require_file_absent", require_file_absent, visit)?;
            opt("line_pattern", line_pattern, visit)?;
            if let Some(any) = any.as_mut() {
                labeled("any[].pattern", any, visit)?;
            }
            opt("exclude_pattern", exclude_pattern, visit)?;
            opt(
                "prev_line_exclude_pattern",
                prev_line_exclude_pattern,
                visit,
            )?;
            opt("file_exclude_pattern", file_exclude_pattern, visit)?;
        }
        Matcher::MethodScan(MethodScan {
            file_pattern,
            require_file,
            require_file_all,
            require_file_absent,
            patterns,
            absent,
            file_exclude_pattern,
            // `trigger` and `after` name a `patterns[].label` (equality, never a regex); the rest are
            // bools/usize. See this module's header.
            skip_comment_lines: _,
            strip_string_literals: _,
            trigger: _,
            trigger_in_loop: _,
            after: _,
            after_in_same_function: _,
            // A call KIND, compared by equality against `CallSite::kind` — never a regex.
            require_call_kind: _,
            snippet_max: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("require_file", require_file, visit)?;
            each("require_file_all", require_file_all, visit)?;
            each("require_file_absent", require_file_absent, visit)?;
            labeled("patterns[].pattern", patterns, visit)?;
            labeled("absent[].pattern", absent, visit)?;
            opt("file_exclude_pattern", file_exclude_pattern, visit)?;
        }
        Matcher::SymbolScan(SymbolScan {
            file_pattern,
            name_pattern,
            // `kind` is a `SourceSymbolKind` enum, `exported`/`negate` are bools.
            kind: _,
            exported: _,
            negate: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("name_pattern", name_pattern, visit)?;
        }
        Matcher::IoScan(IoScan {
            file_pattern,
            file_exclude_pattern,
            key_pattern,
            symbol_pattern,
            anchor_exclude_pattern,
            // `direction`/`kind` are enums, `negate` is a bool, the two `attr_*` are attribute keys.
            direction: _,
            kind: _,
            negate: _,
            attr_absent: _,
            attr_present: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("file_exclude_pattern", file_exclude_pattern, visit)?;
            opt("key_pattern", key_pattern, visit)?;
            opt("symbol_pattern", symbol_pattern, visit)?;
            opt("anchor_exclude_pattern", anchor_exclude_pattern, visit)?;
        }
        Matcher::CallScan(CallScan {
            file_pattern,
            file_exclude_pattern,
            callee_pattern,
            algorithm_pattern,
            line_pattern,
            // `kind` is an EXACT-match call-kind string (compared with `==`, never compiled), the three
            // `attr_*` are attribute keys, `in_loop` is a bool and `snippet_max` a usize. See this
            // module's header for why a `${NAME}` must not reach any of them.
            kind: _,
            in_loop: _,
            attr_present: _,
            attr_absent: _,
            require_attr_declared: _,
            snippet_max: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("file_exclude_pattern", file_exclude_pattern, visit)?;
            opt("callee_pattern", callee_pattern, visit)?;
            opt("algorithm_pattern", algorithm_pattern, visit)?;
            opt("line_pattern", line_pattern, visit)?;
        }
        Matcher::LiteralScan(LiteralScan {
            file_pattern,
            file_exclude_pattern,
            name_pattern,
            name_exclude_pattern,
            // `entropy_min` is an f32 POLICY VALUE, not a pattern, so this walk (and with it the
            // inline-value census, which only sees pattern fields) cannot carry it. Its census home is
            // the Rust side instead: `zzop_core::HIGH_ENTROPY_SECRET_MIN_BITS` (scripts/
            // policy-census.txt), bound to the shipped pack value by
            // `crates/engine/tests/rule_contracts/literal_scan_threshold.rs` so the two spellings
            // cannot drift. `skip_value_equals_name` is a bool.
            entropy_min: _,
            skip_value_equals_name: _,
        }) => {
            visit("file_pattern", file_pattern)?;
            opt("file_exclude_pattern", file_exclude_pattern, visit)?;
            opt("name_pattern", name_pattern, visit)?;
            opt("name_exclude_pattern", name_exclude_pattern, visit)?;
        }
    }
    Ok(())
}
