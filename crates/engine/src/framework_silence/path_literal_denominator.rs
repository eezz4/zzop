//! S19: the internal-path-literal DENOMINATOR (both io directions).
//!
//! ## What makes this one different from the tripwires before it
//! Every consume-side tripwire before it anchors on a VOCABULARY: a decorator shape (S1), a package
//! import (S2/S4/S6), a committed spec (S3), the `fetch(` token (S5), or a wrapper export name (S7).
//! Each one closes the gap the previous one's own doc names, and each one is a new entry in a list
//! that the next legacy stack extends again. Measured 2026-10-03 on a hand-built jQuery-era frontend
//! (`corpus/legacy/web`, 9 `.js` files): 22 real HTTP call sites, `ioConsumesKeyed` 1,
//! `ioConsumesUnresolved` 0, and NOT ONE tripwire in this build fired (no id range here on purpose --
//! `analyze::assemble::warnings` owns that lesson: a prose range goes stale the next time one lands).
//! jQuery arrives through a `<script>` tag, so
//! S4 has no import to anchor on; the tree holds exactly one builtin `fetch(`, under S5's
//! `FETCH_CALL_SITES_MIN`; the house wrapper sits under S7's floor. Three tripwires, three different
//! structural reasons, one silence.
//!
//! The deeper reading of that measurement is the reason this module exists: **an unrecognized call
//! shape does not become an unresolved fact, it produces no fact at all.** So every field that counts
//! what a run could not resolve — `ioConsumesUnresolved`, the unresolved-consume ratio rule, the
//! zero-extraction floor — is computed over facts that already exist, and none of them can see a loss
//! of 21 out of 22. The louder the silence, the quieter the disclosure.
//!
//! ## The needle: count the paths, not the libraries
//! This tripwire counts STRING LITERALS THAT LOOK LIKE AN INTERNAL PATH (`'/user/list.do'`), and names
//! no library anywhere. That is deliberate: a denominator must not share extraction's vocabulary, or
//! it inherits exactly the blindness it exists to measure.
//!
//! 📏 Why a literal census rather than a call-token census, measured on the same tree: a call-token
//! needle (`$.ajax|$.get|$.post|$.getJSON|jQuery.ajax|.open(|fetch(`) returns 33 — but 10 of those 33
//! sit inside COMMENTS (`/* 2. $.get */`), so the honest call-site count is 22. The path-literal census
//! returns 27 over the same files. ⚠ A hand-written grep used while designing this said 25: its character
//! class omitted `?` and `=`, so it dropped two real internal paths carrying query strings
//! (`'/user/export.do?fmt='`, `'/order/stat.do?from=2024-01-01'`). The shipped pattern is the correct one and
//! this number is read off the binary, not off that grep. The literal census is both closer to the truth and free
//! of the per-library enumeration, so it is what ships. A comment mentioning a path still counts, which
//! is the documented over-disclosure tolerance this whole module family accepts (see S5's file-family
//! doc): over-disclosure is safe, silence is fatal.
//!
//! ## Why the gate is BOTH directions
//! A leading-slash literal is a consume on a client (`$.get('/users')`) and a PROVIDE on a server
//! (`app.get('/users', …)`). This module cannot tell them apart and does not try. Gating on
//! [`IO_NEAR_ZERO_FLOOR`] in BOTH channels removes the conflation structurally rather than by
//! heuristic: a healthy server extracted its provides and never reaches here, a healthy client
//! extracted its consumes and never reaches here, and what survives is a tree that mentions internal
//! paths repeatedly while producing almost no io facts in either direction. That is the only
//! population the warning's wording has to be true for, and for that population "this tree talks about
//! internal paths far more often than it produced io facts" is true whichever direction they were.
//!
//! ## What it does NOT do
//! It does not widen extraction, name a framework, or change a verdict — it reports a ratio. The
//! vocabulary work it makes measurable (teaching the extractor the jQuery/XHR egress shapes) is a
//! separate, later step, and the ordering is deliberate: a denominator can be counted without the
//! vocabulary, but the vocabulary's progress cannot be read without the denominator.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::builtin_fetch::is_js_ts_family;
use super::committed_spec_io_silence::IO_NEAR_ZERO_FLOOR;

/// A string or template literal whose body opens with a single `/` and carries no quote of its own
/// kind — the lexical shape of an internal, origin-relative path. Bounded body length is a
/// pathological-input guard, not a policy: a 300-character "path" is not what this census is about.
///
/// Protocol-relative (`//cdn.example.com/x`) and absolute (`https://…`) URLs are excluded by
/// [`is_internal_path_literal`] rather than by this pattern, so the exclusion is one readable
/// predicate instead of a regex that has to express "starts with exactly one slash" three times.
fn path_literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"["'`]/[^"'`\n]{0,200}["'`]"#).unwrap())
}

