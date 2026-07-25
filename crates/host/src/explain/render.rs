//! The human-readable rendering half of `zzop explain <rule-id>` — everything downstream of a
//! successful lookup: the stable-order field block, the per-matcher-kind suppress-marker answer, and the
//! per-matcher-kind exclusion reporting. Split out of `explain.rs` purely to keep that file under the
//! repo's source-file line cap; the lookup lanes and their failure messages stay there.

use zzop_core::{Matcher, RuleDef, RulePackDef, Severity};

/// The stable-order, human-readable rendering: full id, pack, severity, message, the DERIVED suppress
/// marker (`RuleDef::suppress_marker()` — never stored, see its own doc) with the comment leaders that
/// matcher kind actually honors, matcher kind, and then one line per exclusion field THAT matcher kind
/// actually carries, with its real value (see [`exclusion_lines`]).
pub(super) fn render(pack: &RulePackDef, rule: &RuleDef) -> String {
    let mut lines = vec![
        format!("id: {}/{}", pack.id, rule.id),
        format!("pack: {}", pack.id),
        format!("severity: {}", severity_str(rule.severity)),
        format!("message: {}", rule.message),
        format!("suppress marker: {}", suppress_marker_str(rule)),
        format!("matcher: {}", matcher_kind(&rule.matcher)),
    ];
    lines.extend(exclusion_lines(&rule.matcher));
    lines.join("\n")
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// One of the four matcher shapes `docs/rules/dsl-reference.md` documents — the serde tag names
/// themselves (`Matcher`'s `#[serde(tag = "type", rename_all = "kebab-case")]`), so this can never drift
/// from what a pack's own `"type"` field spells.
fn matcher_kind(matcher: &Matcher) -> &'static str {
    match matcher {
        Matcher::LineScan(_) => "line-scan",
        Matcher::MethodScan(_) => "method-scan",
        Matcher::SymbolScan(_) => "symbol-scan",
        Matcher::IoScan(_) => "io-scan",
    }
}

/// The derived marker plus the comment leaders that matcher kind actually HONORS — the leader set is
/// per-pass, not universal (`zzop_core::dsl::markers::Leaders`): the whole-tree io-scan pass compiles its
/// marker with `compile_marker_line_comment` (`//` or `#`, since an `http` provide's anchor line can be
/// Python), while the per-file line/method-scan passes compile `//` only, widened to `--` inside a `.sql`
/// file. Printing the bare token would let a Python reader of an io-scan rule and a Python reader of a
/// line-scan rule draw the same conclusion from two different truths.
///
/// `symbol-scan` is the third answer: its findings have no source line to anchor a comment against, so no
/// marker can ever suppress them (`docs/rules/dsl-reference.md`'s "Suppress-marker semantics"). Printing
/// `<id>-ok` there would hand the reader a comment that silently does nothing. Latent today — no bundled
/// pack uses `symbol-scan` — which is exactly why it is worth answering honestly before the first one ships.
///
/// The io-scan answer carries a further RUN-MODE condition, and printing the marker without it is the same
/// silent failure in slower motion. An io-scan marker is read off the anchor line's source TEXT
/// (`zzop_core::dsl::ir_scan`'s one-line lookback), and envelope mode has none: `envelope::ingest` supplies
/// a constant-`None` `anchor_line` callback, since a Normalized-AST envelope carries no source to re-read.
/// So every io-scan marker is inert in `analyze-envelope` runs — a reader who copies the marker this
/// command prints into an envelope-fed pipeline gets silence and concludes the rule stopped firing. The
/// condition rides the same derived-from-the-rule path as the marker itself (matched on THIS rule's own
/// matcher kind), so no list of which rules are io-scan is kept here.
fn suppress_marker_str(rule: &RuleDef) -> String {
    match rule.matcher {
        Matcher::SymbolScan(_) => {
            "none (symbol-scan findings have no line to anchor a marker)".to_string()
        }
        Matcher::IoScan(_) => format!(
            "{} (in a `//` or `#` line comment, on the finding's own line or the one directly above it) \
             — NATIVE-PARSE RUNS ONLY: in envelope mode (`analyze-envelope`) this finding's anchor line \
             has no source text, so no marker there can suppress anything; disable the rule id or exclude \
             the path instead",
            rule.suppress_marker()
        ),
        _ => format!(
            "{} (in a `//` line comment — also `--` inside a .sql file — on the finding's own line or \
             the one directly above it)",
            rule.suppress_marker()
        ),
    }
}

