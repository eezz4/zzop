//! Inline-handler idempotency-key-read judgment for `router_mounts`. Feeds
//! `build::FragmentBuilder`'s Verb-arm classification: when a route's inline handler reads the
//! idempotency-key header, mints `IDEMPOTENCY_GUARDED_ATTR_KEY` onto that Verb entry's
//! `attr_keys` — see the parent module doc for the recognizer spec this feeds into.

use swc_core::ecma::ast::{Expr, Str, Tpl};
use swc_core::ecma::visit::{Visit, VisitWith};

/// The attribute key emitted when an inline handler is judged to read the idempotency-key
/// header. Producer<->consumer contract vocabulary — pairs with `zzop_rules_cross_layer`'s public
/// `IDEMPOTENCY_GUARDED_ATTR` (`"idempotency-guarded"`); an engine e2e pins the pairing, same
/// convention as `guard.rs`'s `AUTH_GUARDED_ATTR_KEY` <-> rules-http's `AUTH_GUARDED_ATTR`.
pub(super) const IDEMPOTENCY_GUARDED_ATTR_KEY: &str = "idempotency-guarded";

/// The idempotency-key header names this recognizer accepts as a witness, already lowercased.
const IDEMPOTENCY_HEADER_NAMES: &[&str] = &["idempotency-key", "x-idempotency-key"];

/// Judges whether an inline handler expression's body reads the idempotency-key header.
///
/// Only `Expr::Arrow`/`Expr::Fn` (an INLINE handler expression) are ever judged — a
/// named-identifier handler (defined elsewhere, resolved only by name) deliberately returns
/// `false`. v1 is inline-only, never-guess: a cross-file handler's idempotency behavior is
/// covered by attribute INJECTION (a Mode B overlay), not by native recognition here.
///
/// Implementation: a small `Visit` walk over the handler expression looking for a string literal
/// (or a no-substitution, single-quasi template literal) whose value, lowercased, equals
/// `"idempotency-key"` or `"x-idempotency-key"` — the literal is the recognized witness, found
/// anywhere in the handler body, including inside nested closures/functions declared within it (a
/// helper closure reading the header is still this handler's own behavior — only the OUTER
/// expression must be the inline `Arrow`/`Fn`).
///
/// Why literal-scan, not call-shape-scan (documented deliberately, not an oversight): the read
/// idiom varies per framework/style — `req.get('Idempotency-Key')`, `req.headers['idempotency-key']`,
/// Hono's `c.req.header('Idempotency-Key')`, `X-Idempotency-Key` casing variants, and more — but
/// every one of them must name the header as a string (or zero-interpolation template) literal
/// somewhere in the handler body. That literal is the one stable witness across all these call
/// shapes. Scanning is scoped to the handler body ONLY, never file-wide, so a header literal read
/// by some unrelated handler elsewhere in the same file can never leak this tag onto this route.
pub(super) fn inline_handler_reads_idempotency_key(handler: &Expr) -> bool {
    let mut finder = IdempotencyKeyLiteralFinder { found: false };
    match handler {
        Expr::Arrow(arrow) => arrow.visit_with(&mut finder),
        Expr::Fn(f) => f.visit_with(&mut finder),
        _ => return false,
    }
    finder.found
}

/// Walks an inline handler's body, setting `found` once a string/template literal matches
/// [`IDEMPOTENCY_HEADER_NAMES`]. No early-abort on `found` — a `Visit` walk has no cheap way to
/// short-circuit, and a single handler body is small enough that the extra visits after the first
/// match cost nothing worth guarding against.
struct IdempotencyKeyLiteralFinder {
    found: bool,
}

impl Visit for IdempotencyKeyLiteralFinder {
    fn visit_str(&mut self, n: &Str) {
        if is_idempotency_header_literal(n.value.as_str().unwrap_or_default()) {
            self.found = true;
        }
    }

    fn visit_tpl(&mut self, n: &Tpl) {
        // Only a no-substitution, single-quasi template counts (`` `idempotency-key` ``) — same
        // "cooked, zero-interpolation" discipline as the rest of this parser's static-string
        // literal reads (see e.g. `asset_refs::static_str_arg`).
        if n.exprs.is_empty() && n.quasis.len() == 1 {
            if let Some(text) = n.quasis[0].cooked.as_ref().and_then(|c| c.as_str()) {
                if is_idempotency_header_literal(text) {
                    self.found = true;
                }
            }
        }
        n.visit_children_with(self);
    }
}

fn is_idempotency_header_literal(value: &str) -> bool {
    IDEMPOTENCY_HEADER_NAMES.contains(&value.to_ascii_lowercase().as_str())
}
