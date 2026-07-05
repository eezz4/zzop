//! Strangler seam scoring — answers the central legacy-migration question: *where do we cut first?* A good first
//! extraction is a folder that is highly self-contained (most of its import edges stay inside it) and barely tied to
//! the rest (few edges cross its boundary). Such a module can be lifted into a service / package with minimal blast.
//!
//! Per top-level folder: `internal` = import edges with both endpoints inside, `boundary` = edges crossing in or out.
//! `cohesion = internal / (internal + boundary)`, `score = internal / (boundary + 1)` (higher = cleaner seam). Pure
//! over the dep graph — language-agnostic. The folder granularity is the first path segment (the natural layer/module
//! unit, same as cross-layer co-churn).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::coupling::CouplingMap;
use zzop_core::DepGraph;

/// Minimum files in a folder to be a meaningful seam.
pub const SEAMS_MIN_FILES: usize = 3;
/// Max rows returned (best seams first).
pub const SEAMS_LIMIT: usize = 15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeamCandidate {
    pub folder: String,
    pub files: usize,
    /// Import edges fully inside the folder.
    pub internal_edges: u32,
    /// Import edges crossing the folder boundary (inbound + outbound).
    pub boundary_edges: u32,
    pub inbound: u32,
    pub outbound: u32,
    /// Co-change links crossing the folder boundary — temporal coupling the static graph misses. A folder that is
    /// statically clean but always changes WITH other folders is not a cheap extraction (the coupling is real, just
    /// hidden). Counts toward the boundary in the score.
    pub temporal_boundary: u32,
    /// internal / (internal + boundary) — 1.0 = fully self-contained.
    pub cohesion: f64,
    /// internal / (boundary + temporal_boundary + 1) — higher = cheaper to extract. Ranking key.
    pub score: f64,
}

/// `coupling` is the co-change map — cross-folder co-change is added to each folder's boundary so a
/// statically-clean but temporally-entangled folder is correctly demoted as an extraction candidate.
pub fn compute_seams(
    dep: &DepGraph,
    coupling: &CouplingMap,
    min_files: usize,
    limit: usize,
) -> Vec<SeamCandidate> {
    // Every analyzed file is a dep key; a target outside this set is external.
    let in_repo: HashSet<&str> = dep.keys().map(|s| s.as_str()).collect();
    let mut file_count: BTreeMap<&str, usize> = BTreeMap::new();
    for file in &in_repo {
        let folder = folder_of(file);
        if is_noise_folder(folder) {
            continue; // test/build/config dirs are never extraction targets
        }
        *file_count.entry(folder).or_insert(0) += 1;
    }

    let mut internal: BTreeMap<&str, u32> = BTreeMap::new();
    let mut inbound: BTreeMap<&str, u32> = BTreeMap::new();
    let mut outbound: BTreeMap<&str, u32> = BTreeMap::new();
    for (importer, imports) in dep {
        let from = folder_of(importer);
        if is_noise_folder(from) {
            continue; // edges out of a test/build dir don't shape a real seam
        }
        for imported in imports {
            // Skip external/bare modules (not an in-repo file) and edges into test/build dirs — neither is real
            // cross-module coupling, and counting them would inflate a clean folder's boundary and unfairly demote
            // its seam score.
            if !in_repo.contains(imported.as_str()) {
                continue;
            }
            let to = folder_of(imported);
            if is_noise_folder(to) {
                continue;
            }
            if from == to {
                *internal.entry(from).or_insert(0) += 1;
            } else {
                *outbound.entry(from).or_insert(0) += 1;
                *inbound.entry(to).or_insert(0) += 1;
            }
        }
    }

    // Cross-folder co-change: for each coupled pair in different (non-noise, in-repo) folders, add the co-change
    // count to the source folder's temporal boundary. The CouplingMap is symmetric, so each folder accrues its own
    // crossings.
    let mut temporal: BTreeMap<&str, u32> = BTreeMap::new();
    for (file, partners) in coupling {
        let from = folder_of(file);
        if is_noise_folder(from) || !file_count.contains_key(from) {
            continue;
        }
        for p in partners {
            let to = folder_of(&p.path);
            if to == from || is_noise_folder(to) || !file_count.contains_key(to) {
                continue;
            }
            *temporal.entry(from).or_insert(0) += p.count;
        }
    }

    let mut out: Vec<SeamCandidate> = Vec::new();
    for (&folder, &files) in &file_count {
        if files < min_files {
            continue;
        }
        let in_edges = internal.get(folder).copied().unwrap_or(0);
        let inb = inbound.get(folder).copied().unwrap_or(0);
        let outb = outbound.get(folder).copied().unwrap_or(0);
        let boundary = inb + outb;
        let temporal_boundary = temporal.get(folder).copied().unwrap_or(0);
        out.push(SeamCandidate {
            folder: folder.to_string(),
            files,
            internal_edges: in_edges,
            boundary_edges: boundary,
            inbound: inb,
            outbound: outb,
            temporal_boundary,
            cohesion: if in_edges + boundary == 0 {
                0.0
            } else {
                f64::from(in_edges) / f64::from(in_edges + boundary)
            },
            score: f64::from(in_edges) / f64::from(boundary + temporal_boundary + 1),
        });
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.internal_edges.cmp(&a.internal_edges))
            .then_with(|| a.folder.cmp(&b.folder))
    });
    out.truncate(limit);
    out
}

/// Top-level folders that are never strangler extraction targets — surfacing them as "best seams" is noise (they are
/// naturally self-contained but you do not extract a test/build dir).
fn is_noise_folder(folder: &str) -> bool {
    static NOISE: &[&str] = &[
        "tests",
        "test",
        "e2e",
        "__tests__",
        "__test__",
        "spec",
        "playwright",
        "cypress",
        "fixtures",
        "mocks",
        "__mocks__",
        "stories",
        "docs",
        "doc",
        "examples",
        "example",
        "node_modules",
        "dist",
        "build",
    ];
    NOISE.contains(&folder)
}

/// First path segment — the natural module/layer unit. "(root)" when the file sits at the analyzed root.
fn folder_of(path: &str) -> &str {
    match path.find('/') {
        Some(i) => &path[..i],
        None => "(root)",
    }
}

#[cfg(test)]
mod tests {
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
}
