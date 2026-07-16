use zzop_core::IoProvide;

/// NestJS `app.setGlobalPrefix(...)` apply + strip — see `zzop_parser_typescript::adapters::global_prefix`'s
/// module doc for why this rides the `provides` channel as a `nest-global-prefix` sentinel instead of a
/// dedicated field.
///
/// ## Placement (load-bearing — scope correctness)
/// This MUST run at exactly one seam: right after the per-file IO collection loop (which folds every
/// file's `IoFacts.provides` — Nest-controller `http` provides plus the `nest-global-prefix` sentinels —
/// into `io_provides`), and BEFORE the whole-tree provide producers that append OTHER `http` provides:
/// the Java Spring pass (`run_java_provides_project_pass`), Hono/Express router-mount composition
/// (`compose_router_mount_provides`), and file-convention routes (`file_routes`). Those routes carry
/// their own full path (a Next.js `GET /api/foo` is already complete) and must NOT be prefixed — running
/// later would double-prefix them (`GET /api/api/foo`). At this seam `io_provides` holds ONLY per-file
/// provides, so the rewrite is inherently scoped to Nest controllers.
///
/// ## Behavior
/// - Exactly one distinct sentinel value, non-empty after trimming surrounding `/`: every `http`
///   provide's key is rewritten to prepend that prefix (one clean `/` at the seam).
/// - Exactly one distinct sentinel value that normalizes to empty (`''` or `'/'`): a no-op rewrite — an
///   empty global prefix means no prefix — but the sentinel is still stripped.
/// - More than one distinct value: nothing is rewritten — a `warnings` entry is pushed instead (honest
///   degrade over guessing which one is real).
/// - Zero sentinel values: a no-op.
///
/// In every case, every `nest-global-prefix` provide is removed from `io_provides` — the sentinel must
/// never reach output or the cross-layer join.
pub(in crate::analyze) fn apply_and_strip_global_prefix(
    io_provides: &mut Vec<IoProvide>,
    warnings: &mut Vec<String>,
) {
    // Bound to the parser's exported const (not a local literal) so a rename on the emit side
    // cannot silently desynchronize the strip side — a leaked sentinel would reach output.
    const SENTINEL_KIND: &str = zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND;

    let mut prefixes: Vec<String> = io_provides
        .iter()
        .filter(|p| p.kind == SENTINEL_KIND)
        .map(|p| p.key.clone())
        .collect();
    prefixes.sort();
    prefixes.dedup();

    match prefixes.as_slice() {
        [] => {}
        [prefix] => {
            // Trim surrounding slashes on both ends: `setGlobalPrefix('api')`, `'/api'`, and `'api/'`
            // all normalize to `api`; `''` and `'/'` normalize to empty (an empty prefix means no
            // prefix — skip the rewrite entirely, but still strip the sentinel below).
            let prefix = prefix.trim_matches('/');
            if !prefix.is_empty() {
                for p in io_provides.iter_mut() {
                    if p.kind == "http" {
                        p.key = prepend_global_prefix(&p.key, prefix);
                    }
                }
            }
        }
        _ => {
            warnings.push(format!(
                "multiple setGlobalPrefix values found: [{}]; skipping global-prefix rewrite",
                prefixes.join(", ")
            ));
        }
    }

    io_provides.retain(|p| p.kind != SENTINEL_KIND);
}

/// Prepends a global-route prefix (already leading-slash-stripped by the caller) onto an `http` provide
/// key of the shape `"VERB /path"`, producing exactly one `/` at the seam: `("GET /articles", "api")` ->
/// `"GET /api/articles"`; `("GET /", "api")` -> `"GET /api"`. A key with no space (never produced by
/// `http_interface_key`, but handled defensively) is returned unchanged.
pub(super) fn prepend_global_prefix(key: &str, prefix: &str) -> String {
    let Some((verb, path)) = key.split_once(' ') else {
        return key.to_string();
    };
    let rest = path.strip_prefix('/').unwrap_or(path);
    if rest.is_empty() {
        format!("{verb} /{prefix}")
    } else {
        format!("{verb} /{prefix}/{rest}")
    }
}

