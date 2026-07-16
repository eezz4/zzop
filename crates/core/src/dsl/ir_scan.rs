//! IR-query matcher evaluation — `Matcher::SymbolScan` (declaration queries) and `Matcher::IoScan`
//! (per-file IO-fact queries).

use crate::finding::Finding;

use super::def::{IoDirection, IoScan, RuleDef, SymbolScan};
use super::source::RuleContext;

pub(super) fn eval_symbol_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &SymbolScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    let name_re = match &m.name_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

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

pub(super) fn eval_io_scan(
    pack_id: &str,
    rule: &RuleDef,
    m: &IoScan,
    ctx: &RuleContext,
    out: &mut Vec<Finding>,
) {
    // Skip the rule if a regex fails to compile (TODO: report via diagnostics).
    let Ok(file_re) = regex::Regex::new(&m.file_pattern) else {
        return;
    };
    let key_re = match &m.key_pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(_) => return,
        },
        None => None,
    };
    let rule_id = format!("{}/{}", pack_id, rule.id);

    for f in ctx.files {
        if !file_re.is_match(&f.rel) {
            continue;
        }
        let Some(io) = &f.io else {
            continue; // no IO projection for this file (see SourceFile::io doc) -> nothing to scan
        };
        // One flattened list of (kind, key, line) regardless of provide/consume — both line fields are
        // mandatory `u32`s today, so a "fall back to line 1" case is unreachable and not coded.
        let mut entries: Vec<(&str, Option<&str>, u32)> = Vec::new();
        if matches!(m.direction, IoDirection::Provides | IoDirection::Any) {
            entries.extend(
                io.provides
                    .iter()
                    .map(|p| (p.kind.as_str(), Some(p.key.as_str()), p.line)),
            );
        }
        if matches!(m.direction, IoDirection::Consumes | IoDirection::Any) {
            entries.extend(
                io.consumes
                    .iter()
                    .map(|c| (c.kind.as_str(), c.key.as_deref(), c.line)),
            );
        }
        for (kind, key, line) in entries {
            if let Some(k) = &m.kind {
                if kind != k.as_str() {
                    continue;
                }
            }
            // `key_pattern`'s role flips under `negate` — see `IoScan`'s doc; a key-less entry never matches.
            let matches_pattern = match (&key_re, key) {
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
            let snippet = key.unwrap_or("<unresolved>").to_string();
            out.push(Finding {
                rule_id: rule_id.clone(),
                severity: rule.severity,
                file: f.rel.clone(),
                line,
                message: rule.message.clone(),
                data: Some(serde_json::json!({ "snippet": snippet, "kind": kind })),
            });
        }
    }
}
