//! Identifier-token collection from BE source — the substrate both usage-aware schema rules judge
//! against. Split out of `usage.rs` for the 300-line cap, along the seam already there: this module
//! COLLECTS tokens and knows nothing about models, while the parent CONSUMES them and knows nothing
//! about how a file is scanned. The two halves never needed to see each other's internals.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

macro_rules! lazy_re {
    ($f:ident, $p:expr) => {
        fn $f() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($p).unwrap())
        }
    };
}

lazy_re!(block_comment_re, r"(?s)/\*.*?\*/");
lazy_re!(line_comment_re, r"(?m)(^|[^:])//.*$");
lazy_re!(double_quote_re, r#""(?:\\.|[^"\\])*""#);
lazy_re!(single_quote_re, r"'(?:\\.|[^'\\])*'");
lazy_re!(template_re, r"`(?:\\.|[^`\\])*`");
lazy_re!(ident_re, r"[A-Za-z_$][A-Za-z0-9_$]*");

/// Comment/string-stripped identifier tokens referenced anywhere in one file's raw text — the direct
/// per-file substrate `zzop_engine`'s fused per-file pass now feeds into `SchemaUsage.identifier_counts`
/// (each file's set unioned tree-wide, then re-counted to presence — see that crate's `assemble`).
/// Replaces the removed `scan_field_usage`'s own `<root>/src` filesystem walk: same recognizer (plain
/// identifier tokens on comment/string-stripped text — common names like id/name appear everywhere, so
/// they're effectively never flagged dead, keeping false positives low at the cost of recall), just
/// invoked once per file instead of via a second full-tree walk. `rel` gates which files are worth
/// scanning at all (see [`is_field_usage_scan_file`]); an excluded file yields an empty set regardless of
/// `text`.
pub fn field_usage_tokens(rel: &str, text: &str) -> HashSet<String> {
    if !is_field_usage_scan_file(rel) {
        return HashSet::new();
    }
    let stripped = strip_comments_and_strings(text);
    ident_re()
        .find_iter(&stripped)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// The ONE list of extensions [`field_usage_tokens`] will scan at all — the whole evidence channel behind
/// `unreferenced-model-name`/`unreferenced-field-name`. `pub` and quoted (never re-spelled) by
/// [`crate::message::field_usage_sightline`], so the sightline the findings publish cannot drift from the
/// scan itself; the published pages are pinned against that same rendering.
///
/// POLICY VALUE, T2: also spelled by hand, in English prose, in `docs/rules/catalog.md` and
/// `site/rules.html` (a Markdown/HTML page cannot reference a Rust constant) — pinned by
/// `crate::message::tests::the_field_usage_sightline_is_identical_in_the_finding_and_the_published_docs`.
pub const FIELD_USAGE_SCAN_EXTENSIONS: &[&str] = &["ts", "tsx"];

/// `.ts`/`.tsx` only, excluding `.d.ts` declaration files — mirrors the removed `walk_ts_files`'s own
/// per-file filename filter. The old walk also hard-excluded `node_modules`/`dist`/`data` directories;
/// that exclusion isn't reproduced here since the fused per-file pass this now runs inside already skips
/// `node_modules`/`dist` under the DEFAULT `skip_dirs` (`EngineConfig`) — a subset of the old exclusions,
/// so under default config the fused pass covers every file the old `<root>/src` walk did plus more,
/// which only ADDS identifier evidence (the accepted tree-wide-widening deviation, see module doc) and
/// never adds a false unreferenced-field-name positive. Caveat: a MORE-aggressive custom `skip_dirs` could exclude a
/// source dir the old walk scanned, dropping "used" tokens and potentially surfacing a false unreferenced-field-name —
/// acceptable, since a user who scopes analysis away from a directory is opting out of its evidence.
fn is_field_usage_scan_file(rel: &str) -> bool {
    if rel.ends_with(".d.ts") {
        return false;
    }
    FIELD_USAGE_SCAN_EXTENSIONS
        .iter()
        .any(|ext| rel.ends_with(&format!(".{ext}")))
}

fn strip_comments_and_strings(src: &str) -> String {
    let no_block = block_comment_re().replace_all(src, " ");
    let no_line = line_comment_re().replace_all(&no_block, "$1");
    let no_dq = double_quote_re().replace_all(&no_line, "\"\"");
    let no_sq = single_quote_re().replace_all(&no_dq, "''");
    template_re().replace_all(&no_sq, "``").into_owned()
}
