//! Covers seam ranking (self-contained folder scores above an entangled one), the `min_files` floor,
//! noise-folder (test/build) exclusion, external/test-dir import filtering, cohesion for a fully
//! self-contained folder, and cross-folder co-change demoting a statically-clean folder.
use super::*;
use crate::coupling::CouplingEntry;

fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

#[test]
fn ranks_self_contained_folder_above_entangled_one() {
    // billing: 3 internal edges, 1 boundary out -> score 3/2 = 1.5
    // shared:  1 internal edge, 4 boundary (everyone imports it) -> low score
    let d = dep(&[
        ("billing/a.ts", &["billing/b.ts", "billing/c.ts"]),
        ("billing/b.ts", &["billing/c.ts"]),
        ("billing/c.ts", &["shared/util.ts"]),
        ("shared/util.ts", &["shared/base.ts"]),
        ("shared/base.ts", &[]),
        ("ui/x.ts", &["shared/util.ts"]),
        ("ui/y.ts", &["shared/util.ts", "ui/x.ts"]),
    ]);
    let seams = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT);
    assert_eq!(seams[0].folder, "billing"); // best seam first
    let billing = seams.iter().find(|s| s.folder == "billing").unwrap();
    assert_eq!(billing.internal_edges, 3);
    assert_eq!(billing.outbound, 1);
    assert_eq!(billing.inbound, 0);
}

#[test]
fn skips_folders_below_min_files() {
    let d = dep(&[("a/x.ts", &["a/y.ts"]), ("a/y.ts", &[]), ("b/z.ts", &[])]);
    let seams = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT);
    let folders: Vec<&str> = seams.iter().map(|s| s.folder.as_str()).collect();
    assert_eq!(folders, vec!["a"]); // b has 1 file
}

#[test]
fn test_and_build_folders_are_excluded() {
    let d = dep(&[
        ("tests/a.ts", &["tests/b.ts"]),
        ("tests/b.ts", &["tests/c.ts"]),
        ("tests/c.ts", &[]),
        ("playwright/x.ts", &["playwright/y.ts"]),
        ("playwright/y.ts", &[]),
        ("lib/m.ts", &["lib/n.ts"]),
        ("lib/n.ts", &[]),
    ]);
    let folders: Vec<String> = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT)
        .into_iter()
        .map(|s| s.folder)
        .collect();
    assert!(folders.contains(&"lib".to_string()));
    assert!(!folders.contains(&"tests".to_string()));
    assert!(!folders.contains(&"playwright".to_string()));
}

#[test]
fn external_and_test_dir_imports_do_not_inflate_boundary() {
    let d = dep(&[
        // 1 real internal + external + test-dir
        (
            "billing/a.ts",
            &["billing/b.ts", "lodash", "tests/fixture.ts"],
        ),
        ("billing/b.ts", &[]),
        ("billing/c.ts", &[]),
        // note: "lodash" and "tests/fixture.ts" are NOT dep keys -> external / noise
    ]);
    let seams = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT);
    let billing = seams.iter().find(|s| s.folder == "billing").unwrap();
    assert_eq!(billing.internal_edges, 1);
    assert_eq!(billing.boundary_edges, 0); // lodash (external) + tests/ (noise) both ignored
    assert_eq!(billing.cohesion, 1.0);
}

#[test]
fn cohesion_is_full_for_self_contained_folder() {
    let d = dep(&[
        ("m/a.ts", &["m/b.ts"]),
        ("m/b.ts", &["m/c.ts"]),
        ("m/c.ts", &[]),
    ]);
    let seams = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT);
    assert_eq!(seams[0].cohesion, 1.0);
    assert_eq!(seams[0].boundary_edges, 0);
}

#[test]
fn cross_folder_co_change_demotes_statically_clean_folder() {
    // billing is statically self-contained (boundary 0) but always co-changes with ui across the boundary.
    let d = dep(&[
        ("billing/a.ts", &["billing/b.ts"]),
        ("billing/b.ts", &[]),
        ("ui/x.ts", &["ui/y.ts"]),
        ("ui/y.ts", &[]),
    ]);
    let mut coupling = CouplingMap::new();
    coupling.insert(
        "billing/a.ts".to_string(),
        vec![CouplingEntry {
            path: "ui/x.ts".into(),
            count: 12,
        }],
    );
    let clean_seams = compute_seams(&d, &CouplingMap::new(), 2, SEAMS_LIMIT);
    let clean = clean_seams.iter().find(|s| s.folder == "billing").unwrap();
    let entangled_seams = compute_seams(&d, &coupling, 2, SEAMS_LIMIT);
    let entangled = entangled_seams
        .iter()
        .find(|s| s.folder == "billing")
        .unwrap();
    assert_eq!(clean.temporal_boundary, 0);
    assert_eq!(entangled.temporal_boundary, 12);
    assert!(entangled.score < clean.score); // temporal coupling lowers the seam score
}
