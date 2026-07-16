//! Finding-shaping for the `"dead-exports"` native analysis — see the parent module doc's
//! "Engine wiring" section.

use std::collections::HashMap;

use zzop_core::{disable_hint, Finding, Severity, SourceSymbolKind};

use super::{DeadExport, DeadExportReason};

/// Converts every `find_dead_exports` result into a `Finding` at its symbol's declaration line.
pub fn dead_export_findings(
    dead: Vec<DeadExport>,
    symbol_lines: &HashMap<(&str, &str), u32>,
) -> Vec<Finding> {
    dead.into_iter()
        .map(|d| dead_export_to_finding(symbol_lines, d))
        .collect()
}

fn dead_export_to_finding(symbol_lines: &HashMap<(&str, &str), u32>, d: DeadExport) -> Finding {
    let line = symbol_lines
        .get(&(d.file.as_str(), d.name.as_str()))
        .copied()
        .unwrap_or(1);
    let message = format!(
        "exported {} '{}' is {} ({}). {} {} if this is public API consumed outside this repo (e.g. \
         published to npm) — such consumers are invisible to this in-repo import graph.",
        kind_label(d.kind),
        d.name,
        match d.reason {
            DeadExportReason::Unused => "never imported anywhere",
            DeadExportReason::InFileOnly => "only referenced within its own file",
        },
        reason_label(d.reason),
        match d.reason {
            DeadExportReason::Unused => "Delete it, or export it from somewhere it's actually consumed.",
            DeadExportReason::InFileOnly => "Drop the `export` keyword to make the un-used-elsewhere status explicit.",
        },
        disable_hint("dead-exports"),
    );
    Finding {
        rule_id: "dead-exports".to_string(),
        severity: Severity::Info,
        file: d.file.clone(),
        line,
        message,
        data: serde_json::to_value(&d).ok(),
    }
}

fn kind_label(kind: SourceSymbolKind) -> &'static str {
    match kind {
        SourceSymbolKind::Function => "function",
        SourceSymbolKind::Class => "class",
        SourceSymbolKind::Const => "const",
        SourceSymbolKind::Type => "type",
        SourceSymbolKind::Interface => "interface",
    }
}

fn reason_label(reason: DeadExportReason) -> &'static str {
    match reason {
        DeadExportReason::Unused => "deletion candidate",
        DeadExportReason::InFileOnly => "un-export candidate",
    }
}
