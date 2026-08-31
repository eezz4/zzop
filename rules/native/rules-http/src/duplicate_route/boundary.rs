//! Manifest-boundary disclosure for `duplicate-route` — the NON-ERASING half of the 2026-08-17 ruling
//! (`.claude` §24), and the reason this file exists rather than a filter living in the rule body.
//!
//! The rule cannot know whether two registrations reach the same process. A manifest boundary (the
//! directory of a manifest that declares a deployable unit — [`is_deployment_manifest`] is the one
//! place that says which files those are) is the closest thing the tree carries to a deployment unit,
//! and it is deliberately never used to DROP a finding: two sites in different boundaries get that
//! FACT added to the finding, and the finding still fires. (It does move the SEVERITY — see the
//! section below, added 2026-08-20 — but nothing is ever deleted.) The asymmetry is
//! the project's standing rule about evidence — a finding raised on a heuristic is one a reader
//! dismisses in seconds, while a finding DELETED by one is a real shadow that leaves no trace, so the
//! erasing direction requires a DECLARATION. `package.json` declares packaging, not processes: a shared
//! library package can register routes that an app package mounts, which is two boundaries inside one
//! process.
//!
//! ## The third direction: it moves the SEVERITY (2026-08-20)
//! Disclosure and erasure are not the only two things a heuristic can do, and treating them as if they
//! were left the whole cost of the missing evidence on the reader. A `warning` gates CI: `--fail-on
//! warning` reads the counts, so every straddling pair broke a build over a question the rule states,
//! in its own message, that it cannot answer. Measured on macrozheng/mall (0504e86b): four
//! `@SpringBootApplication` classes, four ports, ZERO pom dependencies between the application modules,
//! and 7 of 7 straddling findings false — and the message's own remedy (split into `trees[]`) was
//! measured MORE expensive than the disease, because splitting by module cuts 410 cross-module import
//! edges into a 258-file shared artifact and leaves that artifact with no in-tree consumers at all. So
//! the reader's only working answer was to turn the rule off, which is the outcome the message
//! explicitly tells them not to choose.
//!
//! A straddling pair is therefore reported at `info`: still raised, still named, still carrying both
//! boundary names — but out of the gate. That is the honest middle: erasing needs a declaration and a
//! manifest is not one, while gating needs evidence and a manifest is not that either. Everything the
//! disclose/erase asymmetry protects is intact, because nothing is deleted.
//!
//! Nearest-ancestor wins, the same resolution `go.mod` itself uses: a file under a nested package
//! belongs to that package, never to an enclosing one. A file with no manifest above it has no boundary,
//! and `None` here means "not measured", never "same boundary as the other site".

use std::collections::BTreeSet;

/// Exact file names that declare a deployable/packaged unit, one row per ecosystem, and half of
/// [`is_deployment_manifest`] (the other half is `DEPLOYMENT_MANIFEST_SUFFIXES`). Sorted by name so
/// a new row lands in an obvious place instead of at the end, where a duplicate hides.
const DEPLOYMENT_MANIFEST_NAMES: &[&str] = &[
    "Cargo.toml", // Rust — see [`is_deployment_manifest`] on why the earlier exclusion is not one
    "build.gradle", // JVM (Gradle)
    "build.gradle.kts",
    "composer.json",  // PHP
    "go.mod",         // Go
    "package.json",   // npm — TypeScript/JavaScript
    "pom.xml",        // JVM (Maven)
    "pyproject.toml", // Python, PEP 621 distribution
    "setup.py",       // Python, pre-PEP-621 setuptools
];

/// The one manifest family whose file NAME is not fixed: .NET names a project file after the project
/// itself (`Mall.Admin.csproj`), so it is recognized by extension instead of enumerated. Matched with a
/// length guard, so a file named exactly `.csproj` — an extension with no project name in front of it —
/// is not a project.
const DEPLOYMENT_MANIFEST_SUFFIXES: &[&str] = &[".csproj", ".fsproj", ".vbproj"];

