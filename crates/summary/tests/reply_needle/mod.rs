//! NEEDLE ENCODING for the "this prose is not on the wire" pins — the one owner of the rule that a
//! prose needle must be in the encoding its haystack is in.
//!
//! # Why this is a shared module and not a copy per test file
//! Two integration tests assert the same negative — that a fold removed a paragraph from every reply
//! (`legend_fold.rs` for the four run-invariant legends, `disclosure_fold.rs` for the blindness
//! registry). Both compare a body read out of a served contract document against a SERIALIZED reply,
//! and both are wrong in exactly the same way if they compare raw prose: a JSON string carries `\"`
//! where the document carries `"`, so a needle that quotes anything can never be found and the
//! assertion passes for a reason that has nothing to do with the fold.
//!
//! That failure was MEASURED on 2026-09-02, and it is the reason this lives in one file rather than
//! two. `disclosure_fold.rs`'s pin compared raw prose and was green — but only because its needle is
//! the LAST class body in registry order and that one class happens not to quote. Reordering the
//! registry so a quoting class (`config-error`, four `"`) sorts last left the pin green while the whole
//! registry rode the reply. Nothing about the fold had changed; the needle had.
//!
//! A test directory (no `main.rs`) rather than a `tests/*.rs` file on purpose: cargo builds every
//! `tests/*.rs` as its own test binary, so a flat `reply_needle.rs` would become an empty test target.
//!
//! Both callers use every item here; nothing in this module is allowed to sit unused, because a shared
//! test module is exactly where dead helpers accumulate unnoticed.

/// A needle in the ENCODING the haystack is in. The reply is a JSON document, so its strings carry
/// `\"` where the source prose carries `"` (and `\n`, `\\`, `\t` for the other three a legend body can
/// hold). Comparing raw prose against a serialized reply therefore reports ABSENT for text that is
/// right there — a test that passes for the wrong reason.
///
/// Returns the body of the JSON encoding, without the delimiting quotes, which is what
/// `reply.contains(...)` needs: the reply's own quotes belong to the key/value framing around it.
pub fn as_json_body(text: &str) -> String {
    let encoded = serde_json::to_string(text).expect("a string encodes");
    encoded[1..encoded.len() - 1].to_string()
}
