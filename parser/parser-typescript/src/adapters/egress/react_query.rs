//! react-query query-key-as-URL recognition (`react-query-querykey-v1`) — `useQuery(<url-key>, …)` in a
//! file that imports `@tanstack/react-query` (or the legacy `react-query`). The near-universal RealWorld
//! idiom sets a `defaultQueryFn` that forwards `queryKey[0]` to `axios.get(...)`, so a query hook whose
//! key is a `/`-leading path literal IS a GET to that path. Three evidence gates keep this from guessing:
//!
//! - **Import gate** — the file must import from a react-query package (like the Angular recognizer's
//!   `@angular/common/http` gate); no react-query import → nothing recognized.
//! - **Read-only verb** — only the QUERY hooks (`useQuery`/`useInfiniteQuery`/`useSuspenseQuery`) are
//!   matched, and every one is a READ, so the verb is unambiguously `GET` (a write goes through
//!   `useMutation`, whose explicit `mutationFn` axios call is already extracted by `match_http_call`).
//! - **`/`-leading key shape** — only a query key that is (or, for `useQuery([key, …])`, whose first
//!   element is) a `/`-leading string/template literal is treated as a URL; any other key shape (a bare
//!   label `['todos']`, a computed value) is left alone. A `/`-leading key whose file's queryFn does NOT
//!   forward it as a URL almost always yields just one spurious UNPROVIDED consume (a disclosed non-join);
//!   only if that key text happens to equal a real backend provide path does it fabricate an edge — a
//!   narrow residual, since a `/`-leading key that IS a live endpoint path is the idiom this recognizes.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, Module, ModuleDecl, ModuleItem};

use super::body_shape::BodyStyle;
use super::matchers::HttpCall;
use super::unwrap_expr;

const QUERY_HOOKS: [&str; 3] = ["useQuery", "useInfiniteQuery", "useSuspenseQuery"];

/// True when the file imports (any specifier) from a react-query package — the per-file evidence gate for
/// [`match_react_query_call`] (`react-query-querykey-v1`), mirroring the Angular recognizer's import gate.
pub(super) fn imports_react_query(module: &Module) -> bool {
    module.body.iter().any(|item| {
        matches!(
            item,
            ModuleItem::ModuleDecl(ModuleDecl::Import(imp))
                if matches!(imp.src.value.as_str(), Some("@tanstack/react-query") | Some("react-query"))
        )
    })
}

/// `useQuery(<url-key>, …)` matcher (`react-query-querykey-v1`). Only fires when `enabled` (the file's
/// import gate). The callee must be a bare query-hook identifier and the first argument must resolve to a
/// `/`-leading URL key — either directly or as the first element of a `[key, …]` array. Emits a `GET`
/// (see module doc for why the verb is unambiguous).
pub(super) fn match_react_query_call(call: &CallExpr, enabled: bool) -> Option<HttpCall<'_>> {
    if !enabled {
        return None;
    }
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = &**callee else {
        return None;
    };
    if !QUERY_HOOKS.contains(&&*id.sym) {
        return None;
    }
    // The query key is arg 0, or the first element of an `[key, params]` array key.
    let arg0 = unwrap_expr(&call.args.first()?.expr);
    let key_expr = match arg0 {
        Expr::Array(arr) => unwrap_expr(&arr.elems.first()?.as_ref()?.expr),
        other => other,
    };
    if !is_slash_leading_url(key_expr) {
        return None;
    }
    Some(HttpCall {
        methods: vec!["GET".to_string()],
        arg: key_expr,
        body_style: BodyStyle::DirectArg, // GET is not a body-position verb -> no body witnessed anyway.
        // The underlying transport is the `defaultQueryFn`'s http client — in the queryKey-as-URL idiom
        // that is `axios.get(queryKey[0])`, so tag `axios` and let the shared `axios.defaults.baseURL`
        // seam prefix these exactly like a direct `axios.get`. (A file with no `axios.defaults.baseURL`
        // set has no prefix to apply, so the tag is harmless there.)
        client: "axios",
    })
}

/// Whether `e` is a URL-shaped string or template literal — the URL-shape gate. A plain STRING must be
/// `/`-leading (`'/user'`) to distinguish it from a bare label key (`'todos'`). A TEMPLATE counts when ANY
/// of its quasis contains a `/` — a `/`-bearing interpolated template (`` `/profiles/${u}` `` OR the
/// relative `` `articles/${slug}/comments/${id}` ``) is unambiguously a path (a react-query label key is
/// never a slash-bearing template), so a leading slash is not required there.
fn is_slash_leading_url(e: &Expr) -> bool {
    match e {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().unwrap_or_default().starts_with('/'),
        Expr::Tpl(t) => t
            .quasis
            .iter()
            .filter_map(|q| q.cooked.as_ref().and_then(|a| a.as_str()))
            .any(|s| s.contains('/')),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{clients, extract_http_egress, files, keys};

    const RQ: &str = "import { useQuery } from '@tanstack/react-query';\n";

    #[test]
    fn use_query_with_a_string_url_key_is_a_get() {
        let out = extract_http_egress(&files(&[(
            "useUserQuery.js",
            &format!("{RQ}export const u = () => useQuery('/user');"),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /user".to_string())]);
        // Tagged `axios` (the underlying transport) so the axios.defaults.baseURL seam prefixes it.
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn use_query_with_a_template_url_key_normalizes() {
        let out = extract_http_egress(&files(&[(
            "q.js",
            &format!("{RQ}const f = (u) => useQuery(`/profiles/${{u}}`);"),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /profiles/{}".to_string())]);
    }

    #[test]
    fn use_query_with_a_relative_path_template_key_is_recognized() {
        // A `/`-bearing template with no LEADING slash (`articles/${slug}/comments/${id}`) is still a URL.
        let out = extract_http_egress(&files(&[(
            "q.js",
            &format!(
                "{RQ}const f = (slug, id) => useQuery(`articles/${{slug}}/comments/${{id}}`);"
            ),
        )]));
        // Keying normalizes the missing leading slash, same as a relative `axios.get('articles')`.
        assert_eq!(
            keys(&out),
            vec![Some("GET /articles/{}/comments/{}".to_string())]
        );
    }

    #[test]
    fn use_query_with_an_array_key_reads_the_first_element_url() {
        let out = extract_http_egress(&files(&[(
            "q.js",
            &format!("{RQ}const f = () => useQuery(['/articles', {{ limit: 10 }}]);"),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /articles".to_string())]);
    }

    #[test]
    fn use_query_without_the_import_is_not_recognized() {
        let out = extract_http_egress(&files(&[(
            "q.js",
            "export const u = () => useQuery('/user');",
        )]));
        assert!(out.is_empty());
    }

    #[test]
    fn use_query_with_a_non_url_label_key_is_left_alone() {
        let out = extract_http_egress(&files(&[(
            "q.js",
            &format!("{RQ}const f = () => useQuery(['todos', id]);"),
        )]));
        assert!(out.is_empty());
    }
}