/// Whether `file_name` is a manifest that DECLARES a deployable/packaged unit. `nearest_manifest_dir`
/// answers *which* boundary governs a file; this answers *which directories are boundaries at all*, and
/// the two live together so that "what does zzop count as a boundary" has ONE owner instead of one per
/// producer. Takes a bare file name (`pom.xml`), never a path.
///
/// ## This is a hand-written list, and a hand-written list is what left Maven outside
/// It is a list because there is no shape to fold these into: a manifest is recognized by the filename
/// its ecosystem chose, and those are proper nouns. The only sub-family that HAS a derivable shape is
/// .NET's `<Project>.??proj`, which is why that one is matched by extension rather than enumerated. So
/// the defense is not that the list is complete — the next ecosystem will be outside it too — but that
/// what it cannot see is written down here rather than left as silence, and that the vocabulary sits in
/// the module that CONSUMES the result instead of being spelled out at each producer.
///
/// **What it cannot see**, deliberately:
/// - **Ecosystems with no row**: Ruby (`Gemfile`), Elixir (`mix.exs`), Dart (`pubspec.yaml`), Swift
///   (`Package.swift`), and every ecosystem after them. Such a tree measures zero boundaries, and zero
///   boundaries reads as "not measured" — never as "same boundary" (see `disclosure`).
/// - **Environment and lock descriptors**: `requirements.txt`, `Gemfile.lock`, `package-lock.json`.
///   Those mark where dependencies get installed, not a unit that ships. A per-service
///   `requirements.txt` often IS the deployment marker in a Python monorepo, so this is the exclusion
///   most likely to be revisited — by a measurement, not by a hunch.
/// - **Container and orchestration descriptors**: `Dockerfile`, `Procfile`, Kubernetes manifests. These
///   declare a PROCESS, which is strictly stronger evidence than packaging for the question this rule
///   cannot answer — strong enough that admitting them is a decision about the disclose/erase asymmetry
///   itself, not a quiet addition to a packaging vocabulary.
///
/// ## `Cargo.toml` is in, and that is not a silent reversal
/// This module used to state that Rust contributes no boundaries. The reason given was mechanical and
/// not a judgment: the analysis holds a Rust index of crate NAMES to root files and no Cargo.toml
/// directory set, so there was nothing to hand over. A vocabulary keyed on the manifest FILE NAME has
/// no such gap, so the stated reason stops holding and the row is here. A Rust crate is packaging
/// rather than a process — exactly like `package.json` — so it joins on the same disclose-only footing
/// as every other row.
///
/// ## Who asks
/// The producer that builds the `dirs` set `nearest_manifest_dir` resolves against is
/// `zzop_engine::analyze::assemble::collect`, which asks this predicate of every walked file name. It
/// asks BEFORE any parser-dispatch gate on purpose: a `pom.xml` has no parser frontend, and gating the
/// question on one is what previously reduced this axis to the two ecosystems whose import resolvers
/// happened to index their manifests. `disclosure` names no manifest kind in its sentence, so widening
/// this table cannot make a finding claim a kind the run never read.
pub fn is_deployment_manifest(file_name: &str) -> bool {
    DEPLOYMENT_MANIFEST_NAMES.contains(&file_name)
        || DEPLOYMENT_MANIFEST_SUFFIXES
            .iter()
            .any(|ext| file_name.len() > ext.len() && file_name.ends_with(ext))
}

/// The boundary a file belongs to: the longest manifest directory that is an ancestor of `rel` (or the
/// tree root, spelled `""`, when a root-level manifest exists). `None` when no manifest covers it.
pub(super) fn nearest_manifest_dir<'a>(rel: &str, dirs: &'a BTreeSet<String>) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for d in dirs {
        let covers = if d.is_empty() {
            true
        } else {
            rel.len() > d.len() && rel.starts_with(d.as_str()) && rel.as_bytes()[d.len()] == b'/'
        };
        if covers && best.is_none_or(|b| d.len() > b.len()) {
            best = Some(d.as_str());
        }
    }
    best
}

