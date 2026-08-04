//! The POSITIVE half of `zzop explain <rule-id>`: what the rule actually scans. Sibling of
//! [`super::render`]'s exclusion half, split into its own file for the repo's per-file line cap.
//!
//! ## Why this exists
//!
//! Until this module, `explain` answered only what a rule EXCLUDES (`file_exclude_pattern`, `absent`,
//! the attribute gates). The question a reader actually arrives with — "would this rule look at my
//! file?" — had no shipped surface at all: the `file_pattern` regexes were hand-copied into rule
//! messages, `docs/rules/catalog.md` and `site/rules.html`, and those nine copies were deleted on
//! 2026-08-01 as drift-prone. Deleting the copies is only correct if the tool answers instead, and
//! this module is that answer.
//!
//! ## Which fields are printed, and why that set
//!
//! ALL of them. Every `pub` field of every matcher struct (`zzop_core::dsl::def::matcher`) is
//! reported by one of the three blocks this command prints — scope (here), exclusions
//! ([`super::render::exclusion_lines`]), presentation ([`presentation_lines`]) — and
//! `every_matcher_field_is_reachable_from_explain` (`super::tests`) reads the field list straight out
//! of the struct SOURCE and fails when a newly added field is answered by none of them. A per-field
//! judgement call ("this one is implementation detail") was considered and rejected: the reader cannot
//! see which judgement was made, so an omitted field is indistinguishable from a field that does not
//! exist, and that is the same wondering-whether-it-exists defect the deleted copies left behind.
//!
//! What differs per field is WHICH block it lands in, and that is a real distinction rather than a
//! cosmetic one: a scope field can make the rule ignore your file, an exclusion field can veto a match
//! it already made, and the one presentation field ([`presentation_lines`], `snippet_max`) can do
//! neither — it truncates the echoed snippet and is labelled as such in the output, so a reader is
//! never left inferring that it might gate something.
//!
//! Two shapes are deliberately NOT reported as bare field values, because the bare value would be read
//! wrong:
//! - `negate` flips what its sibling pattern MEANS (`key_pattern` for io-scan, `name_pattern` for
//!   symbol-scan). A bare `negate: yes` next to a printed regex reads as "matches this", which is the
//!   opposite of the truth, so the line names the field it inverts — including the case the struct docs
//!   call out, where `negate` with no pattern to negate lets every entry through.
//! - An absent FILTER (`kind`, `exported`, `direction`'s `any`) prints `any`, not the `no` that absent
//!   PATTERNS print. `kind: no` would read as "this rule matches no kind", i.e. as a rule that can
//!   never fire, when an unset `kind` means the exact opposite: every kind qualifies.

use zzop_core::{IoDirection, LabeledPattern, Matcher, SourceSymbolKind};

use super::render::optional_pattern_line;

/// One line per field of the rule's OWN matcher kind that decides WHAT gets scanned — the file set
/// first (`file_pattern` and the `require_file*` pre-skips, which are not merely a performance
/// optimisation: a file failing one is never read by this rule at all), then what is looked for inside
/// it, then the per-line text transforms that decide which text the patterns even see.
///
/// Field names are the JSON ones a pack author writes, so a reader can go straight from this output to
/// the pack file or to `docs/rules/dsl-reference.md` without a translation step.
pub(super) fn scope_lines(matcher: &Matcher) -> Vec<String> {
    match matcher {
        Matcher::LineScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            optional_pattern_line("require_file", m.require_file.as_deref()),
            list_line("require_file_all", &m.require_file_all),
            optional_pattern_line("line_pattern", m.line_pattern.as_deref()),
            labeled_line("any", m.any.as_deref().unwrap_or(&[])),
            // A structural REQUIREMENT on what counts as a hit (the line must also carry a projected
            // call site of this kind), not a veto over hits already found — the same placement
            // `in_loop` gets on call-scan and for the reason `exclusion_lines`' call-scan arm states.
            optional_pattern_line("line_call_kind", m.line_call_kind.as_deref()),
            flag_line("skip_comment_lines", m.skip_comment_lines),
            flag_line("strip_string_literals", m.strip_string_literals),
        ],
        Matcher::MethodScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            optional_pattern_line("require_file", m.require_file.as_deref()),
            list_line("require_file_all", &m.require_file_all),
            labeled_line("patterns", &m.patterns),
            format!("trigger: {}", m.trigger),
            flag_line("trigger_in_loop", m.trigger_in_loop),
            // Same placement argument as `trigger_in_loop`'s: a positive structural requirement on
            // the SPAN (it must contain a projected call site of this kind), not a veto.
            optional_pattern_line("require_call_kind", m.require_call_kind.as_deref()),
            optional_pattern_line("after", m.after.as_deref()),
            flag_line("after_in_same_function", m.after_in_same_function),
            flag_line("skip_comment_lines", m.skip_comment_lines),
            flag_line("strip_string_literals", m.strip_string_literals),
        ],
        Matcher::SymbolScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            format!("kind: {}", symbol_kind_str(m.kind)),
            optional_pattern_line("name_pattern", m.name_pattern.as_deref()),
            format!("exported: {}", tristate_str(m.exported)),
            negate_line(m.negate, "name_pattern", m.name_pattern.is_some()),
        ],
        Matcher::IoScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            format!("direction: {}", direction_str(&m.direction)),
            format!(
                "kind: {}",
                m.kind.clone().unwrap_or_else(|| "any".to_string())
            ),
            optional_pattern_line("key_pattern", m.key_pattern.as_deref()),
            negate_line(m.negate, "key_pattern", m.key_pattern.is_some()),
            optional_pattern_line("symbol_pattern", m.symbol_pattern.as_deref()),
        ],
        Matcher::CallScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            // `any` rather than `no` for an unset `kind`, per this module's header: an absent FILTER
            // widens, it does not close.
            format!(
                "kind: {}",
                m.kind.clone().unwrap_or_else(|| "any".to_string())
            ),
            optional_pattern_line("callee_pattern", m.callee_pattern.as_deref()),
            // Both W4 fields are positive REQUIREMENTS on what counts as a hit (an algorithm the
            // site spells; a lexical residual on the site's own line), so they sit with the scope
            // fields rather than with the exclusions — the placement argument `in_loop` makes.
            optional_pattern_line("algorithm_pattern", m.algorithm_pattern.as_deref()),
            optional_pattern_line("line_pattern", m.line_pattern.as_deref()),
            flag_line("in_loop", m.in_loop),
        ],
        Matcher::LiteralScan(m) => vec![
            format!("file_pattern: {}", m.file_pattern),
            optional_pattern_line("name_pattern", m.name_pattern.as_deref()),
            // `no` for an absent floor: an unset `entropy_min` widens (every projected literal
            // matches), the same absent-filter reading the call-scan `kind` line gives.
            match m.entropy_min {
                Some(min) => format!("entropy_min: {min} (total Shannon bits over the value)"),
                None => "entropy_min: no (no entropy floor — every projected literal matches)"
                    .to_string(),
            },
        ],
    }
}

