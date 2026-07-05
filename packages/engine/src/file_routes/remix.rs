//! Remix flat-routes (`remix-flat-routes` hybrid `+` convention) path→URL transform: dots are path
//! separators, `folder+/` flattens, `$param` is dynamic, `_`-prefixed tokens are pathless. The
//! resource-route gate (loader/action, no default export) lives in `mod.rs`; this module only owns
//! the filename→URL transform.

use super::has_segment_pair;

/// Splits a single path segment (a `+`-stripped directory, or the extension-stripped file stem)
/// into dot-separated tokens, honoring the `[x]` literal-unwrap escape: a bracketed run is never a
/// split point and its brackets are dropped, so `[.]` yields a literal `.` in the current token.
fn split_dot_tokens(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        match c {
            '.' => tokens.push(std::mem::take(&mut current)),
            '[' => {
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    current.push(inner);
                }
            }
            _ => current.push(c),
        }
    }
    tokens.push(current);
    tokens
}

/// Applies the per-token rules to one dot-token (`is_stem` gates the `route`/`index` folder-module
/// skip, which only applies to stem tokens, never a directory). Returns `Some(None)` for a token
/// that contributes nothing, `Some(Some(part))` when it contributes `part`, and `None` for a bare
/// `$` splat (not statically routable, propagated via `?`).
fn process_token(token: &str, is_stem: bool) -> Option<Option<String>> {
    if token == "_index" {
        return Some(None);
    }
    if token.starts_with('_') {
        return Some(None);
    }
    if token == "$" {
        return None;
    }
    if let Some(param) = token.strip_prefix('$') {
        return Some(Some(format!("{{{param}}}")));
    }
    if is_stem && (token == "route" || token == "index") {
        return Some(None);
    }
    if let Some(stripped) = token.strip_suffix('_') {
        return Some(Some(stripped.to_string()));
    }
    Some(Some(token.to_string()))
}

/// `apps/remix/app/routes/api+/branding.logo.team.$teamId.ts` → `/api/branding/logo/team/{teamId}`.
///
/// Gate: `rel` contains the adjacent segments `app/routes`. Segments after `routes` (directories,
/// trailing `+` dropped, and the extension-stripped stem) are split on `.` into tokens — e.g.
/// `t.$teamUrl+/documents.$id.edit.tsx` → `t`, `$teamUrl`, `documents`, `$id`, `edit`. `[.]` escapes
/// a literal dot (`sitemap[.]xml` → one token); `[x]` unwraps to the literal `x` otherwise.
///
/// Token rules: `_index` and other `_`-prefixed tokens contribute nothing (pathless); a trailing
/// `_` is stripped but the token kept (`authoring_` → `authoring`, opts out of layout nesting);
/// `$param` → `{param}`; a bare `$` (splat) makes the whole file return `None` (not statically
/// routable); a stem token of exactly `route`/`index` contributes nothing. A stem ending in
/// `.server`/`.client` → `None` (not a route module). Result: `/` + joined tokens, or `/` if none.
pub(super) fn remix_flat_route(rel: &str) -> Option<String> {
    if !has_segment_pair(rel, "app", "routes") {
        return None;
    }
    let segs: Vec<&str> = rel.split('/').collect();
    let routes_at = segs
        .windows(2)
        .position(|w| w[0] == "app" && w[1] == "routes")?;
    let after = &segs[routes_at + 2..];
    let (file, dirs) = after.split_last()?;
    let file: &str = file;

    let stem = match file.rfind('.') {
        Some(idx) => &file[..idx],
        None => file,
    };
    let stem_tokens = split_dot_tokens(stem);
    if let Some(last) = stem_tokens.last() {
        if last == "server" || last == "client" {
            return None;
        }
    }

    let mut parts: Vec<String> = Vec::new();
    for dir in dirs {
        let dir = dir.strip_suffix('+').unwrap_or(dir);
        for token in split_dot_tokens(dir) {
            if let Some(part) = process_token(&token, false)? {
                parts.push(part);
            }
        }
    }
    for token in stem_tokens {
        if let Some(part) = process_token(&token, true)? {
            parts.push(part);
        }
    }

    Some(format!("/{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::remix_flat_route;

    #[test]
    fn maps_flattened_api_resource_routes() {
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/api+/health.ts"),
            Some("/api/health".into())
        );
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/api+/stripe.webhook.ts"),
            Some("/api/stripe/webhook".into())
        );
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/api+/avatar.$id.tsx"),
            Some("/api/avatar/{id}".into())
        );
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/api+/branding.logo.team.$teamId.ts"),
            Some("/api/branding/logo/team/{teamId}".into())
        );
    }

    #[test]
    fn pathless_layout_prefix_contributes_nothing() {
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/_authenticated+/dashboard.tsx"),
            Some("/dashboard".into())
        );
        assert_eq!(
            remix_flat_route(
                "apps/remix/app/routes/_authenticated+/o.$orgUrl.settings.billing.tsx"
            ),
            Some("/o/{orgUrl}/settings/billing".into())
        );
    }

    #[test]
    fn nested_plus_folders_and_dotted_stem_combine() {
        assert_eq!(
            remix_flat_route(
                "apps/remix/app/routes/_authenticated+/t.$teamUrl+/documents.$id.edit.tsx"
            ),
            Some("/t/{teamUrl}/documents/{id}/edit".into())
        );
    }

    #[test]
    fn index_stem_under_pathless_layout_maps_to_layout_root() {
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/_authenticated+/settings+/_index.tsx"),
            Some("/settings".into())
        );
    }

    #[test]
    fn deeply_nested_plus_folders_with_dynamic_param() {
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/embed+/_v0+/sign.$token.tsx"),
            Some("/embed/sign/{token}".into())
        );
    }

    #[test]
    fn trailing_underscore_strips_but_keeps_the_segment() {
        assert_eq!(
            remix_flat_route("apps/remix/app/routes/embed+/v1+/authoring_.completed.create.tsx"),
            Some("/embed/v1/authoring/completed/create".into())
        );
    }

    #[test]
    fn bracket_escape_yields_literal_dot() {
        assert_eq!(
            remix_flat_route("app/routes/sitemap[.]xml.ts"),
            Some("/sitemap.xml".into())
        );
    }

    #[test]
    fn bare_splat_is_not_statically_routable() {
        assert_eq!(remix_flat_route("app/routes/$.tsx"), None);
    }

    #[test]
    fn server_module_qualifier_is_not_a_route() {
        assert_eq!(remix_flat_route("app/routes/foo.server.ts"), None);
    }

    #[test]
    fn root_index_maps_to_slash() {
        assert_eq!(remix_flat_route("app/routes/_index.tsx"), Some("/".into()));
    }

    #[test]
    fn rejects_paths_outside_app_routes() {
        assert_eq!(remix_flat_route("apps/web/app/api/cancel/route.ts"), None);
    }

    #[test]
    fn nested_folder_route_module_stem_contributes_nothing() {
        assert_eq!(
            remix_flat_route("app/routes/foo+/bar/route.tsx"),
            Some("/foo/bar".into())
        );
    }
}