/// The sentence appended to the message when the two sites sit in different manifest boundaries, and
/// the `data` payload that carries the same fact in a machine-readable field. Returns `None` when the
/// boundaries match, when either side is unmeasured, or when no manifests were scanned at all — in each
/// of those cases there is nothing established to disclose, and the rule says nothing extra.
pub(super) fn disclosure(
    first_file: &str,
    dup_file: &str,
    dirs: &BTreeSet<String>,
) -> Option<(String, serde_json::Value)> {
    if dirs.is_empty() {
        return None;
    }
    let a = nearest_manifest_dir(first_file, dirs)?;
    let b = nearest_manifest_dir(dup_file, dirs)?;
    if a == b {
        return None;
    }
    let label = |d: &str| {
        if d.is_empty() {
            "<tree root>".to_string()
        } else {
            d.to_string()
        }
    };
    let sentence = format!(
        " Manifest boundaries DIFFER between the two sites — `{}` and `{}` (the nearest ancestor \
         directory carrying a deployable-unit manifest, for each site). THAT IS WHY THIS FINDING IS \
         REPORTED AT `info` rather than `warning`: the counts a `--fail-on warning` gate reads no longer \
         include it, because gating a build on a question this rule holds no evidence for is a cost paid \
         by everyone whose monorepo simply has more than one unit in it. The finding is NOT cleared, \
         because a manifest declares PACKAGING and not a process — a shared package can register routes \
         that an app package mounts, which is two boundaries inside one process — so it still stands, \
         named, for a reader to judge. A site with NO boundary named here was not measured — some \
         ecosystems declare a deployable unit in a file this analysis does not recognize — and an \
         unmeasured site is never reported as differing from the other one, so it keeps the warning.",
        label(a),
        label(b)
    );
    let data = serde_json::json!({ "first": label(a), "duplicate": label(b) });
    Some((sentence, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nearest_ancestor_wins_over_an_enclosing_manifest() {
        let d = dirs(&["", "packages/api", "packages/api/nested"]);
        assert_eq!(
            nearest_manifest_dir("packages/api/nested/src/r.ts", &d),
            Some("packages/api/nested")
        );
        assert_eq!(
            nearest_manifest_dir("packages/api/src/r.ts", &d),
            Some("packages/api")
        );
        assert_eq!(nearest_manifest_dir("src/r.ts", &d), Some(""));
    }

    /// A directory that merely shares a name PREFIX is not an ancestor — `packages/api-v2` must not be
    /// read as living under `packages/api`.
    #[test]
    fn a_sibling_sharing_a_name_prefix_is_not_an_ancestor() {
        let d = dirs(&["packages/api"]);
        assert_eq!(nearest_manifest_dir("packages/api-v2/src/r.ts", &d), None);
    }

    #[test]
    fn no_manifest_above_the_file_is_unmeasured_rather_than_root() {
        let d = dirs(&["packages/api"]);
        assert_eq!(nearest_manifest_dir("services/x/r.go", &d), None);
    }

    #[test]
    fn same_boundary_discloses_nothing() {
        let d = dirs(&["packages/api"]);
        assert!(disclosure("packages/api/a.ts", "packages/api/b.ts", &d).is_none());
    }

    /// The unmeasured side must not be reported as a difference — `None` means "not measured", and
    /// treating it as a distinct boundary would manufacture a fact out of a missing manifest.
    #[test]
    fn an_unmeasured_side_discloses_nothing() {
        let d = dirs(&["packages/api"]);
        assert!(disclosure("packages/api/a.ts", "services/x/b.go", &d).is_none());
    }

    #[test]
    fn differing_boundaries_disclose_both_names() {
        let d = dirs(&["packages/api", "packages/web"]);
        let (sentence, data) =
            disclosure("packages/api/a.ts", "packages/web/b.ts", &d).expect("differs");
        assert!(sentence.contains("packages/api"));
        assert!(sentence.contains("packages/web"));
        assert!(sentence.contains("NOT cleared"));
        assert_eq!(data["first"], "packages/api");
        assert_eq!(data["duplicate"], "packages/web");
    }

    #[test]
    fn a_tree_with_no_manifests_scanned_discloses_nothing() {
        assert!(disclosure("a.ts", "b.ts", &dirs(&[])).is_none());
    }

    /// The vocabulary must answer for the ecosystems this engine has a frontend for, not just the two
    /// it started with — a Maven or Gradle module, a .NET project, a PHP or Python package. Asserted
    /// together with the names it must REFUSE, so a predicate that simply returned `true` fails here.
    #[test]
    fn every_ecosystem_row_is_a_boundary_and_ordinary_files_are_not() {
        for name in [
            "package.json",
            "go.mod",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "composer.json",
            "pyproject.toml",
            "setup.py",
            "Cargo.toml",
            "Mall.Admin.csproj",
            "Api.fsproj",
            "Legacy.vbproj",
        ] {
            assert!(is_deployment_manifest(name), "{name} must be a boundary");
        }
        for name in [
            "OmsOrderController.java",
            "package.json.bak",
            "my-pom.xml.tpl",
            "README.md",
            ".csproj",
            "csproj",
            "",
        ] {
            assert!(
                !is_deployment_manifest(name),
                "{name} must not be a boundary"
            );
        }
    }

    /// The exclusions this vocabulary documents are STATED, not accidental: an environment/lock
    /// descriptor and a container descriptor are both plausible boundary markers it deliberately does
    /// not read. Pinned so admitting one is a deliberate edit here rather than a silent widening.
    #[test]
    fn the_stated_exclusions_are_actually_excluded() {
        for name in [
            "requirements.txt",
            "package-lock.json",
            "Gemfile",
            "Gemfile.lock",
            "mix.exs",
            "pubspec.yaml",
            "Dockerfile",
            "Procfile",
        ] {
            assert!(
                !is_deployment_manifest(name),
                "{name} is documented as out of scope"
            );
        }
    }
}
