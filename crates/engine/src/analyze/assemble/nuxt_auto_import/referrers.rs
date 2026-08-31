//! WHICH files may vouch for an auto-import target, and for WHICH app — the referrer side of the
//! scope wall, and the token roster a vouching file contributes.
//!
//! Two exclusions live here, and both are framework facts rather than heuristics: `<app>/public/` is
//! SERVED VERBATIM (the auto-import transform never runs over it, so no token in it can be a call
//! site), and a pre-scan host that is not a Nuxt component ([`NUXT_TOKEN_ROSTER_EXTS`]) is not a place
//! a Nuxt auto-import resolves either. See the parent module's doc for what each one measured.

use std::collections::BTreeSet;

/// The directory Nuxt copies to the site root UNTRANSFORMED. A minified vendored bundle living there
/// is inside the app dir, so the app-dir wall alone does not exclude it — measured on nocodb,
/// `packages/nc-gui/public/js/swagger-ui-bundle.min.js` is 1.06 MB and 8898 distinct tokens, two of
/// which (`convert`, `deepClone`) are export names of files this module resolves.
const NUXT_PUBLIC_DIR: &str = "public";

/// Pre-scan hosts whose WHOLE-TEXT token roster is allowed to vouch. Only Nuxt's own component
/// dialect qualifies: `.svelte`/`.astro` inside a Nuxt app are not Nuxt modules at all, and a
/// `.md`/`.mdx` page under Nuxt Content compiles only its `<script setup>` block — its prose is not a
/// call site, and this scan cannot tell the two apart. Measured on nocodb's 7 `.md` files: they vouch
/// for 0 of the 189 resolved subjects, so dropping them costs nothing there and removes a class of
/// voucher that could only ever be wrong.
const NUXT_TOKEN_ROSTER_EXTS: &[&str] = &["vue"];

/// Is `rel` inside `app_dir` (or is `app_dir` the analysis root)?
pub(crate) fn in_app_dir(rel: &str, app_dir: &str) -> bool {
    app_dir.is_empty()
        || (rel.starts_with(app_dir) && rel.as_bytes().get(app_dir.len()) == Some(&b'/'))
}

/// The app that COMPILES `rel`: the DEEPEST app dir containing it. Deepest rather than any, because a
/// nested app is the one whose auto-import table the file is actually built against, and a bare name
/// resolves against exactly one table.
pub(crate) fn owning_app_dir<'a>(rel: &str, app_dirs: &'a [String]) -> Option<&'a str> {
    app_dirs
        .iter()
        .filter(|d| in_app_dir(rel, d))
        .max_by_key(|d| d.len())
        .map(String::as_str)
}

/// The app whose auto-import table a bare name in `rel` could resolve against — `None` when nothing
/// `rel` contains can be a call site at all. That is the whole referrer-side wall: a file outside every
/// app dir, and a file under an app's own `public/`, vouch for nothing.
pub(crate) fn referrer_app_dir<'a>(rel: &str, app_dirs: &'a [String]) -> Option<&'a str> {
    let app = owning_app_dir(rel, app_dirs)?;
    let rest = if app.is_empty() {
        rel
    } else {
        rel.get(app.len() + 1..)?
    };
    let head = rest.split_once('/').map(|(h, _)| h);
    (head != Some(NUXT_PUBLIC_DIR)).then_some(app)
}

/// Is `rel` a pre-scan host whose whole-text token roster vouches? Case-insensitive, mirroring
/// `zzop_parser_typescript::prescan_mode`'s own extension normalization.
pub(crate) fn is_token_roster_host(rel: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, ext)| NUXT_TOKEN_ROSTER_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// The identifier tokens in `text`, word-boundary split — the reference roster for a file no structural
/// parser reads. `$` and `_` are identifier characters in JavaScript, and a token that starts with a
/// digit is dropped (a number, never a name).
pub(crate) fn identifier_tokens(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            cur.push(ch);
        } else if !cur.is_empty() {
            take_token(&mut cur, &mut out);
        }
    }
    take_token(&mut cur, &mut out);
    out
}

fn take_token(cur: &mut String, out: &mut BTreeSet<String>) {
    if !cur.is_empty() {
        if !cur.starts_with(|c: char| c.is_ascii_digit()) {
            out.insert(std::mem::take(cur));
        } else {
            cur.clear();
        }
    }
}
