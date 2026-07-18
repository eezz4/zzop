//! S7 per-app fetch-wrapper census — the de-masking dual of `fetch_wrapper_call_site_warning`. Split
//! from the parent `fetch_wrapper` module purely to keep each source file under the line-length cap;
//! all PASS 1/PASS 2 mechanics and tree-wide formatting live in the parent and are reused here via
//! `super::`.

use std::collections::BTreeMap;
use std::path::Path;

use super::super::app_buckets::nearest_app_root;
use super::super::builtin_fetch::{is_js_ts_family, FETCH_CALL_SITES_MIN};
use super::super::controller_silence::MIN_PROVIDES_FLOOR;
use super::{
    call_site_count, fetch_wrapper_call_site_warning, find_wrapper, format_tree_wide_s7,
    imports_the_wrapper, sample_of, wrapper_stem_of, WRAPPER_FUNNEL_TAIL,
};

/// The per-app S7 warning string — names the below-floor app bucket whose wrapper call-site mass a
/// healthy sibling app would otherwise MASK under a tree-wide gate. Same [`WRAPPER_FUNNEL_TAIL`] as the
/// tree-wide wording; the intro is analogous to S5's app-scoped wording (names the app, calls out the
/// internal-relative URLs).
fn format_app_scoped_s7(
    wrapper_rel: &str,
    name_list: &str,
    app_root: &str,
    total: usize,
    keyed: usize,
    matched_files: &[String],
) -> String {
    let refs: Vec<&str> = matched_files.iter().map(String::as_str).collect();
    let sample_str = sample_of(&refs);
    format!(
        "{wrapper_rel} exports a fetch-wrapper idiom ({name_list}) with {total} cross-file call site(s) \
with internal-relative URLs across {} file(s) that import it within app `{app_root}` (e.g. {sample_str}) \
but only {keyed} keyed http consume(s) were extracted for that app — {WRAPPER_FUNNEL_TAIL}",
        matched_files.len()
    )
}

/// The per-app fetch-wrapper census — the de-masking dual of
/// [`fetch_wrapper_call_site_warning`](super::fetch_wrapper_call_site_warning), mirroring
/// [`builtin_fetch_census`](crate::framework_silence::builtin_fetch_census)'s per-app + fallback
/// composition but with the PASS 1 (tree-wide wrapper detection) + PASS 2 (per-importer,
/// internal-intent call-site count) mechanics.
///
/// - Single-package (`app_roots == [""]`): reduces exactly to the tree-wide path — healthy => empty,
///   else delegate to [`fetch_wrapper_call_site_warning`](super::fetch_wrapper_call_site_warning) (a
///   0/1-element vec).
/// - Multi-package: PASS 1 finds the tree-wide wrapper; PASS 2 attributes each importer's
///   internal-intent call-site count to its [`nearest_app_root`], reading ONLY below-floor buckets'
///   files. Per-app fire: every below-floor bucket clearing [`FETCH_CALL_SITES_MIN`] pushes one
///   app-scoped warning (sorted `app_roots` order). Tree fallback: if NOTHING fired per-app, yet the
///   tree-wide keyed sum is below floor AND the below-floor buckets' aggregate call-site total clears
///   [`FETCH_CALL_SITES_MIN`], push ONE tree-wide-worded warning.
pub fn fetch_wrapper_census(
    root: &Path,
    all_rels: &[String],
    keyed_by_root: &BTreeMap<String, usize>,
    app_roots: &[String],
) -> Vec<String> {
    if app_roots.len() == 1 && app_roots[0].is_empty() {
        let keyed = keyed_by_root.get("").copied().unwrap_or(0);
        if keyed >= MIN_PROVIDES_FLOOR {
            return Vec::new();
        }
        return fetch_wrapper_call_site_warning(root, all_rels, keyed)
            .into_iter()
            .collect();
    }

    let js_rels: Vec<&str> = all_rels
        .iter()
        .map(String::as_str)
        .filter(|rel| is_js_ts_family(rel))
        .collect();
    let Some((wrapper_rel, matched_names)) = find_wrapper(root, &js_rels) else {
        return Vec::new();
    };
    let wrapper_stem = wrapper_stem_of(wrapper_rel);
    let name_list = matched_names.join("/");

    let mut per_bucket: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
    for &rel in &js_rels {
        if rel == wrapper_rel {
            continue;
        }
        let bucket = nearest_app_root(rel, app_roots);
        if keyed_by_root.get(bucket).copied().unwrap_or(0) >= MIN_PROVIDES_FLOOR {
            continue; // healthy bucket: never read
        }
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !imports_the_wrapper(&text, wrapper_stem) {
            continue;
        }
        let count: usize = matched_names
            .iter()
            .map(|name| call_site_count(&text, name))
            .sum();
        if count > 0 {
            let entry = per_bucket.entry(bucket).or_default();
            entry.0 += count;
            entry.1.push(rel.to_string());
        }
    }

    let mut warnings = Vec::new();
    let mut any_fired = false;
    for bucket in app_roots {
        let keyed = keyed_by_root.get(bucket).copied().unwrap_or(0);
        if keyed >= MIN_PROVIDES_FLOOR {
            continue;
        }
        if let Some((total, files)) = per_bucket.get(bucket.as_str()) {
            if *total >= FETCH_CALL_SITES_MIN {
                warnings.push(format_app_scoped_s7(
                    wrapper_rel,
                    &name_list,
                    bucket,
                    *total,
                    keyed,
                    files,
                ));
                any_fired = true;
            }
        }
    }

    if !any_fired {
        let tree_keyed: usize = keyed_by_root.values().sum();
        let agg_total: usize = per_bucket.values().map(|(t, _)| *t).sum();
        if tree_keyed < MIN_PROVIDES_FLOOR && agg_total >= FETCH_CALL_SITES_MIN {
            let mut all_files: Vec<&str> = Vec::new();
            for bucket in app_roots {
                if let Some((_, files)) = per_bucket.get(bucket.as_str()) {
                    all_files.extend(files.iter().map(String::as_str));
                }
            }
            warnings.push(format_tree_wide_s7(
                wrapper_rel,
                &name_list,
                agg_total,
                tree_keyed,
                &all_files,
            ));
        }
    }

    warnings
}
