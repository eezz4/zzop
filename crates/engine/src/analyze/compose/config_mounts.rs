use zzop_core::IoProvide;

use super::global_prefix::prepend_global_prefix;

/// Deployment-topology mount apply (`EngineConfig::mounts`) — prepends the winning mount's `at` onto
/// every `http` provide's key, keyed by which tree-relative directory the provide's `file` falls under.
/// Config-declared facts: the outermost gateway layer, applied ON TOP of whatever code-extracted prefix
/// (Nest `setGlobalPrefix`, a router mount, ...) already rewrote that provide's key, since a deployment
/// gateway lives outside the app itself — reuses [`prepend_global_prefix`] for the actual rewrite, same
/// normalization (`trim_matches('/')`) [`apply_and_strip_global_prefix`] uses.
///
/// ## Winner selection
/// Per provide, the winning mount is the entry whose `dir` is the LONGEST match: empty `dir` matches
/// every file; a non-empty `dir` matches when `provide.file == dir` or `provide.file` starts with
/// `"{dir}/"`. Exactly one winner is applied — mounts never stack on top of EACH OTHER (a user double-
/// mounting the same subtree is a config concern the zero-effect tripwire below and prefix-drift
/// disclosure surface, not something this function silently resolves by stacking). A tie on equal `dir`
/// length (a literally duplicated `dir`) resolves to the FIRST entry in `mounts`, deterministically.
///
/// ## Defensive validation (belt and braces — the mapper hard-fails on these too, in a later step)
/// An entry is skipped (never applied, never counted as a winner candidate) with one `warnings` entry
/// naming it when: `at` is empty after trimming surrounding `/`, or contains a scheme separator
/// (`"://"`), a path-param placeholder (`"{}"`), or any whitespace; or `dir` starts with `/` (must be
/// tree-relative) or contains a backslash (must use forward slashes).
///
/// ## Zero-effect tripwire
/// After every provide has been considered, any VALID entry that rewrote zero provides gets its own
/// `warnings` entry — a silent no-op here is exactly the kind of drift this codebase's disclosure contract
/// exists to surface. Two distinct situations both land on `hits == 0`, and are told apart by whether the
/// entry's `dir` matched ANY provide at all (regardless of who won that match):
/// - **stale/wrong-dir/no-http-provides** — the entry's `dir` matched 0 provides by path. Could be a stale
///   mount, a wrong `dir`, or a tree that emits no http provides at all.
/// - **shadowed** — the entry's `dir` matched >=1 provide, but every one of those matches was won by a
///   DIFFERENT, longer-`dir` (or equal-`dir`, earlier) entry (see "Winner selection" above). The entry
///   itself is redundant, not stale — a different message names this so the reader isn't told three false
///   causes ("stale mount, wrong dir, or the tree emits no http provides") when the real cause is none of
///   those.
///
/// ## Placement (load-bearing — see `zzop_engine::analyze::mod`'s call site)
/// Must run LAST among provide transforms: after every provide producer (controller-prefix,
/// global-prefix, tRPC, router-mount, Java, file-convention routes) has finished pushing into
/// `io_provides`, so a config mount covers ALL http provides regardless of which producer emitted them —
/// unlike `apply_and_strip_global_prefix`, which is deliberately scoped early to Nest-controller-only
/// provides (see that function's own placement doc), a deployment gateway sits in front of the WHOLE
/// tree's surface, so its rewrite must see everything.
pub(crate) fn apply_config_mounts(
    io_provides: &mut [IoProvide],
    mounts: &[crate::MountRule],
    warnings: &mut Vec<String>,
) {
    if mounts.is_empty() {
        return;
    }

    struct ValidMount<'a> {
        dir: &'a str,
        at: &'a str,
        hits: usize,
        /// Count of provides whose `dir` matched this entry, WHETHER OR NOT this entry won that match —
        /// distinguishes "0 hits because nothing matched this dir" (stale/wrong-dir) from "0 hits because
        /// every match was won by a more specific entry" (shadowed) in the zero-effect tripwire below.
        matched: usize,
    }
    let mut valid: Vec<ValidMount> = Vec::new();
    for m in mounts {
        let at_trimmed = m.at.trim_matches('/');
        if at_trimmed.is_empty()
            || m.at.contains("://")
            || m.at.contains("{}")
            || m.at.chars().any(char::is_whitespace)
        {
            warnings.push(format!(
                "topology mount at \"{}\" (dir \"{}\") is not usable — empty after trimming, or carries a scheme, a path-param placeholder, or whitespace: skipped",
                m.at, m.dir
            ));
            continue;
        }
        if m.dir.starts_with('/') || m.dir.contains('\\') {
            warnings.push(format!(
                "topology mount at \"{}\" (dir \"{}\") has an invalid dir — must be tree-relative with forward slashes: skipped",
                m.at, m.dir
            ));
            continue;
        }
        valid.push(ValidMount {
            dir: m.dir.as_str(),
            at: at_trimmed,
            hits: 0,
            matched: 0,
        });
    }
    if valid.is_empty() {
        return;
    }

    for p in io_provides.iter_mut() {
        if p.kind != "http" {
            continue;
        }
        let mut winner: Option<usize> = None;
        let mut matched_indices: Vec<usize> = Vec::new();
        for (i, v) in valid.iter().enumerate() {
            let matches =
                v.dir.is_empty() || p.file == v.dir || p.file.starts_with(&format!("{}/", v.dir));
            if !matches {
                continue;
            }
            matched_indices.push(i);
            match winner {
                None => winner = Some(i),
                Some(w) if v.dir.len() > valid[w].dir.len() => winner = Some(i),
                Some(_) => {} // tie on equal dir length -> first entry (already selected) wins
            }
        }
        for i in matched_indices {
            valid[i].matched += 1;
        }
        if let Some(i) = winner {
            p.key = prepend_global_prefix(&p.key, valid[i].at);
            valid[i].hits += 1;
        }
    }

    for v in &valid {
        if v.hits > 0 {
            continue;
        }
        if v.matched == 0 {
            warnings.push(format!(
                "topology mount \"{}\" (dir \"{}\") had no effect: 0 http provides matched — stale mount, wrong dir, or the tree emits no http provides",
                v.at, v.dir
            ));
        } else {
            warnings.push(format!(
                "topology mount \"{}\" (dir \"{}\") had no effect: every file it matched is claimed by a more specific mount (longer dir) — this entry is redundant",
                v.at, v.dir
            ));
        }
    }
}
