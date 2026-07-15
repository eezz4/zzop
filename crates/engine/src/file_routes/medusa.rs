//! Medusa-style file routing: `src/api/**/route.ts` where the directory path after `src/api` IS the
//! URL and `[param]` directories are path params (Medusa v2's convention; the same shape any
//! "file-router on src/api" userland app uses). Verbs come from the file's `export const GET/POST/…`
//! symbols — that part lives in `mod.rs`; this module only owns the path→URL transform.

use super::{convert_dynamic_segment, is_route_module_filename};

/// `src/api/admin/campaigns/[id]/route.ts` → `/admin/campaigns/{id}`. Returns `None` when `rel` is
/// not a route module under a `src/api` segment pair, or when a segment is a catch-all
/// (`[...x]`/`[[...x]]` — not statically routable, see the module-doc scope note in `mod.rs`).
pub(super) fn medusa_route(rel: &str) -> Option<String> {
    let segs: Vec<&str> = rel.split('/').collect();
    let (file, dirs) = segs.split_last()?;
    if !is_route_module_filename(file) {
        return None;
    }
    let pair_at = dirs
        .windows(2)
        .position(|w| w[0] == "src" && w[1] == "api")?;
    let mut parts: Vec<String> = Vec::new();
    for seg in &dirs[pair_at + 2..] {
        parts.push(convert_dynamic_segment(seg)?);
    }
    Some(format!("/{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::medusa_route;

    #[test]
    fn maps_nested_params() {
        assert_eq!(
            medusa_route(
                "packages/medusa/src/api/admin/claims/[id]/claim-items/[action_id]/route.ts"
            ),
            Some("/admin/claims/{id}/claim-items/{action_id}".into())
        );
        assert_eq!(
            medusa_route("src/api/store/products/route.ts"),
            Some("/store/products".into())
        );
    }

    #[test]
    fn root_route_maps_to_slash() {
        assert_eq!(medusa_route("src/api/route.ts"), Some("/".into()));
    }

    #[test]
    fn rejects_non_route_modules_and_other_trees() {
        assert_eq!(medusa_route("src/api/admin/campaigns/middlewares.ts"), None);
        assert_eq!(medusa_route("src/api/admin/helpers.ts"), None);
        assert_eq!(medusa_route("packages/medusa/src/lib/route.ts"), None);
        assert_eq!(medusa_route("apps/web/app/api/cancel/route.ts"), None);
    }

    #[test]
    fn rejects_catch_all_segments() {
        assert_eq!(medusa_route("src/api/admin/[...rest]/route.ts"), None);
        assert_eq!(medusa_route("src/api/admin/[[...rest]]/route.ts"), None);
    }
}
