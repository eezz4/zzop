//! The one PROSE claim in this schema that a key/type/nullability diff cannot reach:
//! `sourceSymbol.id` is NOT UNIQUE.
//!
//! ## Why a test rather than a reviewer
//! Every other dimension this suite seals is structural — a key exists or it does not, a value
//! deserializes or it does not. Non-uniqueness is neither: the id is a plain `String` on both sides,
//! so a schema that promised uniqueness and a schema that denied it are byte-identical to every check
//! in `key_parity`/`required_nullable`/`enum_parity`. The claim lives only in the two descriptions,
//! and descriptions are what drift.
//!
//! They already had drifted. `docs/adapters/envelope.schema.json` gained the NOT-UNIQUE sentence on
//! 2026-08-11 while `crates/core/src/ir.rs`'s own `id` doc still said nothing but "file + name
//! combination id" — so the document an EXTERNAL adapter author reads and the document every INTERNAL
//! producer reads taught opposite things about the same field, and 1.0 was about to freeze both.
//!
//! ## Why it matters more than a wording nit
//! The collision is real and measured: 12 groups over 28 symbols on the reference corpus (Java/C#
//! overloads, TypeScript overload signatures, TS declaration merging). A consumer that reads "file +
//! name combination id" and keys a map by it drops every colliding sibling, and iteration order then
//! decides which survives — which is exactly how an unannotated Java overload silenced
//! `mutating-route-no-auth`, and how TS declaration ORDER flipped a security verdict (2026-08-11).
//! The engine now has ONE convention for that choice (`zzop_rules_http::http_scan::symbols_by_id`),
//! and `ir.rs`'s doc is where a new consumer is told to use it.
//!
//! ## Method, and its one honest limit
//! Both subjects are read as TEXT — the schema because the claim is inside a `description` string,
//! and `ir.rs` because a doc comment does not survive to runtime. So this pins that the SENTENCE is
//! present on both sides, not that the sentence is true; the truth is pinned elsewhere, by
//! `symbol_index`'s own tests. That is the right split: a text test that tried to verify semantics
//! would be asserting on prose it also wrote.

use std::fs;
use std::path::PathBuf;

use super::{load_schema, schema_path};

/// The claim, spelled exactly once here and required on BOTH sides. Deliberately the token both
/// documents actually carry rather than a looser regex: a guard whose needle is vaguer than the claim
/// passes on a document that says something weaker.
const CLAIM: &str = "NOT UNIQUE";

fn ir_rs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/ir.rs")
}

/// The `id` field's own doc block in `ir.rs` — from the `pub struct SourceSymbol` line down to the
/// `pub id: String;` declaration. Scoped rather than searched file-wide so the claim cannot be
/// satisfied by an unrelated mention of the same words elsewhere in a 400-line contract doc.
fn source_symbol_id_doc(text: &str) -> &str {
    let struct_at = text
        .find("pub struct SourceSymbol {")
        .expect("ir.rs no longer declares `pub struct SourceSymbol` — this test's subject moved");
    let rest = &text[struct_at..];
    let id_at = rest
        .find("pub id: String,")
        .expect("SourceSymbol no longer has a `pub id: String,` field — this test's subject moved");
    &rest[..id_at]
}

#[test]
fn the_schema_and_ir_rs_both_declare_the_symbol_id_non_unique() {
    let schema = load_schema();
    let described = schema
        .get("definitions")
        .and_then(|d| d.get("sourceSymbol"))
        .and_then(|d| d.get("properties"))
        .and_then(|p| p.get("id"))
        .and_then(|id| id.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or_else(|| {
            panic!(
                "{} has no definitions.sourceSymbol.properties.id.description — the subject moved, \
                 and an absent subject would make this test vacuously green",
                schema_path().display()
            )
        });
    assert!(
        described.contains(CLAIM),
        "{} describes sourceSymbol.id without the {CLAIM} claim. An external adapter author reads \
         ONLY this file, and the id really does collide (12 groups / 28 symbols on the reference \
         corpus). Description was: {described}",
        schema_path().display()
    );

    let ir = fs::read_to_string(ir_rs_path())
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", ir_rs_path().display()));
    let doc = source_symbol_id_doc(&ir);
    assert!(
        doc.contains(CLAIM),
        "crates/core/src/ir.rs documents SourceSymbol::id without the {CLAIM} claim, while \
         {} carries it. Every INTERNAL producer and consumer reads ir.rs and nothing else, so this \
         is the half that actually changes behaviour. Doc block was: {doc}",
        schema_path().display()
    );
}

/// The companion half: `ir.rs` must NAME the one convention a consumer should use when it has to pick
/// a winner. Without it the doc states a hazard and leaves the reader to invent a second rule — which
/// is the defect the 2026-08-11 fix removed (two consumers had resolved the same collision in
/// opposite directions).
#[test]
fn ir_rs_points_a_colliding_consumer_at_the_single_resolution_convention() {
    let ir = fs::read_to_string(ir_rs_path())
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", ir_rs_path().display()));
    let doc = source_symbol_id_doc(&ir);
    assert!(
        doc.contains("symbols_by_id"),
        "SourceSymbol::id's doc warns that the id collides but names no resolution convention. \
         `zzop_rules_http::http_scan::symbols_by_id` is the one the engine uses (prefer the entry \
         carrying write sites, else first); a doc that omits it invites a second one. Doc block \
         was: {doc}"
    );
}