#[cfg(test)]
mod global_prefix_tests {
    //! Coverage for `apply_and_strip_global_prefix`: the single-prefix rewrite (with and without a
    //! leading slash in source), the no-marker no-op, and the multiple-distinct-values honest degrade.
    use super::*;

    fn http_provide(key: &str, file: &str) -> IoProvide {
        IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
        }
    }

    fn prefix_marker(key: &str) -> IoProvide {
        IoProvide {
            body: None,
            kind: zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND.to_string(),
            key: key.to_string(),
            file: "main.ts".to_string(),
            line: 1,
            symbol: None,
        }
    }

    #[test]
    fn single_prefix_rewrites_http_provides_and_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /api/articles");
        assert!(warnings.is_empty());
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn no_marker_leaves_http_provides_unchanged() {
        let mut provides = vec![http_provide("GET /articles", "articles.controller.ts")];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
        assert!(warnings.is_empty());
    }

    #[test]
    fn leading_slash_in_source_still_yields_exactly_one_slash_at_the_seam() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("/api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/articles");
    }

    #[test]
    fn root_path_prefix_collapses_onto_the_prefix_alone() {
        let mut provides = vec![
            http_provide("GET /", "app.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api");
    }

    #[test]
    fn multiple_distinct_prefixes_skip_the_rewrite_and_warn() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
            prefix_marker("v2"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles"); // unchanged — never guess
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("multiple setGlobalPrefix values found"));
        assert!(warnings[0].contains("api"));
        assert!(warnings[0].contains("v2"));
    }

    #[test]
    fn only_http_kind_provides_are_rewritten() {
        // A non-"http" provide (e.g. "trpc") must not be touched by the rewrite.
        let mut provides = vec![
            IoProvide {
                body: None,
                kind: "trpc".to_string(),
                key: "GET /articles".to_string(),
                file: "t.ts".to_string(),
                line: 1,
                symbol: None,
            },
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
    }

    #[test]
    fn trailing_slash_prefix_yields_exactly_one_slash_at_the_seam() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api/"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/articles");
    }

    #[test]
    fn empty_string_prefix_is_a_no_op_but_still_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker(""),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles"); // unchanged — empty prefix means no prefix
        assert!(warnings.is_empty());
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn bare_slash_prefix_is_a_no_op_but_still_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("/"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn multi_segment_prefix_is_prepended_whole() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api/v1"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/v1/articles");
    }

    #[test]
    fn param_placeholder_in_the_path_is_preserved_across_the_rewrite() {
        let mut provides = vec![
            http_provide("DELETE /articles/{}", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "DELETE /api/articles/{}");
    }

    #[test]
    fn only_provides_present_at_the_seam_get_prefixed() {
        // Scope guard for the moved seam: whatever `http` provides are present when this runs get
        // prefixed; a provide APPENDED afterwards (e.g. a Java/Hono/file-route provide, which in the
        // real pipeline is added only after this call) is untouched because it isn't in the vec yet.
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        // Simulate a later producer appending an already-complete route path.
        provides.push(http_provide("GET /api/foo", "pages/api/foo.ts"));
        assert_eq!(provides.len(), 2);
        assert_eq!(provides[0].key, "GET /api/articles"); // was present -> prefixed
        assert_eq!(provides[1].key, "GET /api/foo"); // appended after -> NOT double-prefixed
    }

    // --- prepend_global_prefix unit coverage (the seam join, prefix already normalized) ---

    #[test]
    fn prepend_produces_one_clean_slash_and_preserves_verb_and_params() {
        assert_eq!(
            prepend_global_prefix("GET /articles", "api"),
            "GET /api/articles"
        );
        assert_eq!(prepend_global_prefix("GET /", "api"), "GET /api");
        assert_eq!(
            prepend_global_prefix("DELETE /articles/{}", "api/v1"),
            "DELETE /api/v1/articles/{}"
        );
    }
}
