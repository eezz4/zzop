//! The `hash-call` half of the Python call-site producer — `hashlib`'s two constructor shapes and
//! the never-guess line between them. Split out of [`super`] for that file's line cap, along the
//! seam that makes the split honest: this module holds the family's VOCABULARY and the one
//! argument-derived decision the channel permits, and nothing about the walk.
//!
//! [`super`]'s module doc owns the family's contract (which spellings, which exclusions, why a
//! `None` algorithm is still a site).

use ruff_python_ast::Expr;
/// `hashlib`'s per-algorithm constructors — the stdlib's own names, where the FUNCTION name is the
/// algorithm. Scope stated so the blanks read as choices: the digests a shipped rule asks about, weak
/// and strong alike (a family carrying only the weak ones would put the rule's judgment in the
/// producer). `hashlib.pbkdf2_hmac`/`scrypt` are absent — they are KDFs, whose parameters, not their
/// name, decide strength.
const HASHLIB_CONSTRUCTORS: &[&str] = &[
    "md5", "sha1", "sha224", "sha256", "sha384", "sha512", "blake2b", "blake2s",
];

/// The generic constructor whose algorithm is an ARGUMENT — the only Python spelling where the
/// never-guess rule has a dynamic case to refuse (`hashlib.new(name)` → `None`).
const HASHLIB_NEW: &str = "hashlib.new";

/// The `hash-call` family test for one call: `Some(algorithm)` when the callee is a recognized
/// `hashlib` construction, `None` when it is not this family at all. The INNER `Option` is the
/// never-guess axis — `Some(Some("md5"))` for `hashlib.md5()` and for `hashlib.new("md5")`,
/// `Some(None)` for `hashlib.new(name)`, where the site is real but the algorithm is not spelled.
pub(super) fn hash_algorithm(
    callee: &str,
    call: &ruff_python_ast::ExprCall,
) -> Option<Option<String>> {
    if let Some(algo) = callee.strip_prefix("hashlib.") {
        if HASHLIB_CONSTRUCTORS.contains(&algo) {
            return Some(Some(algo.to_string()));
        }
    }
    if callee != HASHLIB_NEW {
        return None;
    }
    // First POSITIONAL argument only, and only a plain string literal — an f-string, a name, or a
    // keyword-only spelling all leave the algorithm unspelled at this site.
    let spelled = call.arguments.args.first().and_then(|a| match a {
        Expr::StringLiteral(s) => Some(s.value.to_str().to_string()),
        _ => None,
    });
    Some(spelled)
}