/// One line per exclusion/veto field the rule's OWN matcher kind actually carries, each with its REAL
/// value — never a blanket `exclude_pattern: no` across kinds that have no such field but do have others.
/// The kinds genuinely differ (`zzop_core::dsl::def::matcher`): `LineScan` has `exclude_pattern` +
/// `file_exclude_pattern`; `MethodScan` has `absent` (its veto) + `file_exclude_pattern`; `IoScan` has
/// `file_exclude_pattern` + `anchor_exclude_pattern`; `SymbolScan` has none at all and says so, rather than
/// printing a `no` that reads as "this kind could carry one and this rule declines to".
///
/// Why this is worth the lines: the old single boolean answered `no` for every `IoScan` and `MethodScan`
/// rule, including ones that DO exclude (`http/route-exposure` carries both an `anchor_exclude_pattern` and
/// a `file_exclude_pattern`; `sql/nplus1` carries a `file_exclude_pattern`). A reader believing the tool
/// then reports the excluded case as a false negative, or builds a redundant carve-out for an exclusion
/// that already exists.
fn exclusion_lines(matcher: &Matcher) -> Vec<String> {
    match matcher {
        // The three attribute gates are listed alongside the regex exclusions because they exclude the
        // same way a regex does — and one of them excludes HARDER than any regex can: with
        // `require_attr_declared` set and nothing declaring that key, the rule does not run at all. A
        // reader who cannot see that field reads the resulting silence as a false negative, which is
        // precisely the misreading this whole section exists to prevent.
        Matcher::LineScan(m) => vec![
            optional_pattern_line("exclude_pattern", m.exclude_pattern.as_deref()),
            optional_pattern_line("file_exclude_pattern", m.file_exclude_pattern.as_deref()),
            optional_pattern_line("attr_present", m.attr_present.as_deref()),
            optional_pattern_line("attr_absent", m.attr_absent.as_deref()),
            optional_pattern_line("require_attr_declared", m.require_attr_declared.as_deref()),
        ],
        Matcher::MethodScan(m) => {
            let absent = if m.absent.is_empty() {
                "no".to_string()
            } else {
                m.absent
                    .iter()
                    .map(|p| format!("{}={}", p.label, p.pattern))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            vec![
                format!("absent: {absent}"),
                optional_pattern_line("file_exclude_pattern", m.file_exclude_pattern.as_deref()),
            ]
        }
        Matcher::SymbolScan(_) => {
            vec!["exclusions: none (symbol-scan carries no exclusion field)".to_string()]
        }
        Matcher::IoScan(m) => vec![
            optional_pattern_line("file_exclude_pattern", m.file_exclude_pattern.as_deref()),
            optional_pattern_line(
                "anchor_exclude_pattern",
                m.anchor_exclude_pattern.as_deref(),
            ),
            optional_pattern_line("attr_present", m.attr_present.as_deref()),
            optional_pattern_line("attr_absent", m.attr_absent.as_deref()),
        ],
    }
}

/// `<field>: <the regex this rule set>` or `<field>: no` when it set none — the field name is printed
/// either way, so the reader learns which exclusions this matcher kind even offers.
fn optional_pattern_line(field: &str, value: Option<&str>) -> String {
    format!("{field}: {}", value.unwrap_or("no"))
}
