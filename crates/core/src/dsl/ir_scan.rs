//! IR-query matcher evaluation — `Matcher::SymbolScan` (per-file declaration queries) and
//! `Matcher::IoScan` (whole-tree IO-fact queries).
//!
//! `IoScan` evaluates WHOLE-TREE, not per-file: since the 2026 projection redesign it can no longer run
//! inside the per-file parse pass (`Matcher::IoScan`'s arm in `eval.rs` is a no-op — see that file), because
//! it needs facts the per-file pass cannot see: assemble-composed `IoProvide`s (router-mount/controller-
//! prefix/file-convention composition, Java/C# whole-corpus passes) and the tree-wide `AttributeStore`,
//! which only exists after assemble. The engine calls [`eval_pack_io_scan`] once, after assemble, with an
//! [`IoScanTreeContext`] built from the assembled `provides`/`consumes`/`AttributeStore` — see that
//! function's doc for the exact filter order.

use crate::attributes::AttributeStore;
use crate::finding::Finding;
use crate::io::{IoConsume, IoProvide};

use super::def::{IoDirection, IoScan, Matcher, RuleDef, RulePackDef, SymbolScan};
use super::diagnostics::RuleDiag;
use super::markers::{
    compile_marker_line_comment, marker_suppresses, message_with_near_miss, Leaders,
};
use super::source::RuleContext;

pub(super) fn eval_symbol_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &SymbolScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
    // Rule-skip sink — see `diagnostics`'s module doc.
    diagnostics: &mut Vec<String>,
) {
    let rule_id = format!("{}/{}", pack_id, rule.id);
    let mut diag = RuleDiag::new(&rule_id, diagnostics);
    let Some(file_re) = diag.compile("file_pattern", &m.file_pattern) else {
        return;
    };
    let Some(name_re) = diag.compile_opt("name_pattern", m.name_pattern.as_ref()) else {
        return;
    };

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        for sym in &f.symbols {
            if let Some(k) = &m.kind {
                if sym.kind != *k {
                    continue;
                }
            }
            if let Some(exported) = m.exported {
                if sym.exported != exported {
                    continue;
                }
            }
            // `name_pattern`'s role flips under `negate` — see `SymbolScan`'s doc comment.
            let name_matches = name_re.as_ref().map(|re| re.is_match(&sym.name));
            let keep = match (m.negate, name_matches) {
                (false, None) => true,
                (false, Some(matched)) => matched,
                (true, None) => true,
                (true, Some(matched)) => !matched,
            };
            if !keep {
                continue;
            }
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line: sym.line,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": sym.name })),
            });
        }
    }
}

/// Whole-tree input for [`eval_pack_io_scan`]: every provide/consume the assembled tree carries, the
/// assembled `AttributeStore`, and a source-text lookup for the anchor-line channel
/// (`IoScan::anchor_exclude_pattern` and suppress-marker recognition).
pub struct IoScanTreeContext<'a> {
    pub provides: &'a [IoProvide],
    pub consumes: &'a [IoConsume],
    pub attrs: &'a AttributeStore,
    /// `(repo-relative file, 1-based line) -> that line's text`, when source text is reachable (native
    /// mode). Returns `None` when it is not (e.g. envelope mode has no native source) — every consumer of
    /// this callback (`anchor_exclude_pattern`, suppress-marker recognition) treats `None` as "nothing to
    /// check", never as a match or a miss.
    pub anchor_line: &'a dyn Fn(&str, u32) -> Option<String>,
}

