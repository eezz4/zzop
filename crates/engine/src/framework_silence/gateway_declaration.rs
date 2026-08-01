//! S12 — the unread gateway-declaration self-report.
//!
//! A route key is only as good as the path the deployment actually serves. When a gateway rewrites,
//! prefixes or proxies in front of the app, the path in the SOURCE is not the path a caller uses — and
//! zzop reads none of the files that declare those rewrites (`vercel.json`, `next.config.*`, an nginx
//! conf). The consequence is not a missing feature but a WRONG KEY: the provide side is keyed
//! pre-rewrite while the consume side calls the post-rewrite path, so the two do not join and the run
//! reports an unprovided consume that is actually served.
//!
//! Manual injection already covers the capability (`mountedAt` / `mounts` / `hosts` on the request), so
//! this is not "zzop cannot do it" — it is "zzop will not notice you never told it". That gap is
//! invisible precisely because it produces a plausible finding rather than an absence, which is why it
//! needs a run-level line rather than a per-finding footnote.
//!
//! CONTENT-GATED, not name-gated: a `vercel.json` with no rewrite construct in it declares nothing that
//! could move a key, and warning on its mere presence would train the reader to skip this line. The
//! tree must also carry http provides — with no routes there is nothing a rewrite could mis-key.

use std::path::Path;

/// Declaration files that can move a route path, each with the token that proves this one actually
/// does. `(filename-or-suffix, marker, human name)`.
///
/// The marker is what keeps this from being a filename census: `next.config.js` is present in most
/// Next projects and only a minority declare `rewrites`/`basePath`. Matching the token is a lexical
/// test on purpose — reading these files properly is the FEATURE this warning exists to say is absent,
/// and doing it half-way here would be the worst of both.
const GATEWAY_DECLARATIONS: &[(&str, &[&str], &str)] = &[
    (
        "vercel.json",
        &["\"rewrites\"", "\"routes\""],
        "Vercel rewrites",
    ),
    (
        "next.config.js",
        &["rewrites", "basePath"],
        "Next.js rewrites/basePath",
    ),
    (
        "next.config.mjs",
        &["rewrites", "basePath"],
        "Next.js rewrites/basePath",
    ),
    (
        "next.config.ts",
        &["rewrites", "basePath"],
        "Next.js rewrites/basePath",
    ),
    (
        "next.config.cjs",
        &["rewrites", "basePath"],
        "Next.js rewrites/basePath",
    ),
    (
        "nginx.conf",
        &["proxy_pass", "rewrite "],
        "nginx proxy/rewrite",
    ),
];

/// One warning when this tree declares a gateway rewrite zzop does not read AND has routes whose keys
/// that rewrite could move. `None` otherwise — including for a tree with the file but no rewrite
/// construct in it, and for a tree with no http provides at all.
pub fn gateway_declaration_warning(root: &Path, http_provide_count: usize) -> Option<String> {
    if http_provide_count == 0 {
        return None;
    }
    let mut found: Vec<String> = Vec::new();
    for (name, markers, label) in GATEWAY_DECLARATIONS {
        let path = root.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(hit) = markers.iter().find(|m| text.contains(**m)) {
            let entry = format!("{name} (declares {label}, matched `{hit}`)");
            if !found.contains(&entry) {
                found.push(entry);
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    Some(format!(
        "Unread gateway declaration(s): {} — zzop does not read these files, so this tree's {} http \
         route key(s) are keyed as the SOURCE spells them, not as the deployment serves them. If a \
         rewrite moves a path, the provide side and the consume side key differently and the join \
         reports an unprovided consume for a route that is actually served — a plausible finding \
         rather than an absence, which is why this is said here rather than left to be noticed. \
         Declare the effective prefix yourself with `trees[].topology.mountedAt` / `.mounts` / \
         `.hosts` in your config; the capability exists, the automatic reading of these declarations \
         does not. (If you already declared it, this line is still worth a look: it reports an \
         UNREAD declaration file, not a missing config — the two can both be true.)",
        found.join(", "),
        http_provide_count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework_silence::tests::TempDir;

    #[test]
    fn a_vercel_rewrite_with_routes_is_reported() {
        let dir = TempDir::new("zzop-gateway-vercel");
        dir.write(
            "vercel.json",
            "{\"rewrites\":[{\"source\":\"/api/:p*\",\"destination\":\"/be/:p*\"}]}\n",
        );
        let w = gateway_declaration_warning(dir.path(), 3).expect("warning");
        assert!(w.contains("vercel.json"), "{w}");
        assert!(
            w.contains("3 http route"),
            "names how many keys are at stake: {w}"
        );
        // The remedy must name the knob that already exists — otherwise this reads as an unfixable gap.
        assert!(w.contains("mountedAt"), "{w}");
    }

    /// Name-gating would fire on most Next projects, nearly all of which declare no rewrite. A warning
    /// that is usually noise is a warning nobody reads, which costs more than the gap it names.
    #[test]
    fn the_file_alone_is_not_enough_without_a_rewrite_construct() {
        let dir = TempDir::new("zzop-gateway-plain");
        dir.write("vercel.json", "{\"framework\":\"nextjs\"}\n");
        assert!(gateway_declaration_warning(dir.path(), 3).is_none());
        dir.write(
            "next.config.js",
            "module.exports = { reactStrictMode: true };\n",
        );
        assert!(gateway_declaration_warning(dir.path(), 3).is_none());
    }

    /// With no routes there is no key a rewrite could move, so the line would name a hazard that cannot
    /// apply to this tree.
    #[test]
    fn a_tree_with_no_routes_is_silent_even_with_a_rewrite() {
        let dir = TempDir::new("zzop-gateway-noroutes");
        dir.write("vercel.json", "{\"rewrites\":[]}\n");
        assert!(gateway_declaration_warning(dir.path(), 0).is_none());
    }

    #[test]
    fn a_tree_with_none_of_the_files_is_silent() {
        let dir = TempDir::new("zzop-gateway-none");
        dir.write("package.json", "{}\n");
        assert!(gateway_declaration_warning(dir.path(), 5).is_none());
    }
}
