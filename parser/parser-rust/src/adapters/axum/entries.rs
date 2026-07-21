//! `RouterMountEntry::Verb` construction for the axum adapter — the verb-expansion + dedup + entry-build
//! cluster, extracted from `axum.rs` (file-size limit). Deals in the adapter OUTPUT type
//! (`RouterMountEntry`), unlike the pure syn-expression helpers in `util.rs`.

use zzop_core::RouterMountEntry;

/// Push one `Verb` entry per method for `method`, expanding the `any(...)` sentinel "ANY" to every
/// `HTTP_KEY_VERBS` verb (a catch-all binds one handler to all methods) and passing any concrete verb
/// through unchanged. Handler/line are shared across the expansion.
///
/// Deduped by (method, path): when a concrete verb co-occurs with a catch-all on the same path
/// (`.route("/x", get(h).any(h2))` → GET from both the concrete call and the `any` expansion), only the
/// FIRST push for a given verb survives — so the concrete handler is preserved and `duplicate-route`
/// doesn't see a phantom second GET /x from one `.route()` registration.
pub(super) fn push_verb(
    out: &mut Vec<RouterMountEntry>,
    method: String,
    path: &str,
    handler: Option<String>,
    line: u32,
) {
    if method == "ANY" {
        for v in zzop_core::HTTP_KEY_VERBS {
            push_unique(out, v.to_string(), path, handler.clone(), line);
        }
    } else {
        push_unique(out, method, path, handler, line);
    }
}

/// Push a `Verb` entry only if `out` has no entry with the same (method, path) yet.
fn push_unique(
    out: &mut Vec<RouterMountEntry>,
    method: String,
    path: &str,
    handler: Option<String>,
    line: u32,
) {
    let dup = out.iter().any(|e| {
        matches!(e, RouterMountEntry::Verb { method: m, path: p, .. } if *m == method && p == path)
    });
    if !dup {
        out.push(verb_entry(method, path, handler, line));
    }
}

fn verb_entry(method: String, path: &str, handler: Option<String>, line: u32) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method,
        path: path.to_string(),
        handler,
        line,
        attr_keys: Vec::new(),
    }
}