/// Evaluates every `Matcher::IoScan` rule in `pack` against the whole tree's IO facts (`ctx.provides` /
/// `ctx.consumes`), appending findings to `out`. Non-`IoScan` rules are skipped — this function is the
/// io-scan-only counterpart of `eval_pack`; a caller running a mixed pack calls both.
///
/// For each `IoScan` rule, every entry selected by `direction` (`provides` then `consumes`, each in INPUT
/// ORDER — the determinism contract) passes through these gates, cheap-first, in order:
///
/// 1. `file_pattern` on the entry's `file`.
/// 2. `file_exclude_pattern` on the same `file` — an entry whose file matches this is skipped entirely,
///    evaluated right after `file_pattern` since it's the same cheap string-only cost class (no attribute
///    lookup, no anchor-text fetch).
/// 3. `kind` exact match.
/// 4. `key_pattern` + `negate` — ported verbatim from the pre-redesign per-file evaluator: an entry with
///    `key: None` never matches `key_pattern`, so under `negate: true` that absence itself is a hit.
/// 5. `symbol_pattern` — a provides-only gate (a consume's `symbol` is always `None` here, so it never
///    passes when this is set); `negate` does NOT apply to this field.
/// 6. `attr_present` / `attr_absent` — `AttributeStore::route_attr(entry.kind, key, attr_key)` truthiness;
///    an entry with no resolved key has nothing to look up (fails `attr_present`, satisfies `attr_absent`).
/// 7. `anchor_exclude_pattern` — regex against `ctx.anchor_line(entry.file, entry.line)`; a `None` callback
///    result means the exclusion does not apply.
/// 8. Suppress-marker: `rule.suppress_marker()` (derived `<id>-ok`), checked via `ctx.anchor_line` against
///    the anchor line's own text and the line directly above it (the one-line lookback `line_scan`/
///    `method_scan` share). A marker-SHAPED token there that is NOT this rule's marker suppresses nothing
///    and is disclosed in the emitted message — see `markers::message_with_near_miss`.
pub fn eval_pack_io_scan(pack: &RulePackDef, ctx: &IoScanTreeContext, out: &mut Vec<Finding>) {
    eval_pack_io_scan_into(pack, ctx, out, &mut Vec::new());
}

/// [`eval_pack_io_scan`] with the rule-skip diagnostics channel attached: every io-scan rule this pass
/// SKIPS because one of its regexes does not compile appends one line to `diagnostics` (see the
/// `diagnostics` module doc for the wording and the dedupe contract). Findings are identical to
/// `eval_pack_io_scan`'s — the sink only makes an already-happening skip visible. `eval_pack_io_scan`
/// itself drops the messages, which is exactly the gap a caller with a warnings channel closes by
/// calling this instead.
pub fn eval_pack_io_scan_into(
    pack: &RulePackDef,
    ctx: &IoScanTreeContext,
    out: &mut Vec<Finding>,
    diagnostics: &mut Vec<String>,
) {
    for rule in &pack.rules {
        let Matcher::IoScan(m) = &rule.matcher else {
            continue;
        };
        eval_io_scan_rule(&pack.id, rule, m, ctx, out, diagnostics);
    }
}

/// One resolved IO entry, flattened from either an `IoProvide` or an `IoConsume` so the filter chain below
/// is direction-agnostic. `symbol` is always `None` for a consume — see `IoScan::symbol_pattern`'s doc.
struct IoEntry<'a> {
    file: &'a str,
    kind: &'a str,
    key: Option<&'a str>,
    line: u32,
    symbol: Option<&'a str>,
}

