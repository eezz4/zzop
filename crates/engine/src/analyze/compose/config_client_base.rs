//! `trees[].topology.clientBase` apply — the CALLING side's mirror of `apply_config_mounts`.
//!
//! `mountedAt`/`mounts` declare where a tree is SERVED; this declares the prefix a tree's own outbound
//! calls carry. It exists because the engine refuses to guess a base it cannot read statically: a
//! `baseURL` assigned from a cross-file constant (`axios.defaults.baseURL = settings.baseApiUrl`) emits
//! nothing, so the calls key as `GET /articles` while the backend serves `GET /api/articles` and the join
//! finds nothing. Measured over the 17-tree dogfood corpus, that shape produced ZERO cross-source http
//! edges; `cross-layer/all-consumes-unjoined` is the disclosure that fires in the meantime, and this knob
//! is what its message points at.
//!
//! ## What is rewritten, and what deliberately is not
//! A consume is eligible only when ALL of: `kind == "http"`, `key` is `Some` (an unresolved consume stays
//! unresolved — never guessed), and the key's path starts with `/` without a scheme (`://` — an absolute
//! URL no client base applies to).
//!
//! Unlike the sentinel pass this is NOT scoped by `client`: a config declaration is the author's statement
//! about the whole tree, the same scope `mountedAt` has on the provide side.
//!
//! ## Idempotence — where this DIVERGES from the code-extracted pass, and why
//! [`super::apply_client_base_prefixes`] prepends unconditionally, including onto a path that already
//! starts with the prefix (`/api` + `/api/users` -> `/api/api/users`). That is correct there: it is
//! SIMULATING a runtime, and axios really does concatenate. This pass must not copy it. A config
//! declaration is not a simulation of anything — it is the author stating what the effective route is, and
//! an eligible consume already keyed under the base already HAS that effective route.
//!
//! The refutation is a shape, not a preference. The Next-style front end reaches its own API two ways from
//! one tree: a browser client carrying the base implicitly, and server-side rendering calling `fetch` with
//! the FULL path (no client instance is configured there). Both spellings live in the same tree, so the
//! knob's scope covers both, and a blind prepend would take every full-path call that joins today and
//! re-key it to `/api/api/...`. That trades the exact failure this knob exists to fix for the same failure
//! pointed the other way. Skipping a path already under the base costs only the case of a genuine
//! `/api/api/x` route — a route that is its own base twice — against breaking every correctly-spelled call
//! in a mixed tree.
//!
//! "Already under the base" means the path equals the base or continues it at a SEGMENT boundary: `/api`
//! covers `/api` and `/api/users`, never `/apiv2/users`.
//!
//! ## What this warns about
//! Idempotence turns the double-prefix case into a no-op, so the warning that matters is the declaration
//! that changed nothing: either there was nothing eligible to prefix at all, or every eligible consume
//! already carried the base — which, when the code ALSO declared a literal base the engine already
//! applied, means the declaration is a duplicate of something zzop could read for itself. A declaration
//! that moved SOME keys and skipped others is the normal mixed shape above and says nothing.

use zzop_core::IoConsume;

/// Prepends `client_base` to every keyed, relative `http` consume. `already_prefixed` is the set of
/// clients a code-extracted base was applied for in this tree (from [`super::apply_client_base_prefixes`]);
/// it drives the double-prefix warning only and never changes what is rewritten.
pub(crate) fn apply_config_client_base(
    io_consumes: &mut [IoConsume],
    client_base: Option<&str>,
    already_prefixed: &[String],
    warnings: &mut Vec<String>,
) {
    let Some(base) = client_base else { return };

    // Defensive backstop, mirroring `apply_config_mounts`: the config mapper is the fail-fast gate for
    // shape, but this crate is also a library whose embedders build a request by hand.
    if !base.starts_with('/') || base.contains("://") || base.contains("{}") {
        warnings.push(format!(
            "trees[].topology.clientBase {base:?} is not a usable path prefix (it must start with \"/\", \
             carry no scheme, and contain no \"{{}}\" placeholder) — no consume was rewritten."
        ));
        return;
    }
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        return;
    }

    let mut rewritten = 0usize;
    let mut already = 0usize;
    for c in io_consumes.iter_mut() {
        if c.kind != "http" {
            continue;
        }
        let Some(key) = c.key.as_ref() else { continue };
        let Some((method, path)) = key.split_once(' ') else {
            continue;
        };
        if !path.starts_with('/') || path.contains("://") {
            continue;
        }
        if path_is_under(path, base) {
            already += 1;
            continue;
        }
        c.key = Some(format!("{method} {base}{path}"));
        rewritten += 1;
    }

    if rewritten > 0 {
        return; // The knob did its job. A partial apply is the normal mixed-spelling shape.
    }

    // A declared knob that moved nothing is almost always stale, duplicated, or on the wrong tree, and
    // silence would let it look effective. Same zero-effect stance the declared-hosts tripwire takes.
    if already == 0 {
        warnings.push(format!(
            "trees[].topology.clientBase is declared as {base:?} but this tree has no keyed relative http \
             consume to prefix — the declaration had no effect. Either the calls are extracted as \
             unresolved (see this tree's coverage warnings), or they are absolute URLs, or the \
             declaration belongs on a different tree."
        ));
    } else {
        let source = if already_prefixed.is_empty() {
            "the call sites already write the base themselves".to_string()
        } else {
            format!(
                "zzop already read that base from the code for client(s) {} and applied it",
                already_prefixed.join(", ")
            )
        };
        warnings.push(format!(
            "trees[].topology.clientBase {base:?} had no effect: all {already} keyed relative http \
             consume(s) in this tree already key under {base:?} — {source}. The declaration is a \
             duplicate and can be dropped."
        ));
    }
}

/// Is `path` already under `base` — equal to it, or continuing it at a SEGMENT boundary? `/api` covers
/// `/api` and `/api/users` but never `/apiv2/users`. `base` is pre-normalized to carry no trailing slash.
fn path_is_under(path: &str, base: &str) -> bool {
    path.strip_prefix(base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests;
