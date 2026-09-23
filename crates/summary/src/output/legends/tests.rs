//! Unit seals for the fold's DERIVATION. The wire behaviour — nothing duplicated, nothing unreachable
//! — is `crates/summary/tests/legend_fold.rs`'s to prove over real replies; what belongs here is the
//! property that makes that test's population complete: the reply and the document read ONE list.

use super::*;

/// The list is the whole mechanism, so it must not be empty and every entry must be usable from BOTH
/// ends. A `folded()` that lost an entry would make the integration test's derived needle set smaller
/// and green at the same time — the failure mode a derived population is supposed to remove.
#[test]
fn every_entry_yields_a_short_note_and_a_full_text_both_ways() {
    let list = folded();
    assert!(!list.is_empty(), "the fold list is empty");
    for legend in &list {
        let note = (legend.note)();
        let full: usize = (legend.full)().iter().map(|(_, b)| b.len()).sum();
        assert!(
            note.len() > 200,
            "`{}`'s note is {} bytes — a pointer with no reading attached is not what this fold ships",
            legend.key,
            note.len()
        );
        assert!(
            full > note.len(),
            "`{}` folds to {} bytes from a full text of {full} — a fold that does not shrink is a \
             second copy",
            legend.key,
            note.len()
        );
    }
}

/// Both pointer shapes name the SAME document, and both name it through the constant rather than a
/// spelling of their own. A folded key that pointed at a name the contract table does not carry is the
/// one failure worse than not folding at all.
#[test]
fn both_pointer_shapes_name_the_contract_constant_and_no_other() {
    let uri = format!("{URI_PREFIX}{REPLY_LEGENDS_CONTRACT_NAME}");
    let command = format!("zzop contract {REPLY_LEGENDS_CONTRACT_NAME}");

    // 🔴 THE SEAL on the one COPY this module keeps. `INLINE_POINTER` spells both names literally
    // because contract 16's twin resolution reads the document name out of the SOURCE text and an
    // interpolated one resolves to nothing. That copy is only safe while this holds: rename the
    // document and this reds in the same commit, instead of shipping a sentence pointing at a name the
    // contract table no longer answers.
    let string_form = folded_string("coverageGaps.meaning");
    assert!(
        string_form.contains(&uri) && string_form.contains(&command),
        "a string-valued legend has no sibling slot, so both dialects must be inside the sentence — \
         and both must be the spelling the contract table answers to: {string_form}"
    );

    let object_form = folded_object("packsLoadedMeaning");
    assert_eq!(object_form["resource"], json!(uri));
    assert_eq!(object_form["command"], json!(command));
    assert!(
        object_form["note"].as_str().is_some_and(|n| n.len() > 200),
        "the object form must still say what its counts are about: {object_form}"
    );
}

/// The rendered document carries one heading per folded key, spelled as the REPLY spells it. The
/// pointer tells a reader to look "under this key's own heading", so a heading that did not match the
/// key would send them somewhere that is not there.
#[test]
fn the_document_heads_every_section_with_the_reply_key_it_explains() {
    let doc = contract_text();
    for legend in folded() {
        assert!(
            doc.contains(&format!("## `{}`", legend.key)),
            "the document has no heading for `{}`, which the reply's own pointer tells a reader to \
             find",
            legend.key
        );
    }
    // CANARY: the needle must be able to fail. A key that is NOT folded must not have a heading —
    // otherwise "contains a heading" is a statement about markdown, not about this list.
    assert!(
        !doc.contains("## `architecture.painMeaning`"),
        "`painMeaning` is deliberately NOT folded (output principle §1.5) — a heading for it here \
         means the document is being written by hand rather than derived from the fold list"
    );
}