fn eval_io_scan_rule(
    pack_id: &str,
    rule: &RuleDef,
    m: &IoScan,
    ctx: &IoScanTreeContext,
    out: &mut Vec<Finding>,
    // Rule-skip sink — see `diagnostics`'s module doc. Same contract as every other DSL matcher: a rule
    // whose regex does not compile is skipped, not fatal, and never silent.
    diagnostics: &mut Vec<String>,
) {
    let rule_id = format!("{}/{}", pack_id, rule.id);
    let mut diag = RuleDiag::new(&rule_id, diagnostics);
    let Some(file_re) = diag.compile("file_pattern", &m.file_pattern) else {
        return;
    };
    let Some(file_exclude_re) =
        diag.compile_opt("file_exclude_pattern", m.file_exclude_pattern.as_ref())
    else {
        return;
    };
    let Some(key_re) = diag.compile_opt("key_pattern", m.key_pattern.as_ref()) else {
        return;
    };
    let Some(symbol_re) = diag.compile_opt("symbol_pattern", m.symbol_pattern.as_ref()) else {
        return;
    };
    let Some(anchor_exclude_re) =
        diag.compile_opt("anchor_exclude_pattern", m.anchor_exclude_pattern.as_ref())
    else {
        return;
    };
    // Line-comment-NEUTRAL marker (`//` or `#`) — io-scan anchor lines span every provide-producing
    // language, Python included, unlike the `//`-only per-file line/method-scan marker. Built from the
    // rule id (escaped), so a failure is structural rather than an author's bad pattern.
    let marker = rule.suppress_marker();
    let Some(marker_re) = compile_marker_line_comment(&marker) else {
        diag.malformed("its derived suppress marker does not compile as a regex");
        return;
    };

    // Determinism contract: provides then consumes, each in input order.
    let mut entries: Vec<IoEntry> = Vec::new();
    if matches!(m.direction, IoDirection::Provides | IoDirection::Any) {
        entries.extend(ctx.provides.iter().map(|p| IoEntry {
            file: p.file.as_str(),
            kind: p.kind.as_str(),
            key: Some(p.key.as_str()),
            line: p.line,
            symbol: p.symbol.as_deref(),
        }));
    }
    if matches!(m.direction, IoDirection::Consumes | IoDirection::Any) {
        entries.extend(ctx.consumes.iter().map(|c| IoEntry {
            file: c.file.as_str(),
            kind: c.kind.as_str(),
            key: c.key.as_deref(),
            line: c.line,
            symbol: None, // a consume never carries a symbol — see `IoScan::symbol_pattern`'s doc.
        }));
    }

    for e in entries {
        if !file_re.is_match(e.file) {
            continue;
        }
        if let Some(re) = &file_exclude_re {
            if re.is_match(e.file) {
                continue;
            }
        }
        if let Some(k) = &m.kind {
            if e.kind != k.as_str() {
                continue;
            }
        }
        // `key_pattern`'s role flips under `negate` — ported verbatim from the pre-redesign evaluator; a
        // key-less entry never matches.
        let matches_pattern = match (&key_re, e.key) {
            (Some(re), Some(k)) => re.is_match(k),
            (Some(_), None) => false,
            (None, _) => true,
        };
        let keep = if m.negate {
            !matches_pattern
        } else {
            matches_pattern
        };
        if !keep {
            continue;
        }
        // `symbol_pattern` — plain conjunctive gate, `negate` does not apply to it (see `IoScan`'s doc).
        if let Some(re) = &symbol_re {
            if !e.symbol.is_some_and(|s| re.is_match(s)) {
                continue;
            }
        }
        // `attr_present` / `attr_absent` — vocab-free `AttributeStore` lookup keyed on the entry's own
        // `(kind, key)`. An entry with no resolved key has nothing to look up: it never satisfies
        // `attr_present`, and always satisfies `attr_absent`.
        if let Some(attr_key) = &m.attr_present {
            let truthy = e
                .key
                .and_then(|k| ctx.attrs.route_attr(e.kind, k, attr_key))
                .is_some_and(crate::attributes::attr_is_truthy);
            if !truthy {
                continue;
            }
        }
        if let Some(attr_key) = &m.attr_absent {
            let truthy = e
                .key
                .and_then(|k| ctx.attrs.route_attr(e.kind, k, attr_key))
                .is_some_and(crate::attributes::attr_is_truthy);
            if truthy {
                continue;
            }
        }
        // `anchor_exclude_pattern` — a `None` callback result means the exclusion does not apply (see
        // `IoScan::anchor_exclude_pattern`'s doc). Fetched unconditionally: the suppress-marker channel
        // below is DERIVED from the rule id, so every io-scan rule always has an anchor-text feature in
        // play (the old `anchor_exclude_re.is_some() || marker_re.is_some()` gate could never be false).
        let anchor_text = (ctx.anchor_line)(e.file, e.line);
        if let Some(re) = &anchor_exclude_re {
            if anchor_text.as_deref().is_some_and(|t| re.is_match(t)) {
                continue;
            }
        }
        // Suppress-marker: same one-line lookback as `line_scan`/`method_scan` (`MARKER_LOOKBACK_LINES`),
        // applied to the anchor line's own text and the line directly above it. A line whose text is
        // unreachable (`anchor_line` returns `None`) contributes an empty string — never a match, so an
        // absent line simply never suppresses, same "honestly absent" treatment as `anchor_exclude_pattern`.
        let above_text = (ctx.anchor_line)(e.file, e.line.saturating_sub(1)).unwrap_or_default();
        let current_text = anchor_text.unwrap_or_default();
        let window = [above_text.as_str(), current_text.as_str()];
        if marker_suppresses(&marker_re, &window, 1) {
            continue;
        }
        let message =
            message_with_near_miss(Leaders::SlashOrHash, &marker, &window, 1, &rule.message);
        let snippet = e.key.unwrap_or("<unresolved>").to_string();
        out.push(Finding {
            rule_id: rule_id.clone(),
            severity: rule.severity,
            file: e.file.to_string(),
            line: e.line,
            message,
            data: Some(serde_json::json!({ "snippet": snippet, "kind": e.kind })),
        });
    }
}
