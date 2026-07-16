//! `scan_unsafe_read_endpoint` + `scan_non_idempotent_write` — the two native whole-graph rules in this
//! crate that need call-graph BFS. `apiChurn` (needs git-history joins) and `feBeSpecDrift` (cross-service
//! type drift) are out of scope: both need capabilities beyond a single-repo call graph.
//!
//! Both scanners resolve a method-gated `ApiEndpoint`'s handler to a symbol id, then BFS downstream over
//! the whole-repo `SymbolGraph` (`zzop_core::callgraph::bfs_reachable`) until a symbol carrying a
//! qualifying write site is found (lowest depth wins; ties break by symbol id ascending). Write-site
//! detection itself is NOT done here: it is a structural attribute computed once at TS parse time
//! (`zzop_parser_typescript::write_sites_for_symbol`, feeding `SourceSymbol::write_sites`) rather than a
//! regex re-scan of each BFS-reached symbol's raw text on every analysis run — see that function's module
//! doc for the detection rules (vocabulary, SQL-vs-ORM precedence, the `unsafe-read-endpoint`-specific
//! counter-site exclusion) and their two narrowing consequences, both unchanged by the move: a nested
//! function's body is included in its outer symbol's scanned span, so a write inside it attributes to the
//! outer symbol; and a raw-SQL label truncates at the first newline, so a multi-line statement's label can
//! be incomplete.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use zzop_core::SourceSymbol;

mod non_idempotent;
mod unsafe_read;

pub use non_idempotent::{scan_non_idempotent_write, ScanNonIdempotentWriteInput};
pub use unsafe_read::{scan_unsafe_read_endpoint, ScanUnsafeReadEndpointInput};

/// Lines above a handler's body start to look back for an `idempotent-ok` marker (shared by both scanners).
const OK_MARKER_LOOKBACK_LINES: u32 = 4;

const SAFE_METHODS: [&str; 2] = ["GET", "HEAD"];
/// The crate's single write-verb vocabulary (T1): `mutating_route_no_auth` imports this same
/// symbol — same meaning ("HTTP methods that mutate"), so no per-rule copy (policy census).
pub(crate) const WRITE_HTTP_METHODS: [&str; 4] = ["PUT", "DELETE", "POST", "PATCH"];

fn ok_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"//\s*idempotent-ok:").unwrap())
}

// --- Shared helpers (name index / handler resolution / whitelist) ---

fn ident_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z_$][\w$]*").unwrap())
}

/// Tail name (after the last `.`) -> symbol ids (`"file#name"`). `pub(crate)`: also used by `mutating_route_no_auth`.
pub(crate) fn build_name_index(symbols: &[SourceSymbol]) -> HashMap<String, Vec<String>> {
    let mut idx: HashMap<String, Vec<String>> = HashMap::new();
    for s in symbols {
        let tail = s.name.rsplit('.').next().unwrap_or(&s.name).to_string();
        idx.entry(tail).or_default().push(s.id.clone());
    }
    idx
}

/// Resolves a handler reference string to a unique symbol id, stripping wrapper calls (`rateLimit(fn)`) and
/// member access (`ctrl.list`). `None` when unknown or ambiguous (defined in multiple files) — never guessed.
pub(crate) fn resolve_handler(handler: &str, idx: &HashMap<String, Vec<String>>) -> Option<String> {
    let ids: Vec<&str> = ident_re().find_iter(handler).map(|m| m.as_str()).collect();
    for ident in ids.iter().rev() {
        match idx.get(*ident) {
            Some(candidates) if candidates.len() == 1 => return Some(candidates[0].clone()),
            Some(_) => return None, // ambiguous — do not guess
            None => continue,
        }
    }
    None
}

/// A `// idempotent-ok: <reason>` comment within `OK_MARKER_LOOKBACK_LINES` lines above the handler's body suppresses the finding (also covers the declaration's own line, an off-by-one side effect).
fn is_whitelisted(
    handler_symbol: &str,
    symbols: &[SourceSymbol],
    files: &HashMap<String, String>,
) -> bool {
    let Some(sym) = symbols.iter().find(|s| s.id == handler_symbol) else {
        return false;
    };
    let Some(text) = files.get(&sym.file) else {
        return false;
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let decl_line = sym.body_start.unwrap_or(sym.line);
    let start = decl_line.saturating_sub(OK_MARKER_LOOKBACK_LINES);
    let mut i = start;
    while i < decl_line {
        if let Some(l) = lines.get(i as usize) {
            if ok_marker_re().is_match(l) {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests;
