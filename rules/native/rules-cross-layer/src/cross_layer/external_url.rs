//! External-consume key decomposition (`"METHOD scheme://host[/path][?query]"`) — the shared reader
//! every `external_*` rule uses. Moved verbatim from the module root (`cross_layer/mod.rs`) purely for
//! file-size layout; re-exported there so every existing `super::split_external_key` import still resolves.

/// An `external_consumes`-bucket consume key decomposed: `"GET https://api.vendor.com:8443/v1/users?key=x"` ->
/// method `GET`, host `api.vendor.com:8443` (port kept — port drift IS config drift), path `/v1/users`
/// (leading slash kept; `"/"` when the URL has no path), query `Some("key=x")`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExternalUrl<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
}

/// Splits an external consume key (`"METHOD scheme://host[/path][?query]"`) into its parts, or `None` when
/// the key doesn't carry the host-marker shape the join's external gate keys on (defensive — every key the
/// `external_consumes` bucket holds does contain `"://"`).
pub(crate) fn split_external_key(key: &str) -> Option<ExternalUrl<'_>> {
    let (method, url) = key.split_once(' ')?;
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let (host, path_query) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if host.is_empty() {
        return None;
    }
    let (path, query) = match path_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_query, None),
    };
    Some(ExternalUrl {
        method,
        host,
        path,
        query,
    })
}
