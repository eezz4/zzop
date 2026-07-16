use std::collections::{BTreeMap, HashMap};

use zzop_core::{http_interface_key, IoProvide};

/// Resolves `ControllerPrefixRouteFragment`s (`controller-prefix-ref-v1` — a `@Controller(RouteKey.Asset)`
/// dotted member-expression prefix, deferred by `zzop_parser_typescript::extract_controller_prefix_route_fragments`
/// because a single file can't see where `RouteKey` is declared) into whole-tree `http` `IoProvide`s.
///
/// `consts` is the SAME project-wide merged constant map [`late_resolve_cross_file_consumes`] uses
/// (built by [`merge_const_map_fragments`], which now also folds string-valued `enum` members — see
/// `zzop_parser_typescript::const_map_fragment`'s doc) — a caller computes it once and passes it to both.
///
/// A `prefix_ref` present in `consts` resolves exactly like `extract_controller_provides`'s own literal
/// path: `"{prefix}/{path}"` joined and normalized via `http_interface_key`. A `prefix_ref` ABSENT from
/// `consts` never guesses — instead one `warnings` entry is pushed per distinct `(file, prefix_ref)`
/// pair, naming the ref, the file, and how many routes were dropped, and no provide is emitted for any
/// of that controller's fragments.
///
/// ## Placement (load-bearing — see `zzop_engine::analyze::mod`'s call site)
/// Must run BEFORE `apply_and_strip_global_prefix`: a NestJS tree can compose BOTH a `RouteKey.Asset` ->
/// `assets` prefix resolution here AND a `setGlobalPrefix('api')` rewrite there on the very same route
/// (`GET /api/assets/{}`) — this function's output has to already be in `io_provides` for the global-
/// prefix seam to see and prepend it, same requirement every other per-file-composed provide has at
/// that seam.
pub(crate) fn compose_controller_prefix_provides(
    fragments: Vec<(String, Vec<zzop_core::ControllerPrefixRouteFragment>)>,
    consts: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Vec<IoProvide> {
    let mut fragments = fragments;
    fragments.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    // `(file, prefix_ref) -> count of routes dropped` — one aggregated warning per distinct pair rather
    // than one per route, so a 5-route controller with an unresolvable prefix produces one honest line,
    // not five.
    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();

    for (file, frags) in &fragments {
        for frag in frags {
            match consts.get(&frag.prefix_ref) {
                Some(prefix) => {
                    let full_path = format!("{prefix}/{}", frag.path);
                    out.push(IoProvide {
                        // Carried through so a prefix-ref route's composed `IoProvide` keeps the same
                        // body evidence a literal-prefix route gets directly (`ControllerPrefixRouteFragment`
                        // doc) — `resolve_provide_body_refs` (below) resolves its `dto_ref` afterward
                        // exactly like any other provide's.
                        body: frag.body.clone(),
                        kind: "http".to_string(),
                        key: http_interface_key(&frag.verb, &full_path),
                        file: file.clone(),
                        line: frag.line,
                        symbol: frag.symbol.clone(),
                    });
                }
                None => {
                    *unresolved
                        .entry((file.clone(), frag.prefix_ref.clone()))
                        .or_insert(0) += 1;
                }
            }
        }
    }

    for ((file, prefix_ref), count) in unresolved {
        let route_word = if count == 1 { "route" } else { "routes" };
        warnings.push(format!(
            "could not resolve controller prefix `{prefix_ref}` ({file}) to a literal — its {count} {route_word} are not projected; the prefix constant may live in an unanalyzed file"
        ));
    }

    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

#[cfg(test)]
mod controller_prefix_compose_tests {
    //! Coverage for `compose_controller_prefix_provides`: literal resolution against the merged const
    //! map, the never-guess unresolved warning (aggregated per `(file, prefix_ref)`, singular/plural
    //! wording), a resolved and an unresolved controller side by side, and determinism.
    use super::*;
    use zzop_core::ControllerPrefixRouteFragment;

    fn frag(
        prefix_ref: &str,
        verb: &str,
        path: &str,
        line: u32,
        symbol: &str,
    ) -> ControllerPrefixRouteFragment {
        ControllerPrefixRouteFragment {
            body: None,
            prefix_ref: prefix_ref.to_string(),
            verb: verb.to_string(),
            path: path.to_string(),
            line,
            symbol: Some(symbol.to_string()),
        }
    }

    fn consts(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn resolved_prefix_ref_composes_a_joined_provide() {
        let fragments = vec![(
            "src/asset.controller.ts".to_string(),
            vec![
                frag("RouteKey.Asset", "GET", ":id", 3, "getById"),
                frag("RouteKey.Asset", "DELETE", "", 6, "remove"),
            ],
        )];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["DELETE /assets", "GET /assets/{}"]);
        assert!(warnings.is_empty());
        let get = out.iter().find(|p| p.key == "GET /assets/{}").unwrap();
        assert_eq!(get.file, "src/asset.controller.ts");
        assert_eq!(get.line, 3);
        assert_eq!(get.symbol.as_deref(), Some("getById"));
    }

    #[test]
    fn unresolved_prefix_ref_drops_its_routes_and_warns_once_per_file_and_ref() {
        let fragments = vec![(
            "controller.ts".to_string(),
            vec![
                frag("RouteKey.Asset", "GET", "a", 1, "a"),
                frag("RouteKey.Asset", "GET", "b", 2, "b"),
            ],
        )];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(fragments, &consts(&[]), &mut warnings);
        assert!(out.is_empty(), "never guess an unresolved prefix: {out:?}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("RouteKey.Asset"));
        assert!(warnings[0].contains("controller.ts"));
        assert!(warnings[0].contains("2 routes"));
    }

    #[test]
    fn singular_route_count_uses_singular_wording() {
        let fragments = vec![(
            "controller.ts".to_string(),
            vec![frag("RouteKey.Asset", "GET", "a", 1, "a")],
        )];
        let mut warnings = Vec::new();
        compose_controller_prefix_provides(fragments, &consts(&[]), &mut warnings);
        assert!(warnings[0].contains("1 route "), "{warnings:?}");
        assert!(!warnings[0].contains("1 routes"), "{warnings:?}");
    }

    #[test]
    fn resolved_and_unresolved_controllers_are_independent() {
        let fragments = vec![
            (
                "resolved.controller.ts".to_string(),
                vec![frag("RouteKey.Asset", "GET", "a", 1, "a")],
            ),
            (
                "unresolved.controller.ts".to_string(),
                vec![frag("RouteKey.Missing", "GET", "b", 1, "b")],
            ),
        ];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "GET /assets/a");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("RouteKey.Missing"));
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "a.controller.ts".to_string(),
                    vec![frag("RouteKey.A", "GET", "a", 1, "a")],
                ),
                (
                    "b.controller.ts".to_string(),
                    vec![frag("RouteKey.B", "GET", "b", 1, "b")],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let c = consts(&[("RouteKey.A", "a-prefix"), ("RouteKey.B", "b-prefix")]);
        let mut w1 = Vec::new();
        let mut w2 = Vec::new();
        let out1 = compose_controller_prefix_provides(build(false), &c, &mut w1);
        let out2 = compose_controller_prefix_provides(build(true), &c, &mut w2);
        let view = |v: &[IoProvide]| -> Vec<(String, String, u32)> {
            v.iter()
                .map(|p| (p.key.clone(), p.file.clone(), p.line))
                .collect()
        };
        assert_eq!(view(&out1), view(&out2));
    }

    #[test]
    fn body_shape_is_carried_through_onto_the_composed_provide() {
        // `ControllerPrefixRouteFragment.body` (`body-shape-v1`) must survive the prefix-ref join
        // unchanged — `resolve_provide_body_refs` resolves its `dto_ref` in a LATER pass, over whatever
        // `io_provides` holds by then, this composer included.
        let mut with_body = frag("RouteKey.Asset", "POST", "", 1, "create");
        with_body.body = Some(zzop_core::ProvideBodyShape {
            sub_key: None,
            dto_ref: Some("CreateAssetDto".to_string()),
            fields: Vec::new(),
            complete: false,
        });
        let fragments = vec![("asset.controller.ts".to_string(), vec![with_body])];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        assert_eq!(out.len(), 1);
        let body = out[0].body.as_ref().expect("body carried through");
        assert_eq!(body.dto_ref.as_deref(), Some("CreateAssetDto"));
    }
}