/// Minimum internal-path literals in the tree before S19 speaks. A handful of path strings is the
/// normal furniture of any frontend (one asset path, one router entry) and says nothing; a tree that
/// writes two dozen of them while producing almost no io facts is the subject.
///
/// Policy tier T3 (coincidental axis, do not unify): counts LITERAL OCCURRENCES, a different axis
/// from S5's `FETCH_CALL_SITES_MIN` (call tokens) and from [`IO_NEAR_ZERO_FLOOR`] (extracted facts).
/// 📏 Set against the 2026-10-03 measurement: the subject tree carries 25, and the floor sits far
/// enough below that a tree with half its path surface still fires.
const PATH_LITERAL_SITES_MIN: usize = 8;

/// Sample cap for the example-file list — a presentation bound, same role and value as the sibling
/// modules' `MAX_SAMPLES` (T3: a display cap, not a firing threshold; free to diverge).
const MAX_SAMPLES: usize = 3;

/// True when a matched literal is an INTERNAL path: its body opens with exactly one `/`. A second
/// slash makes it protocol-relative (`//cdn…`), which is egress to another origin and nothing a
/// cross-layer join could use; a scheme (`https://…`) never reaches here because the match must open
/// with `/` immediately after the quote.
fn is_internal_path_literal(m: &str) -> bool {
    // m is `"…"` / `'…'` / `` `…` `` with the body starting at byte 1 and opening with `/`.
    !m.as_bytes().get(2).is_some_and(|&b| b == b'/')
}

/// Counts internal-path literals across the tree's js/ts-family sources, returning the total and up
/// to [`MAX_SAMPLES`] example files in the caller's (already sorted) order.
///
/// Determinism: `candidate_rels` must already be sorted by the caller — the same convention S1/S3/S5
/// rely on — so the sample is stable without a re-sort. Unreadable files are skipped silently: a file
/// this pass cannot read contributes zero to a DENOMINATOR, which understates rather than invents.
fn count_path_literals(root: &Path, candidate_rels: &[String]) -> (usize, Vec<String>) {
    let re = path_literal_re();
    let mut total = 0usize;
    let mut samples: Vec<String> = Vec::new();
    for rel in candidate_rels.iter().filter(|r| is_js_ts_family(r)) {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let hits = re
            .find_iter(&text)
            .filter(|m| is_internal_path_literal(m.as_str()))
            .count();
        if hits == 0 {
            continue;
        }
        total += hits;
        if samples.len() < MAX_SAMPLES {
            samples.push(rel.clone());
        }
    }
    (total, samples)
}

/// `Some(warning)` when this tree writes at least [`PATH_LITERAL_SITES_MIN`] internal-path literals
/// while io extraction stays below [`IO_NEAR_ZERO_FLOOR`] in BOTH directions — the denominator the
/// vocabulary-anchored tripwires structurally cannot provide.
///
/// Cheap on the success path: the two count gates short-circuit before any disk read, the same
/// convention S1/S3/S5 follow; only a tree already near-silent in both io directions pays the re-read,
/// and only over its js/ts-family files.
pub fn path_literal_denominator_warning(
    root: &Path,
    candidate_rels: &[String],
    io_provides_count: usize,
    io_consumes_keyed_count: usize,
) -> Option<String> {
    if io_provides_count >= IO_NEAR_ZERO_FLOOR || io_consumes_keyed_count >= IO_NEAR_ZERO_FLOOR {
        return None;
    }
    let (total, samples) = count_path_literals(root, candidate_rels);
    if total < PATH_LITERAL_SITES_MIN {
        return None;
    }
    let extracted = io_provides_count + io_consumes_keyed_count;
    Some(format_warning(total, extracted, &samples))
}

