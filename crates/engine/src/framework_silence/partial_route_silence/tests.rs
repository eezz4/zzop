//! The axis: a tree missing SOME of its routes must not look identical to a healthy one.

use super::*;

fn imports(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(spec, files)| {
            (
                spec.to_string(),
                files.iter().map(|f| f.to_string()).collect(),
            )
        })
        .collect()
}

fn files(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

/// The measured shape: 14 routes in the source, 9 extracted, and the two files that register the
/// missing 5 take their `app` as a parameter. Every tree-wide tripwire stays quiet at 9 routes.
#[test]
fn framework_importing_files_that_produced_no_route_are_named() {
    let w = partial_route_silence_warning(
        &imports(&[(
            "express",
            &[
                "src/app.ts",
                "src/routes/orders.ts",
                "src/routes/reports.ts",
            ],
        )]),
        &files(&["src/app.ts"]),
        9,
    )
    .expect("two silent files is the reportable shape");
    assert!(w.contains("2 file(s) import a server framework"), "{w}");
    assert!(w.contains("src/routes/orders.ts"), "{w}");
    assert!(w.contains("src/routes/reports.ts"), "{w}");
    // The consequence is the part a reader cannot infer: partial loss does not read as silence.
    assert!(w.contains("confidently wrong"), "{w}");
}

/// TOTAL silence belongs to S1/S2, which say it better and name the framework. Two warnings about one
/// silence read as two problems.
#[test]
fn a_tree_with_no_routes_at_all_is_left_to_the_tree_wide_tripwires() {
    assert!(partial_route_silence_warning(
        &imports(&[("express", &["a.ts", "b.ts", "c.ts"])]),
        &BTreeSet::new(),
        0,
    )
    .is_none());
}

/// The invalidation that keeps this from being noise: a single silent importer is the ordinary
/// middleware/types/helper case, and firing on it would put a warning on healthy trees.
#[test]
fn one_silent_framework_importing_file_is_not_reported() {
    assert!(partial_route_silence_warning(
        &imports(&[("express", &["src/app.ts", "src/middleware/auth.ts"])]),
        &files(&["src/app.ts"]),
        9,
    )
    .is_none());
}

/// A tree where every framework-importing file produced routes has nothing to disclose.
#[test]
fn a_tree_whose_importers_all_produced_routes_stays_silent() {
    assert!(partial_route_silence_warning(
        &imports(&[("express", &["a.ts", "b.ts", "c.ts"])]),
        &files(&["a.ts", "b.ts", "c.ts"]),
        9,
    )
    .is_none());
}

/// Non-framework imports must not pull a file into the census — an http CLIENT says nothing about
/// whether this file serves routes, the same distinction S2's own vocabulary draws.
#[test]
fn a_file_importing_only_a_client_library_is_not_counted() {
    assert!(partial_route_silence_warning(
        &imports(&[("axios", &["src/api/a.ts", "src/api/b.ts", "src/api/c.ts"])]),
        &BTreeSet::new(),
        9,
    )
    .is_none());
}

/// The sample is capped and says how many it left out — the repo's standing rule that a truncated list
/// discloses its truncation rather than reading as the whole set.
#[test]
fn a_long_list_is_sampled_and_says_how_many_it_left_out() {
    let w = partial_route_silence_warning(
        &imports(&[("koa", &["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts"])]),
        &files(&["a.ts"]),
        4,
    )
    .expect("five silent files");
    assert!(w.contains("5 file(s) import a server framework"), "{w}");
    assert!(w.contains("and 2 more"), "{w}");
}

/// The remedy has to be one that exists. Both routes out are named, including the cheap one — a
/// reader who only needs routes should not be sent to write an adapter.
#[test]
fn the_warning_names_both_ways_out() {
    let w = partial_route_silence_warning(
        &imports(&[("express", &["a.ts", "b.ts", "c.ts"])]),
        &files(&["a.ts"]),
        4,
    )
    .unwrap();
    assert!(w.contains("Mode B overlay adapter"), "{w}");
    assert!(w.contains("trees[].routes"), "{w}");
}
