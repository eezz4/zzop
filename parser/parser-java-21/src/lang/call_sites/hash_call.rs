//! The `hash-call` half of the Java call-site producer — the JCA digest factory's receiver test and
//! the one argument read the channel permits. Split out of [`super`] for that file's line cap,
//! along the seam that keeps the split honest: everything here is a pure question about one node,
//! with no walk state and no site construction.
//!
//! [`super`]'s module doc owns the family's contract (which spellings, which exclusions, and why a
//! `None` algorithm is still a site).

use tree_sitter::Node;

use crate::util::{node_text, string_literal_text, valid_named_children};
/// The JCA digest factory whose calls are `hash-call` sites. A BARE `MessageDigest` identifier at the
/// site, the same receiver line every other arm here draws — a fully qualified
/// `java.security.MessageDigest.getInstance(...)` is a field-access chain rather than this shape and
/// is deliberately silent, which is worth stating because the JDK tutorial spells it both ways.
pub(super) const MESSAGE_DIGEST_TYPE: &str = "MessageDigest";

/// Does this receiver name the JCA digest factory — the bare identifier `MessageDigest`, or a dotted
/// field-access chain whose LAST segment is it (`java.security.MessageDigest`)?
///
/// The qualified spelling is admitted here and NOT for the console/exec families, and the asymmetry is
/// a property of the spellings rather than a loosening: nobody writes `java.lang.System.out.println`,
/// while `java.security.MessageDigest.getInstance(...)` is the JDK's own tutorial form and appears in
/// real code. The tail test is the same one the C# producer applies to its namespace-qualified
/// factories, and it costs no precision — a user type named `MessageDigest` is caught either way, an
/// accepted syntactic imprecision this module's own note already states.
pub(super) fn names_message_digest(node: Node, src: &str) -> bool {
    match node.kind() {
        "identifier" => node_text(node, src) == MESSAGE_DIGEST_TYPE,
        "field_access" | "scoped_identifier" => node
            .child_by_field_name("field")
            .or_else(|| node.child_by_field_name("name"))
            .is_some_and(|f| node_text(f, src) == MESSAGE_DIGEST_TYPE),
        _ => false,
    }
}

/// This call's first argument when it is a plain `"…"` string literal, decoded of its delimiters —
/// the ONE argument-derived fact this producer reads, and only for
/// `MessageDigest.getInstance(...)`. `None` for a variable, a constant reference, a concatenation, a
/// text block, or no argument at all: never-guess, so a rule filtering on `algorithm` goes silent
/// there rather than approximating.
pub(super) fn first_string_argument(call: Node, src: &str) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let first = valid_named_children(args).into_iter().next()?;
    string_literal_text(first, src)
}
