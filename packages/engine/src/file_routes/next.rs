//! Next.js file-routing path→URL transforms — both generations: legacy `pages/api/**` (the file
//! stem is the last URL segment) and app-router `app/**/route.ts` (the directory path is the URL).
//! Verb derivation lives in `mod.rs` (app-router: `export const GET/POST/…` symbols; pages/api:
//! `scan_pages_api_handler` over the re-read text); this module only owns the path mapping.

use super::{convert_dynamic_segment, has_segment_pair, is_route_module_filename};

/// `apps/web/pages/api/book/event.ts` → `/api/book/event`.
///
/// Gate: `rel` contains the adjacent segments `pages/api` and a ts|tsx|js|jsx|mjs|cjs extension.
/// Mapping: `/api/` + directory segments after the pair + the file stem (`index` contributes
/// nothing, e.g. `pages/api/foo/index.ts` → `/api/foo`). `[param]` → `{param}`; a catch-all
/// (`[...x]`/`[[...x]]`) anywhere → `None` (not statically routable).
pub(super) fn pages_api_route(rel: &str) -> Option<String> {
    if !has_segment_pair(rel, "pages", "api") {
        return None;
    }
    let segs: Vec<&str> = rel.split('/').collect();
    let (file, dirs) = segs.split_last()?;
    let (stem, ext) = file.rsplit_once('.')?;
    if !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") {
        return None;
    }
    let pair_at = dirs
        .windows(2)
        .position(|w| w[0] == "pages" && w[1] == "api")?;
    let mut parts: Vec<String> = Vec::new();
    for seg in &dirs[pair_at + 2..] {
        parts.push(convert_dynamic_segment(seg)?);
    }
    if stem != "index" {
        parts.push(convert_dynamic_segment(stem)?);
    }
    if parts.is_empty() {
        Some("/api".to_string())
    } else {
        Some(format!("/api/{}", parts.join("/")))
    }
}

/// `apps/web/app/api/avatar/[uuid]/route.ts` → `/api/avatar/{uuid}`.
///
/// Gate: filename is a route module and some directory segment is exactly `app` (first match;
/// supports `src/app/**`). Over the segments after `app`: route groups `(marketing)` and parallel
/// slots `@modal` are stripped; `[param]` → `{param}`; an interception segment (`(.)`/`(..)`/`(...)`),
/// a private segment (`_...`), or a catch-all (`[...x]`/`[[...x]]`) → `None`. Empty remainder → `/`.
pub(super) fn app_router_route(rel: &str) -> Option<String> {
    let segs: Vec<&str> = rel.split('/').collect();
    let (file, dirs) = segs.split_last()?;
    if !is_route_module_filename(file) {
        return None;
    }
    let app_at = dirs.iter().position(|&s| s == "app")?;
    let mut parts: Vec<String> = Vec::new();
    for seg in &dirs[app_at + 1..] {
        if seg.starts_with("(.)") || seg.starts_with("(..)") || seg.starts_with("(...)") {
            return None;
        }
        if seg.starts_with('(') && seg.ends_with(')') {
            continue;
        }
        if seg.starts_with('@') {
            continue;
        }
        if seg.starts_with('_') {
            return None;
        }
        parts.push(convert_dynamic_segment(seg)?);
    }
    Some(format!("/{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::{app_router_route, pages_api_route};

    #[test]
    fn pages_api_maps_plain_and_nested_paths() {
        assert_eq!(
            pages_api_route("apps/web/pages/api/book/event.ts"),
            Some("/api/book/event".into())
        );
        assert_eq!(
            pages_api_route("apps/web/pages/api/auth/verify-email.ts"),
            Some("/api/auth/verify-email".into())
        );
    }

    #[test]
    fn pages_api_index_stem_contributes_nothing() {
        assert_eq!(
            pages_api_route("pages/api/foo/index.ts"),
            Some("/api/foo".into())
        );
        assert_eq!(pages_api_route("pages/api/index.ts"), Some("/api".into()));
    }

    #[test]
    fn pages_api_param_stem_and_segment() {
        assert_eq!(
            pages_api_route("apps/web/pages/api/trpc/viewer/[trpc].ts"),
            Some("/api/trpc/viewer/{trpc}".into())
        );
    }

    #[test]
    fn pages_api_catch_all_is_none() {
        assert_eq!(
            pages_api_route("apps/web/pages/api/auth/[...nextauth].ts"),
            None
        );
        assert_eq!(pages_api_route("pages/api/[[...rest]].ts"), None);
    }

    #[test]
    fn pages_api_rejects_non_pages_paths_and_wrong_extension() {
        assert_eq!(pages_api_route("apps/web/app/api/cancel/route.ts"), None);
        assert_eq!(pages_api_route("pages/api/book/event.json"), None);
    }

    #[test]
    fn app_router_maps_plain_and_param_paths() {
        assert_eq!(
            app_router_route("apps/web/app/api/cancel/route.ts"),
            Some("/api/cancel".into())
        );
        assert_eq!(
            app_router_route("apps/web/app/api/avatar/[uuid]/route.ts"),
            Some("/api/avatar/{uuid}".into())
        );
        assert_eq!(
            app_router_route("apps/web/app/api/webhooks/calendar-subscription/[provider]/route.ts"),
            Some("/api/webhooks/calendar-subscription/{provider}".into())
        );
    }

    #[test]
    fn app_router_strips_route_groups_and_slots() {
        assert_eq!(
            app_router_route("app/(marketing)/api/x/route.ts"),
            Some("/api/x".into())
        );
        assert_eq!(
            app_router_route("app/@modal/api/x/route.ts"),
            Some("/api/x".into())
        );
    }

    #[test]
    fn app_router_rejects_private_interception_and_catch_all() {
        assert_eq!(app_router_route("app/api/_private/x/route.ts"), None);
        assert_eq!(app_router_route("app/api/[...rest]/route.ts"), None);
        assert_eq!(app_router_route("app/api/[[...rest]]/route.ts"), None);
        assert_eq!(app_router_route("app/(.)photo/route.ts"), None);
        assert_eq!(app_router_route("app/(..)photo/route.ts"), None);
        assert_eq!(app_router_route("app/(...)photo/route.ts"), None);
    }

    #[test]
    fn app_router_uses_first_app_segment_and_supports_src_layout() {
        assert_eq!(
            app_router_route("src/app/api/me/route.ts"),
            Some("/api/me".into())
        );
    }

    #[test]
    fn app_router_root_route_maps_to_slash() {
        assert_eq!(app_router_route("app/route.ts"), Some("/".into()));
    }

    #[test]
    fn app_router_rejects_missing_app_segment_and_non_route_filenames() {
        assert_eq!(app_router_route("src/api/x/route.ts"), None);
        assert_eq!(app_router_route("app/api/cancel/handler.ts"), None);
    }
}
