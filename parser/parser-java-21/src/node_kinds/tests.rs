use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{PINNED_ANONYMOUS_KEYWORDS, PINNED_NODE_KINDS};

/// Every `.rs` file under this crate's `src/`, MINUS the pin list's own module (which spells all of
/// `PINNED_NODE_KINDS` and would make the reverse test below tautological).
fn crate_source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {dir:?}: {e}")) {
            let path = entry.expect("readable dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.ends_with("node_kinds.rs")
                && !path.ends_with(Path::new("node_kinds").join("tests.rs"))
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out.sort();
    out
}

/// `line` with every `child_by_field_name("<field>")` argument removed — see `kind_shaped_literals`.
fn strip_field_name_arguments(line: &str) -> String {
    // A `let`, not a `const`: this is a parse token, not a policy value (scripts/check-policy-census.sh).
    let marker = "child_by_field_name(\"";
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find(marker) {
        out.push_str(&rest[..i]);
        rest = &rest[i + marker.len()..];
        match rest.find('"') {
            Some(end) => rest = &rest[end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Every `"..."` literal in `text` that could be a tree-sitter kind name — the deliberately dumb parse
/// `packages/cli-bin/src/cli/help/tests.rs` uses, for the same reason: a dumb scan can only fail by
/// finding too little, and the caller asserts its own subject set is non-empty first. Comment lines are
/// skipped (a doc comment naming a kind is not a match), and only `[a-z_0-9]+`-shaped literals are
/// considered, which is the shape every grammar kind name has. `child_by_field_name("...")` arguments
/// are stripped first: a FIELD name is a different grammar vocabulary that happens to overlap the kind
/// names (`type`, `expression`, `superclass` are both), and this list is about kinds.
fn kind_shaped_literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let line = strip_field_name_arguments(line);
        let mut rest = line.as_str();
        while let Some(i) = rest.find('"') {
            rest = &rest[i + 1..];
            let Some(end) = rest.find('"') else { break };
            let lit = &rest[..end];
            rest = &rest[end + 1..];
            if !lit.is_empty()
                && lit
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                out.insert(lit.to_string());
            }
        }
    }
    out
}

/// The REVERSE direction, and the one that catches GROWTH: a kind this crate's code actually matches
/// on but that never reached `PINNED_NODE_KINDS`. The forward test below only asks whether the listed
/// kinds still exist, so a newly matched kind was invisible to it — the list could only ever shrink in
/// value. Subject set is the crate's own source text (there is no runtime registry of matched kinds),
/// filtered to literals the compiled grammar itself recognizes as NAMED kinds, so a plain string that
/// merely looks like one contributes nothing. ANONYMOUS keyword tokens are deliberately out of scope
/// here (`PINNED_ANONYMOUS_KEYWORDS`' own forward test covers those): every English word this crate
/// spells is a candidate anonymous token, so the same scan run with `named: false` reports package
/// names and annotation spellings as if they were grammar matches.
#[test]
fn every_grammar_node_kind_literal_in_this_crate_is_pinned() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let files = crate_source_files();
    assert!(
        files.len() > 5,
        "the source walk found {} file(s) — it has stopped seeing this crate, so this test would \
         vouch for nothing",
        files.len()
    );
    let mut used: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        for lit in kind_shaped_literals(&text) {
            if lang.id_for_node_kind(&lit, true) != 0 {
                used.insert(lit);
            }
        }
    }
    assert!(
        !used.is_empty(),
        "the scan found NO grammar kind literal anywhere in src/ — the parse has stopped matching, \
         so an empty result must be RED, never a silent pass"
    );
    let pinned: BTreeSet<&str> = PINNED_NODE_KINDS.iter().copied().collect();
    let unpinned: Vec<&String> = used
        .iter()
        .filter(|k| !pinned.contains(k.as_str()))
        .collect();
    assert!(
        unpinned.is_empty(),
        "these tree-sitter-java NAMED kinds are matched somewhere in src/ but missing from \
         PINNED_NODE_KINDS: {unpinned:?} — add them (or, if the literal is not a kind match at all, \
         say so where it is written)"
    );
}

/// A grammar upgrade that renames one of `PINNED_NODE_KINDS` fails HERE, loudly, instead of every
/// extractor that matches on the renamed kind silently returning nothing — crate root doc's tree-sitter
/// discipline.
#[test]
fn node_kinds_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    for kind in PINNED_NODE_KINDS {
        assert_ne!(
            lang.id_for_node_kind(kind, true),
            0,
            "node kind {kind:?} is no longer a named kind in tree-sitter-java — grammar upgrade broke a match"
        );
    }
}

/// Same guarantee, for the anonymous keyword tokens `util::has_modifier_keyword` compares against
/// (looked up with `named: false` — module doc).
#[test]
fn anonymous_keywords_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    for kw in PINNED_ANONYMOUS_KEYWORDS {
        assert_ne!(
            lang.id_for_node_kind(kw, false),
            0,
            "keyword token {kw:?} is no longer an anonymous token in tree-sitter-java — grammar upgrade broke a match"
        );
    }
}

/// `program` is the crate's implicit root assumption (`parse_tree`/every top-level walk starts from
/// `tree.root_node()`) — never matched by string comparison anywhere, but still worth a direct
/// grammar-shape sanity check here, mirroring `zzop_parser_go::node_kinds::tests::root_kind_is_source_file`.
#[test]
fn root_kind_is_program() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    assert_ne!(lang.id_for_node_kind("program", true), 0);
}
