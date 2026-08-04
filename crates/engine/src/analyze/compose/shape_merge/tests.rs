//! Coverage for `ShapeMerge::poisoned_disclosures`' wording: the file-count phrase must match the
//! actual file set — a SAME-FILE conflict (TS allows two `interface A` declarations in one file,
//! reachable since the declaration-merge poisoning covers within-file pairs too) says "in <file>",
//! never the self-contradictory "across 1 files".
use std::collections::HashSet;

use zzop_core::{ClassShapeFragment, ProvideBodyField};

use super::ShapeMerge;

fn frag(name: &str, field_names: &[&str]) -> ClassShapeFragment {
    ClassShapeFragment {
        name: name.to_string(),
        fields: field_names
            .iter()
            .map(|n| ProvideBodyField {
                name: n.to_string(),
                optional: false,
            })
            .collect(),
        complete: true,
    }
}

#[test]
fn same_file_conflict_is_disclosed_as_in_the_file_not_across_1_files() {
    let merge = ShapeMerge::build(&[(
        "a.ts".to_string(),
        vec![frag("A", &["id"]), frag("A", &["id", "email"])],
    )]);
    let referenced: HashSet<&str> = ["A"].into();
    let out = merge.poisoned_disclosures(&referenced, "declared-response");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("in a.ts"), "{}", out[0]);
    assert!(!out[0].contains("across 1 files"), "{}", out[0]);
}

#[test]
fn cross_file_conflict_keeps_the_across_n_files_wording() {
    let merge = ShapeMerge::build(&[
        ("a.ts".to_string(), vec![frag("A", &["id"])]),
        ("b.ts".to_string(), vec![frag("A", &["id", "email"])]),
    ]);
    let referenced: HashSet<&str> = ["A"].into();
    let out = merge.poisoned_disclosures(&referenced, "request-body");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("across 2 files (a.ts, b.ts)"), "{}", out[0]);
}