/// The warning text. States the ratio and what it does and does not mean — this channel reports a
/// denominator, so it must not be read as "these are missed routes".
fn format_warning(total: usize, extracted: usize, samples: &[String]) -> String {
    let sample_tail = if samples.is_empty() {
        String::new()
    } else {
        format!(" Examples: {}.", samples.join(", "))
    };
    format!(
        "{total} internal path literal(s) (e.g. '/user/list.do') appear in this tree's js/ts sources \
while only {extracted} http io fact(s) were extracted tree-wide (provides + keyed consumes, both \
below the near-zero floor). THIS IS A DENOMINATOR, NOT A FINDING: a path literal is not proof of an \
HTTP call, and this channel does not claim these are missed routes. What it does say is that \
\"nothing was extracted\" cannot be read as \"there was nothing to extract\" for this tree — an HTTP \
call idiom this build does not recognize produces NO io fact at all, so it leaves no trace in \
`ioConsumesUnresolved`, in the unresolved-consume ratio, or in the zero-extraction floor, all of \
which are computed over facts that already exist. Cross-layer joins involving this tree will be \
near-silent on whichever side those paths belong to. To restore join visibility without waiting for \
extraction to learn the idiom, project this tree's io with a Mode B overlay adapter: a partial \
envelope covering just the affected channel is enough; contract: MCP resource \
`zzop://contract/envelope-guide` on MCP hosts (`zzop contract envelope-guide` with the CLI \
binary), docs/NORMALIZED_AST.md in the repo.{sample_tail}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework_silence::tests::TempDir;

    /// Builds a tree from (rel, body) pairs using the module family's own `TempDir` — this crate has
    /// no `tempfile` dev-dependency and the sibling tests all share this helper.
    fn tree(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new("zzop-path-literal-denominator");
        for (rel, body) in files {
            dir.write(rel, body);
        }
        dir
    }

    /// The subject population: many internal path literals, io near-zero in both directions.
    #[test]
    fn fires_when_paths_are_many_and_io_is_near_zero() {
        let body = (0..10)
            .map(|i| format!("$.ajax({{ url: '/user/op{i}.do' }});\n"))
            .collect::<String>();
        let d = tree(&[("js/user.js", &body)]);
        let rels = vec!["js/user.js".to_string()];
        let w = path_literal_denominator_warning(d.path(), &rels, 0, 1).expect("should fire");
        assert!(w.contains("10 internal path literal(s)"), "{w}");
        assert!(w.contains("DENOMINATOR, NOT A FINDING"), "{w}");
        assert!(w.contains("js/user.js"), "{w}");
    }

    /// A healthy consume side silences it — this is the conflation guard, not a tuning knob.
    #[test]
    fn silent_when_consumes_were_extracted() {
        let body = (0..10)
            .map(|i| format!("fetch('/user/op{i}.do');\n"))
            .collect::<String>();
        let d = tree(&[("js/a.js", &body)]);
        let rels = vec!["js/a.js".to_string()];
        assert!(path_literal_denominator_warning(d.path(), &rels, 0, IO_NEAR_ZERO_FLOOR).is_none());
    }

    /// A healthy PROVIDE side silences it too — a server that registered its routes is not a subject
    /// even though `app.get('/users')` is the same lexical shape.
    #[test]
    fn silent_when_provides_were_extracted() {
        let body = (0..10)
            .map(|i| format!("app.get('/user/op{i}', h);\n"))
            .collect::<String>();
        let d = tree(&[("js/server.js", &body)]);
        let rels = vec!["js/server.js".to_string()];
        assert!(path_literal_denominator_warning(d.path(), &rels, IO_NEAR_ZERO_FLOOR, 0).is_none());
    }

    /// Below the literal floor, ordinary furniture stays quiet.
    #[test]
    fn silent_below_the_literal_floor() {
        let d = tree(&[("js/a.js", "const logo = '/img/logo.png';\n")]);
        let rels = vec!["js/a.js".to_string()];
        assert!(path_literal_denominator_warning(d.path(), &rels, 0, 0).is_none());
    }

    /// Protocol-relative and absolute URLs are not internal paths, so they do not inflate the
    /// denominator — the one exclusion this census makes, and it is lexical, not a vocabulary.
    #[test]
    fn external_origins_do_not_count() {
        let body = (0..12)
            .map(|i| format!("load('//cdn.example.com/a{i}.js'); load('https://x.test/b{i}');\n"))
            .collect::<String>();
        let d = tree(&[("js/a.js", &body)]);
        let rels = vec!["js/a.js".to_string()];
        assert!(path_literal_denominator_warning(d.path(), &rels, 0, 0).is_none());
    }

    /// Non-js files are outside the census even when they carry path literals — the file family is
    /// shared with S5 rather than re-derived here.
    #[test]
    fn non_js_files_are_not_counted() {
        let body = (0..12)
            .map(|i| format!("<a href='/user/op{i}.do'>x</a>\n"))
            .collect::<String>();
        let d = tree(&[("views/list.jsp", &body)]);
        let rels = vec!["views/list.jsp".to_string()];
        assert!(path_literal_denominator_warning(d.path(), &rels, 0, 0).is_none());
    }
}
