use std::collections::{BTreeMap, HashMap};

use zzop_core::{http_consume_interface_key, IoConsume};

/// Deterministically merges every file's own constant-map fragment into one project-wide map — the
/// shared substrate both [`late_resolve_cross_file_consumes`] (CONSUME re-resolution) and
/// `compose_controller_prefix_provides` (PROVIDE resolution) resolve against, so a `RouteKey.Asset`
/// enum member and an `axios.get(ControlKey.X)` constant are both looked up in exactly the same map.
///
/// `fragments` is sorted by `rel`, then folded first-writer-wins: a constant key duplicated across two
/// files always resolves to the lexicographically smallest file's value, independent of
/// `HashMap`/rayon iteration order. Takes `&[...]` (not by value) so a caller can compute this merged
/// map before separately consuming the same `fragments` `Vec` elsewhere (e.g.
/// `late_resolve_cross_file_consumes`, which still owns its own copy of the merge for its own callers/
/// tests).
pub(crate) fn merge_const_map_fragments(
    fragments: &[(String, HashMap<String, String>)],
) -> HashMap<String, String> {
    let mut sorted: Vec<&(String, HashMap<String, String>)> = fragments.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for (_, fragment) in sorted {
        for (key, value) in fragment {
            merged.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    // `BTreeMap` above exists only so the merge loop itself is deterministic; callers have no ordering
    // requirement of their own, so this returns a plain `HashMap`.
    merged.into_iter().collect()
}

/// Late cross-file constant re-resolution — closes the gap `crate::io`'s module doc documents as the "v1
/// fusion tradeoff": a one-file-slice HTTP egress scan cannot resolve a constant imported from another
/// file, so it emits `IoConsume { key: None, raw: Some(<dotted expr text>), method: Some(<METHOD>) }`
/// instead of guessing. This function fixes that up AFTER every file's own constant-map fragment has
/// been collected, using only data the fused per-file pass already produced — no second parse.
///
/// **Deterministic merge**: delegates to [`merge_const_map_fragments`] — see its own doc.
///
/// **Re-resolution**: every consume with `key: None` whose `raw`/`method` are both `Some` is looked up
/// via `zzop_parser_typescript::resolve_raw_path`; a hit sets `key` to the normalized join key and
/// deliberately keeps `raw` as provenance (this consume was only resolvable via the project-wide
/// constant merge, not from its own file alone). A miss leaves the consume exactly as unresolved as
/// before — this function only ever turns an unresolved consume INTO a resolved one, never the reverse.
///
/// Must run before `io_consumes` is frozen into `MinimalIr::io` — every whole-tree native rule that
/// reads `io_consumes` directly must see the resolved key, not the raw one.
pub(crate) fn late_resolve_cross_file_consumes(
    fragments: Vec<(String, HashMap<String, String>)>,
    io_consumes: &mut [IoConsume],
) {
    let consts = merge_const_map_fragments(&fragments);
    for consume in io_consumes.iter_mut() {
        if consume.key.is_some() {
            continue;
        }
        let (Some(raw), Some(method)) = (consume.raw.as_deref(), consume.method.as_deref()) else {
            continue;
        };
        if let Some(path) = zzop_parser_typescript::resolve_raw_path(raw, &consts) {
            // A leading `/` is an internal route (normalized key); an absolute `http(s)://` URL keeps
            // the verbatim host-carrying key so `link_cross_layer_io`'s `"://"` gate still routes it
            // to the `external` bucket; a base-relative path literal (`users/login` — the axios
            // `baseURL` idiom) keys as its root-normalized form, mirroring the egress extractor's own
            // gating; anything else stays unresolved. Deliberately NO base-carrier head-drop bucket
            // here (unlike `consume_key_for`'s 4-bucket dispatch): `resolve_raw_path` only accepts
            // dotted-const chains whose resolved values are source string literals, so a `{}`-headed
            // assembled variant can never reach this mirror — the omission is structural, not drift.
            if path.starts_with('/') {
                consume.key = Some(http_consume_interface_key(method, &path));
            } else if zzop_parser_typescript::is_external_url(&path) {
                consume.key = Some(format!("{method} {path}"));
            } else if let Some(rooted) = zzop_parser_typescript::base_relative_path(&path) {
                consume.key = Some(http_consume_interface_key(method, &rooted));
            }
        }
    }
}

#[cfg(test)]
mod late_resolve_tests {
    use super::*;

    fn unresolved(raw: &str, method: &str) -> IoConsume {
        IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
            key: None,
            file: "src/caller.ts".to_string(),
            line: 1,
            raw: Some(raw.to_string()),
            method: Some(method.to_string()),
        }
    }

    fn consts(entries: &[(&str, &str)]) -> Vec<(String, HashMap<String, String>)> {
        let fragment: HashMap<String, String> = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        vec![("src/consts.ts".to_string(), fragment)]
    }

    #[test]
    fn slash_value_resolves_to_a_normalized_internal_key() {
        let mut consumes = vec![unresolved("Api.user", "GET")];
        late_resolve_cross_file_consumes(consts(&[("Api.user", "/api/user/")]), &mut consumes);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/user"));
        assert!(consumes[0].raw.is_some()); // provenance retained
    }

    #[test]
    fn absolute_url_value_keeps_the_verbatim_external_key() {
        let mut consumes = vec![unresolved("Api.vendor", "POST")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.vendor", "https://vendor.com/x")]),
            &mut consumes,
        );
        // Verbatim -- `link_cross_layer_io`'s `"://"` gate must still see the host.
        assert_eq!(
            consumes[0].key.as_deref(),
            Some("POST https://vendor.com/x")
        );
    }

    #[test]
    fn base_relative_fragment_value_keys_root_normalized() {
        // Intent change (`base-relative-egress-v1`, cross-layer-resolution decision 2026-07-10): a
        // path-shaped fragment (`authen/getUserInfo` — the axios `baseURL` idiom) keys as its
        // root-normalized path instead of staying unresolved, mirroring the egress extractor's gating.
        let mut consumes = vec![unresolved("Api.frag", "GET")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.frag", "authen/getUserInfo")]),
            &mut consumes,
        );
        assert_eq!(consumes[0].key.as_deref(), Some("GET /authen/getUserInfo"));
    }

    #[test]
    fn non_path_shaped_fragment_value_stays_unresolved() {
        // The never-guess veto list survives the intent change: a document-relative `./` value and a
        // whitespace-carrying value are not base-relative paths.
        let mut consumes = vec![unresolved("Api.rel", "GET"), unresolved("Api.txt", "GET")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.rel", "./authen"), ("Api.txt", "not a path")]),
            &mut consumes,
        );
        assert_eq!(consumes[0].key, None);
        assert_eq!(consumes[1].key, None);
    }
}

#[cfg(test)]
mod merge_const_map_fragments_tests {
    use super::*;

    #[test]
    fn first_writer_wins_by_sorted_rel_regardless_of_input_order() {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("K".to_string(), "from-a".to_string());
        let mut z: HashMap<String, String> = HashMap::new();
        z.insert("K".to_string(), "from-z".to_string());

        let in_order = vec![
            ("a.ts".to_string(), a.clone()),
            ("z.ts".to_string(), z.clone()),
        ];
        let reversed = vec![("z.ts".to_string(), z), ("a.ts".to_string(), a)];

        assert_eq!(
            merge_const_map_fragments(&in_order).get("K"),
            merge_const_map_fragments(&reversed).get("K")
        );
        assert_eq!(
            merge_const_map_fragments(&in_order)
                .get("K")
                .map(String::as_str),
            Some("from-a")
        );
    }
}