/// The one field that is neither scope nor exclusion, printed last and labelled with what it cannot do.
/// `snippet_max` truncates the finding's echoed source line; it can never change whether a finding
/// exists or which files were read. Printing it unlabelled next to fields that DO gate would invite the
/// reader to price it as a gate; dropping it silently would leave them wondering whether the rule has
/// one. Line-scan, method-scan and call-scan carry the field — symbol-scan and io-scan findings have no
/// snippet, and literal-scan deliberately has neither field nor snippet (the echoed line would carry
/// the candidate secret) — so the block is empty for those three rather than reporting a field that
/// does not exist.
pub(super) fn presentation_lines(matcher: &Matcher) -> Vec<String> {
    let snippet_max = match matcher {
        Matcher::LineScan(m) => m.snippet_max,
        Matcher::MethodScan(m) => m.snippet_max,
        // call-scan DOES echo a snippet (the site's own source line), so it joins the reporters.
        Matcher::CallScan(m) => m.snippet_max,
        Matcher::LiteralScan(_) | Matcher::SymbolScan(_) | Matcher::IoScan(_) => return Vec::new(),
    };
    vec![format!(
        "snippet_max: {snippet_max} (snippet truncation only — never decides whether this rule fires \
         or which files it reads)"
    )]
}

/// `<field>: <a>, <b>` or `<field>: no` when the rule set none — the same always-name-the-field
/// convention [`optional_pattern_line`] uses, for the list-shaped fields.
fn list_line(field: &str, values: &[String]) -> String {
    if values.is_empty() {
        return format!("{field}: no");
    }
    format!("{field}: {}", values.join(", "))
}

/// `<field>: <label>=<pattern>, ...` — the same rendering `exclusion_lines` already uses for
/// method-scan's `absent` veto, so a reader meets one spelling for labelled-pattern lists rather than
/// two.
fn labeled_line(field: &str, patterns: &[LabeledPattern]) -> String {
    if patterns.is_empty() {
        return format!("{field}: no");
    }
    format!(
        "{field}: {}",
        patterns
            .iter()
            .map(|p| format!("{}={}", p.label, p.pattern))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// `<field>: yes` / `<field>: no` for the boolean gates. Printed even when `false`, same as the regex
/// fields: the reader learns which switches this matcher kind offers.
fn flag_line(field: &str, value: bool) -> String {
    format!("{field}: {}", if value { "yes" } else { "no" })
}

/// `negate`, never as a bare boolean — see this module's header for why. It names the sibling field it
/// inverts, and states the struct-documented degenerate case (`negate` with nothing to negate against
/// lets every candidate through, which a bare `yes` would read as the exact opposite of).
fn negate_line(negate: bool, inverts: &str, pattern_set: bool) -> String {
    if !negate {
        return "negate: no".to_string();
    }
    if pattern_set {
        return format!("negate: yes (fires when {inverts} does NOT match)");
    }
    format!("negate: yes (no {inverts} to invert, so every candidate passes this filter)")
}

/// An unset symbol `kind` means every kind qualifies — see this module's header for why that prints
/// `any` rather than the `no` an unset PATTERN prints.
fn symbol_kind_str(kind: Option<SourceSymbolKind>) -> &'static str {
    match kind {
        None => "any",
        Some(SourceSymbolKind::Function) => "function",
        Some(SourceSymbolKind::Class) => "class",
        Some(SourceSymbolKind::Const) => "const",
        Some(SourceSymbolKind::Type) => "type",
        Some(SourceSymbolKind::Interface) => "interface",
    }
}

/// `exported`'s three states, with the unset one spelled `any` for the same reason as
/// [`symbol_kind_str`]: it restricts nothing.
fn tristate_str(value: Option<bool>) -> &'static str {
    match value {
        None => "any",
        Some(true) => "yes",
        Some(false) => "no",
    }
}

/// The serde tag names themselves (`IoDirection`'s `rename_all = "lowercase"`), so this cannot drift
/// from what a pack's own `direction` key spells.
fn direction_str(direction: &IoDirection) -> &'static str {
    match direction {
        IoDirection::Provides => "provides",
        IoDirection::Consumes => "consumes",
        IoDirection::Any => "any",
    }
}
