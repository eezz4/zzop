use std::collections::{BTreeMap, HashMap};

use zzop_core::IoConsume;

/// Axios `axios.defaults.baseURL` path-prefix apply + strip (`axios-defaults-base-v1`) — the
/// CONSUME-side counterpart of [`apply_and_strip_global_prefix`]: see
/// `zzop_parser_typescript::adapters::client_base`'s module doc for why this rides the `consumes`
/// channel as a `"client-base-prefix"` sentinel (mirrors the `"nest-global-prefix"` sentinel-string
/// convention that function's own doc describes) instead of a dedicated field.
///
/// ## Grouping
/// Sentinels are grouped by `client` (`"axios"` from `axios.defaults.baseURL`, `"generated"` from the
/// swagger `HttpClient.baseUrl` field — [`zzop_parser_typescript::extract_generated_client_base_prefix_marker`])
/// so one recognizer's base prefix can never cross-contaminate another's. Within one client group:
/// - Exactly one distinct sentinel path: every `http` consume tagged with that SAME `client` gets the
///   path prepended (see "Apply" below).
/// - 2+ distinct sentinel paths: nothing is applied for that client — ONE aggregated warning names the
///   client, every distinct path, and the declaring `file:line` of each sentinel (honest degrade over
///   guessing which one is real, same stance as [`apply_and_strip_global_prefix`]'s own multi-value
///   case).
/// - A sentinel with `key: None`, or a path that normalizes to empty/`"/"`: skipped defensively —
///   `extract_client_base_prefix_marker` never emits these per its own doc, but this seam does not rely
///   on that invariant holding.
///
/// ## Apply
/// A consume is rewritten only when ALL of: `kind == "http"`, `client` equals the resolved client, `key`
/// is `Some` (an unresolved consume is left exactly as unresolved — never guessed), and the key's path
/// (everything after the first space) starts with `/` and does not carry a scheme (`://` — an absolute
/// URL axios's `baseURL` never applies to). A matching key `"METHOD /path"` becomes
/// `"METHOD /<prefix>/path"` — deliberately prepended even when `/path` already starts with the prefix
/// (`"/api"` + `"/api/users"` -> `"/api/api/users"`), mirroring what the axios runtime actually does.
///
/// In every case every `"client-base-prefix"` sentinel is stripped from `io_consumes` unconditionally
/// (even when conflicting/unapplied) — it must never reach output, the linker, or rules.
///
/// ## Placement (load-bearing)
/// Must run AFTER [`late_resolve_cross_file_consumes`] — that pass fills `key` IN PLACE and preserves
/// the `client` tag, so a late-resolved axios consume still gets the prefix; this tag preservation is
/// the load-bearing ordering constraint. Sitting after [`resolve_wrapper_consumes`] is only "after the
/// last consume-mutating pass" hygiene: wrapper-emitted consumes carry `client: None` and are
/// DELIBERATELY never prefixed (custom wrappers stay uninterpreted — overlay territory). Must stay
/// BEFORE `io_consumes` is frozen into `MinimalIr::io` / read by any whole-tree rule
/// (`unprovided-consume`) or the cross-layer linker — see `zzop_engine::analyze::mod`'s call site.
pub(crate) fn apply_client_base_prefixes(
    io_consumes: &mut Vec<IoConsume>,
    warnings: &mut Vec<String>,
) {
    // Bound to the parser's exported const (not a local literal) so a rename on the emit side
    // cannot silently desynchronize the strip side — a leaked sentinel would reach output.
    const SENTINEL_KIND: &str = zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND;

    // client -> every sentinel naming it: (normalized path, file, line) — collected before any mutation
    // so the resolve step below can see every candidate for a client regardless of iteration order.
    let mut by_client: BTreeMap<String, Vec<(String, String, u32)>> = BTreeMap::new();
    for c in io_consumes.iter() {
        if c.kind != SENTINEL_KIND {
            continue;
        }
        let Some(client) = c.client.clone() else {
            continue; // defensive: the parser always tags a sentinel's client
        };
        let Some(key) = c.key.as_deref() else {
            continue; // defensive: the parser never emits a keyless sentinel
        };
        let path = key.trim_matches('/');
        if path.is_empty() {
            continue; // defensive: an empty/"/" base has no path part to prepend
        }
        by_client
            .entry(client)
            .or_default()
            .push((path.to_string(), c.file.clone(), c.line));
    }

    // Resolve each client group to exactly one applicable prefix, or none (never-guess on conflict).
    let mut prefixes: HashMap<String, String> = HashMap::new();
    for (client, entries) in &by_client {
        let mut distinct: Vec<&str> = entries.iter().map(|(p, _, _)| p.as_str()).collect();
        distinct.sort();
        distinct.dedup();
        match distinct.as_slice() {
            [] => {}
            [path] => {
                prefixes.insert(client.clone(), (*path).to_string());
            }
            _ => {
                let mut sorted_entries = entries.clone();
                sorted_entries.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.1.cmp(&b.1))
                        .then_with(|| a.2.cmp(&b.2))
                });
                let detail: Vec<String> = sorted_entries
                    .iter()
                    .map(|(p, f, l)| format!("/{p} ({f}:{l})"))
                    .collect();
                warnings.push(format!(
                    "multiple base-URL values found for client `{client}`: [{}]; skipping base-URL prefix rewrite",
                    detail.join(", ")
                ));
            }
        }
    }

    for c in io_consumes.iter_mut() {
        if c.kind != "http" {
            continue;
        }
        let Some(client) = c.client.as_deref() else {
            continue;
        };
        let Some(prefix) = prefixes.get(client) else {
            continue;
        };
        let Some(key) = c.key.as_deref() else {
            continue; // unresolved — never guessed
        };
        let Some((verb, path)) = key.split_once(' ') else {
            continue; // defensive: never produced by http_consume_interface_key
        };
        if !path.starts_with('/') || path.contains("://") {
            continue; // external/absolute-URL key — axios ignores baseURL for those
        }
        c.key = Some(format!("{verb} /{prefix}{path}"));
    }

    io_consumes.retain(|c| c.kind != SENTINEL_KIND);
}

#[cfg(test)]
mod tests;
