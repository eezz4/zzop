//! `ir.rs`'s "Body span contract" section opens by claiming it is the SINGLE OWNER of the span
//! semantics — "no parser module doc restates it — they link here". This holds that claim for the
//! documents, because it has already been false twice.
//!
//! ## The two failures this exists for
//! On 2026-08-11 commit `1bf9a9e` corrected the two documents a 1.0 review had named
//! (`docs/NORMALIZED_AST.md`, `docs/adapters/envelope.schema.json`), both of which had endorsed a
//! lexical brace-matcher as a span source — the reading the contract explicitly rejects. It missed two
//! more sites carrying the SAME defect one file over:
//!
//! - `docs/modules/facade.md` said the pair is "set only for functions/classes with a recoverable body
//!   span" — which gets both halves backwards. It framed omission as a MEASUREMENT FAILURE when the
//!   contract makes it a positive claim, and it named a `kind` whitelist the contract does not have.
//! - `docs/rules/dsl-reference.md` offered "or a parser that couldn't project one" as an alternative
//!   reading of an absent span — the same could-not-compute framing, and this one is **baked into the
//!   shipped binary** by `crates/summary/src/contracts.rs`, so a rule author reads it offline.
//!
//! Two independent fixes of one claim, each missing sites the other did not touch, is the signature of
//! a fact with no owner. The owner exists; what was missing was anything that noticed a second copy.
//!
//! ## What this checks, and why it is a link check rather than a content check
//! Every markdown document under `docs/` that names the span fields must POINT AT the owner. It does
//! not check what the document says about them — that would be this test asserting on prose it cannot
//! read, and would go red on every honest rewording.
//!
//! The link is the load-bearing part anyway: all four drifted sites were written by someone who did
//! not know a contract section existed, and a document carrying the pointer is one whose next editor
//! is told where the truth lives before they restate it. A document that mentions the fields WITHOUT
//! the pointer is, by construction, a second copy with no path back to the first.
//!
//! ## Scope: documents only
//! Deliberately not extended to `parser/**/*.rs`, where 25 files name `body_start`. Nearly all of them
//! merely ASSIGN the field, and requiring a contract link on every assignment would produce pasted
//! links that carry no information and train people to paste them — the opposite of the goal. The
//! defect class measured here is a PROSE RESTATEMENT drifting from the contract, and prose lives in
//! `docs/`.

use std::fs;
use std::path::{Path, PathBuf};

/// The wire and Rust spellings both, since a document may use either.
const SPAN_FIELDS: [&str; 4] = ["bodyStart", "bodyEnd", "body_start", "body_end"];

/// Either form of the pointer. The section title is what a reader searches for; the path is what a
/// reader clicks. Accepting both keeps the guard from dictating link style.
const OWNER_REFS: [&str; 2] = ["Body span contract", "core/src/ir.rs"];

fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs")
        .canonicalize()
        .expect("docs/ must exist relative to crates/core")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn every_doc_naming_the_span_fields_points_at_the_contract_that_owns_them() {
    let docs = docs_dir();
    let mut files = Vec::new();
    markdown_files(&docs, &mut files);
    files.sort();

    let mut mentioning = 0usize;
    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
        if !SPAN_FIELDS.iter().any(|f| text.contains(f)) {
            continue;
        }
        mentioning += 1;
        if !OWNER_REFS.iter().any(|r| text.contains(r)) {
            offenders.push(
                file.strip_prefix(&docs)
                    .unwrap_or(file)
                    .display()
                    .to_string(),
            );
        }
    }

    // Non-vacuity: this test is a no-op the moment nothing under docs/ names the fields, which is
    // exactly what a rename would cause. Three documents name them today.
    assert!(
        mentioning >= 3,
        "only {mentioning} document(s) under docs/ name any of {SPAN_FIELDS:?}, down from 3. Either \
         the fields were renamed — in which case update SPAN_FIELDS, because this guard is now \
         watching a name nothing uses — or documents were removed and the coverage this test claims \
         no longer exists"
    );

    assert!(
        offenders.is_empty(),
        "these docs/ document(s) name the body-span fields without pointing at the contract that \
         owns them ({OWNER_REFS:?}): {offenders:?}. Every such document is a second copy of a \
         single-owner contract, and both times that happened the copy taught the OPPOSITE of the \
         contract (a brace-matcher span source; an absent span as a measurement failure rather than \
         a positive claim). Add a pointer to `crates/core/src/ir.rs`'s \"Body span contract\" section \
         rather than restating what it says"
    );
}
