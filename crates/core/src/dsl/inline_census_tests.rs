//! The THIRD policy-value census: every policy value written INLINE in a pack's matcher fields, which
//! the other two are structurally unable to see.
//!
//! # The hole this closes
//!
//! `scripts/check-policy-census.sh` censuses Rust `const` NAMES. `tests_fragments::name_census` censuses
//! `${NAME}` fragment NAMES. Both key on a name, so a policy value spelled inline in a matcher field —
//! which has no name — was invisible to both, and the fragment census's header claim that moving
//! vocabulary "into a pack" no longer bypasses the triage moment was true only of NAMED fragments.
//!
//! Measured, not hypothesised: `sql/nplus1` shipped the root anchor `^(?:domains/[^/]+/routes/.+|api/.+)`
//! — a policy decision that made a flagship rule structurally silent under `src/api/` — and it lived
//! there with NO triage record anywhere, because no census had a row for it. It was found by hand during
//! an unrelated review and is now also structurally forbidden by
//! `zzop-engine`'s `tests/rule_contracts/path_anchor_pin.rs`; that pin closes ONE axis (`file_pattern`
//! root anchors), this census closes the surface.
//!
//! # Subject set: derived, never listed
//!
//! The fields walked are whatever `def::pattern_fields::for_each_pattern_field` walks — the same
//! function `RulePackDef::expand_fragments` resolves `${NAME}` refs through, whose arms destructure each
//! matcher struct with no `..`. A new pattern field on any matcher is therefore a COMPILE ERROR there
//! until it is classified, and the moment it is classified as a pattern this census starts emitting rows
//! for it. A hand list would have gone blind at exactly that moment, which is the failure mode the two
//! sibling censuses each recorded independently.
//!
//! Packs are read RAW (`tests_fragments::raw_packs`, a plain `serde_json` parse), not expanded. A
//! `${NAME}` reference therefore censuses as the REFERENCE, axis `named`, and its content is triaged
//! once — by the fragment census — instead of being copied into this file at every use site. The two
//! censuses partition the surface rather than overlapping on it.
//!
//! # Why Rust and not a `scripts/check-*.sh` guard
//!
//! Same reason `name_census` lives here: the subject is JSON, this repo's shells have neither `jq` nor a
//! usable python, and the one previous attempt at a line-oriented `awk` extractor over these files was
//! measured to drop keys SILENTLY on valid JSON (see `name_census`'s header for the table). The
//! extraction cannot be done in shell without hand-rolling a JSON tokenizer in `awk`, one crate away
//! from the parser this crate already runs over the same files. It also does not belong in
//! `.githooks/pre-commit`: pre-commit must stay in the seconds, and this reads and parses every pack.
//!
//! # Why a committed snapshot and not an inline `const` list
//!
//! `name_census` argues (correctly, for 7 rows) that an inline list needs no update command and shows up
//! in the diff of the change that adds it. This axis is two orders of magnitude larger — the same
//! threshold at which `scripts/policy-census.txt` earns its own file — and its rows carry VALUES, which
//! are regexes. A snapshot file is what that shape wants.
//!
//! # The axis column
//!
//! Existence is not enough; the artifact has to be the VERDICT. A row's axis is one of [`AXES`], and `?`
//! — what regeneration writes for a row it has never seen — is deliberately not one of them, so
//! regenerating can never be a way to make this test pass.
//!
//! The discriminating question for an inline VALUE is: **who chose this string?**
//!
//! * `fact` — a framework, language, vendor or protocol did. `\bRuntime\.exec\b` is the JDK's spelling,
//!   `innerHTML` is the DOM's, `\$transaction\s*\(` is Prisma's. Another project cannot spell it
//!   differently and still mean the same thing.
//! * `convention` — WE did, and another project could reasonably want it different: path anchors,
//!   directory names, what counts as a guard, what counts as "money", how many is too many. These are
//!   the ones that belong in config with the built-in shipped as a template default. When in doubt write
//!   this one, for the same asymmetry the Rust census documents: a fact misfiled as a convention costs a
//!   config key nobody sets; a convention misfiled as a fact leaves the engine guessing.
//! * `shape` — mechanical syntax carrying no vocabulary: extension alternations, `(?:^|/)` anchors,
//!   comment/whitespace/word-boundary scaffolding, "an identifier followed by `(`". Nobody's opinion is
//!   encoded, so nothing is configurable and nothing can be wrong except the regex itself.
//! * `named` — the whole value is a `${NAME}` fragment reference. Triaged by `name_census`, not here.
//! * `unreviewed` — an honest refusal: nobody has judged this row yet. Legal (so an unfinished triage
//!   cannot hide behind a green `?`-free file), but RATCHETED by [`UNREVIEWED_CEILING`], which may only
//!   ever be lowered.
//!
//! A row's key includes the rule id, so two rules sharing a value get two rows. That is deliberate and
//! fail-closed: they will normally carry the same axis (the verdict is a property of the value), but a
//! NEW rule reusing an already-triaged value still has to be looked at, because applying a policy
//! somewhere new is itself a decision.
//!
//! # The `-> <config-key>` column on `convention` rows
//!
//! The axis definition above says a `convention` "belongs in config" — but until 2026-08-03 nothing
//! made any row say WHICH config key, so the claim was unfalsifiable row by row: the census could call
//! sixty values config-shaped while the config surface offered a home to none of them, and no diff
//! would ever show the gap. So a `convention` row must carry the same `-> <config-key>` column the
//! sibling `scripts/policy-census.txt` requires of its convention rows: either a key that actually
//! exists on the vocabulary surface (`-> vocabulary.moneyTokens`), or the honest `-> (none yet)`.
//! It rides in the tail, after the quoted value, because the value column is quote-delimited here
//! (the Rust census has no value column, so its arrow can follow the axis directly).
//!
//! `(none yet)` is not an exemption — it is the debt list: the set of conventions whose config home
//! does not exist yet, enumerated in one greppable place instead of implied. And the axis itself is
//! RATCHETED by [`CONVENTION_CEILING`] for the same reason `unreviewed` is: every convention row is
//! either a key nobody can set yet or a key the pack is not wired to read, so the count may only
//! shrink as vocabulary moves into config — a new one must raise the ceiling in the same change,
//! with the case for why it cannot be config from day one.

// This module PRINTS BY DESIGN — the regeneration path has to say what it wrote and how much of it is
// still untriaged, and the check path prints the axis distribution the way `check-policy-census.sh`
// prints its own. The workspace `print_stdout` lint exists because stdout carries a machine-readable
// contract on the SHIPPING lanes; a `cargo test` harness is not one of them.
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;

use super::def::for_each_pattern_field;
use super::tests_fragments::{raw_packs, repo_rel};

/// Repo-relative path of the committed snapshot.
const CENSUS_FILE: &str = "scripts/dsl-inline-census.txt";

/// Set to regenerate the snapshot instead of checking it (the `--update` of the shell census).
const UPDATE_ENV: &str = "ZZOP_UPDATE_DSL_INLINE_CENSUS";

/// The exact regeneration command, spelled once, printed by every failure that a regeneration fixes.
/// No harness flags in it: a `--flag` spelled in shipped source has to be a real CLI/external-tool flag
/// (`rule_contracts::reference_validation`), and a test-harness flag is neither. That constraint is why
/// the regeneration path REPORTS BY FAILING rather than by printing — see the update branch below.
const UPDATE_CMD: &str =
    "ZZOP_UPDATE_DSL_INLINE_CENSUS=1 cargo test -p zzop-core dsl_inline_value_census";

/// The axis vocabulary and its one-line legend — see this module's header for the full discriminating
/// question. This is the SINGLE owner of both: the snapshot's header block is generated from it, so the
/// legend a reader of the txt file sees cannot drift from the one the check enforces.
const AXES: &[(&str, &str)] = &[
    (
        "fact",
        "a framework/language/vendor/protocol fixed this string; another project cannot respell it",
    ),
    (
        "convention",
        "WE chose it and another project could want it different -> belongs in config; the row's tail must name its config home: `-> <config-key>` or `-> (none yet)`",
    ),
    (
        "shape",
        "mechanical regex syntax carrying no vocabulary (extension lists, path anchors, comment/boundary scaffolding)",
    ),
    (
        "named",
        "a whole-value ${NAME} fragment reference -- triaged by zzop_core::dsl::tests_fragments::name_census, not here",
    ),
    (
        "unreviewed",
        "nobody has judged this row yet; ratcheted by UNREVIEWED_CEILING, which may only be lowered",
    ),
];

/// Floor on the number of extracted triples (§5.5). Far enough above zero that a broken walk, an empty
/// `rules/dsl`, or a `raw_packs` that stopped finding packs fails LOUDLY instead of leaving this census
/// vacuously green over an empty subject set. Today's count is deliberately not written here: this
/// prose said "644 across 12 packs and 140 rules" until v0.30.0 exported 17 rules to `examples/packs/`,
/// which took it to 538 across 11 packs and 116 rules without a single triple being reviewed away. The
/// test prints the live figures on every run, and the assertion below prints them on failure.
///
/// The drift comparison cannot be its own floor: it is self-referential the moment someone regenerates
/// against a broken extraction, after which an empty snapshot equals an empty scan forever. This is the
/// same reasoning `scripts/check-policy-census.sh`'s census floor records, and the same one
/// `path_anchor_pin.rs`'s `SUBJECT_FLOOR` records one axis over.
const SUBJECT_FLOOR: usize = 500;

/// Ratchet on the `unreviewed` axis: how many rows may honestly say "not judged yet". LOWER ONLY. An
/// axis that means "I could not decide" has to cost something, or it becomes the default and this census
/// degrades into the snapshot it exists not to be.
const UNREVIEWED_CEILING: usize = 0;

/// Ratchet on the `convention` axis: how many inline values may claim "WE chose this and it belongs in
/// config" while still living inline in a pack. LOWER ONLY, like [`UNREVIEWED_CEILING`]: every one of
/// these rows is standing debt — either its `-> (none yet)` column says no config key exists for it
/// yet, or a key exists and the pack is not wired to read it — so the number may shrink as vocabulary
/// moves into config and must never grow silently. A new convention entering the packs raises this
/// ceiling in the same change, with the case for why it cannot be config from day one. 62 measured
/// 2026-08-03, the day the `-> <config-key>` column was added. +2 same day (A17):
/// `high-entropy-secret`'s `name_pattern`/`name_exclude_pattern` — the secret-name keyword vocabulary
/// and the mock-word name-hygiene list, both the SAME word lists `hardcoded-secret`'s pre-existing
/// convention rows already carry (restated on a second rule because applying a policy somewhere new
/// is a decision, per this module's header). Not config from day one for the sibling rows' reason: no
/// `vocabulary.secretNameTokens`-shaped key exists yet, and minting one for this change alone would
/// leave the older rows' identical vocabulary still inline — that debt closes together or not at all.
/// +2 2026-08-09: `typescript/as-cast` and `typescript/no-explicit-any` each gained a
/// `require_file_absent` restating `zzop_engine::generated_banner::MARKERS` as a regex, so a rule whose
/// fix advice is "change this line" goes silent on a file the next regeneration rewrites (measured: 30
/// findings over two corpus files). NOT config from day one for a STRUCTURAL reason, not a missing key:
/// `vocabulary.generatedFileMarkers` already exists and the engine already reads it — a DSL matcher
/// cannot. Its patterns are compiled from the pack's own JSON before any config is resolved, so no
/// line-scan field can consult a vocabulary key without a new engine-side matcher input, which is
/// exactly the surface the 2026-08-09 decision declined to build. The two rules disclose the divergence
/// in their own `message`; the debt closes if a config-aware matcher field is ever minted.
/// +1 2026-08-09: `egress/get-and-body` gained the SAME `require_file_absent` regex, byte for byte, for
/// the same reason one rule over — its fix advice ("use POST, or move the data to query params") is
/// undone by the next regeneration. A third row rather than a shared one because the value's key
/// includes the rule id and applying a policy to a new rule is itself a decision (this module's header);
/// the STRUCTURAL reason it cannot be config is unchanged, and method-scan is under it exactly as much
/// as line-scan — `MethodScan`'s patterns are compiled from the pack JSON before any config resolves.
/// Measured: 1 corpus finding removed (`fe-vue/src/services/api.ts`, a swagger-typescript-api client),
/// which was this rule's entire corpus yield; deleting the banner line alone brings it back.
///
/// **Set to the measured count, never "count + room".** Reviewed 2026-08-09: the two bumps above had
/// each added +1 to a bound that was already slack, so the ceiling stood at 67 against 65 actual rows —
/// two more convention rows could have entered with no bump, hence no author and no written case, which
/// is the decision moment this ratchet exists to force. Recount before changing it:
/// `cargo test -p zzop-core dsl_inline_value_census -- --nocapture` prints `convention=<n>`.
///
/// A convention row leaves for TWO different reasons and only ONE of them is the one the paragraph
/// above imagines. (1) Its vocabulary moved into config — the debt was PAID, which is what this ratchet
/// is for. (2) The rule carrying it LEFT THE BUNDLE: v0.30.0 exported 17 rules to `examples/packs/`
/// (`5d2050f`, `2de0399`, `9a49080`, `105c52f`; one returned in `c0cc8ed`), this census scans
/// `rules/dsl/**` only, and the convention axis fell to 57 with nothing paid off — the same rows are
/// still inline, one directory over. An export therefore leaves this ceiling slack by exactly the count
/// it carried out, and that slack appears in NO line of the diff that caused it. **Lower it in the same
/// change as the export**, for the 2026-08-09 reason one paragraph up: the audit that found this at 65
/// against 57 found 8 rows of room, four times the gap that was judged a defect then. 57 measured
/// 2026-08-12 (v0.30.0 release audit).
const CONVENTION_CEILING: usize = 57;

/// [`CENSUS_FILE`] as an absolute path — one owner, so the reader and the writer cannot disagree about
/// which file this census is. `real_dsl_dir()` is `<manifest>/../../rules/dsl`, so two levels up from it
/// is the repo root.
fn census_path() -> std::path::PathBuf {
    super::tests_fragments::real_dsl_dir()
        .join("../..")
        .join(CENSUS_FILE)
}

/// `(key, value)` for every pattern-bearing field of every rule of every shipped pack, sorted and
/// deduplicated. `key` is `<pack json path>:<rule id>:<field>`; `value` is the raw JSON string, RAW
/// (unexpanded), so a `${NAME}` reference stays a reference.
fn extracted_rows() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (path, mut pack) in raw_packs() {
        let rel = repo_rel(&path);
        for rule in &mut pack.rules {
            let prefix = format!("{rel}:{}", rule.id);
            for_each_pattern_field::<std::convert::Infallible>(rule, &mut |field, value| {
                out.push((format!("{prefix}:{field}"), value.clone()));
                Ok(())
            })
            .expect("the census callback is infallible");
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Splits `rest` — everything after `<key> <axis> ` on a snapshot line — into the debug-quoted VALUE and
/// the optional ` # rationale` tail. Quote-aware (a value's own `\"` must not end it), so a rationale is
/// never mistaken for part of the value and vice versa.
fn split_value_and_tail(rest: &str) -> Option<(String, String)> {
    let bytes: Vec<char> = rest.chars().collect();
    if bytes.first() != Some(&'"') {
        return None;
    }
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            '\\' => i += 1,
            '"' => {
                let value: String = bytes[..=i].iter().collect();
                let tail: String = bytes[i + 1..].iter().collect();
                return Some((value, tail.trim().to_string()));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The committed snapshot as `(key, quoted value) -> (axis, tail)`, plus its row order. `#` lines are the
/// generated legend header and are skipped.
fn read_snapshot(text: &str) -> BTreeMap<(String, String), (String, String)> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once(' ') else {
            continue;
        };
        let Some((axis, rest)) = rest.split_once(' ') else {
            continue;
        };
        let Some((value, tail)) = split_value_and_tail(rest) else {
            continue;
        };
        out.insert((key.to_string(), value), (axis.to_string(), tail));
    }
    out
}

/// The snapshot text for `rows`, carrying each row's axis and rationale forward from `prior` and writing
/// `?` for a row `prior` has never seen. The header block is generated from [`AXES`].
fn render(
    rows: &[(String, String)],
    prior: &BTreeMap<(String, String), (String, String)>,
) -> String {
    let mut out = String::new();
    out.push_str(
        "# DSL inline policy-value census -- one line per (pack/rule, field, value) triple over\n",
    );
    out.push_str("# every pattern-bearing matcher field in rules/dsl/**. Owner + full legend:\n");
    out.push_str("# crates/core/src/dsl/inline_census_tests.rs. Regenerate:\n");
    out.push_str(&format!("#   {UPDATE_CMD}\n"));
    out.push_str(
        "# Line shape: <pack>:<rule>:<field> <axis> <value, Rust-debug-quoted> [# rationale]\n",
    );
    out.push_str("# Axes:\n");
    for (axis, legend) in AXES {
        out.push_str(&format!("#   {axis:<11}{legend}\n"));
    }
    for (key, value) in rows {
        let quoted = format!("{value:?}");
        let (axis, tail) = prior
            .get(&(key.clone(), quoted.clone()))
            .cloned()
            .unwrap_or_else(|| ("?".to_string(), String::new()));
        out.push_str(key);
        out.push(' ');
        out.push_str(&axis);
        out.push(' ');
        out.push_str(&quoted);
        if !tail.is_empty() {
            out.push(' ');
            out.push_str(&tail);
        }
        out.push('\n');
    }
    out
}

/// The census: every inline policy value in every shipped pack is registered, in both directions, with a
/// human axis on it. See this module's header for what each axis means and why the surface exists.
#[test]
fn dsl_inline_value_census_matches_the_committed_snapshot() {
    let census_path = census_path();
    let rows = extracted_rows();

    assert!(
        rows.len() >= SUBJECT_FLOOR,
        "the inline-value census extracted only {} triple(s) from rules/dsl/**, below the floor of \
         {SUBJECT_FLOOR}. Two readings, and they take OPPOSITE actions, so read the diff that caused \
         this before choosing:\n\
         (1) the walk, the pack enumeration, or the pack tree itself is broken and this census is \
         vouching for NOTHING — fix the extraction; do NOT lower the floor to make this pass;\n\
         (2) rules legitimately LEFT `rules/dsl/**`, the way v0.30.0 exported 17 of them to \
         `examples/packs/` (644 triples -> 538) — the rows did not disappear, they moved one directory \
         over, out of this census's scan root. Nothing is broken, and lowering the floor in the same \
         commit as the export IS the correct action. Lower CONVENTION_CEILING in that same commit too: \
         an export leaves it slack by exactly what it carried out.",
        rows.len(),
    );

    let prior_text = std::fs::read_to_string(&census_path).unwrap_or_default();
    let prior = read_snapshot(&prior_text);

    if std::env::var(UPDATE_ENV).is_ok() {
        let rendered = render(&rows, &prior);
        let untriaged = rendered.lines().filter(|l| is_axis(l, "?")).count();
        // LF, unconditionally, and pinned `text eol=lf` in `.gitattributes` for the same reason
        // `scripts/policy-census.txt` is: regeneration rewrites this file WHOLE, so without the pin a
        // Windows checkout (autocrlf) and a Linux one disagree about every single line the first time
        // anyone regenerates on the other platform.
        std::fs::write(&census_path, rendered).expect("write census snapshot");
        println!(
            "dsl-inline-census: regenerated ({} rows) -> {CENSUS_FILE}",
            rows.len()
        );
        // The untriaged count is reported by FAILING, not by printing: a cargo-test harness swallows a
        // passing test's stdout, and the one thing this run must not do is write a file full of `?` and
        // look like it finished. (`--nocapture` would show the print, but a harness flag cannot be
        // spelled in shipped source — see UPDATE_CMD.) The write above already happened, so the fix is
        // to edit the axis column, not to re-run.
        assert_eq!(
            untriaged, 0,
            "regenerated {CENSUS_FILE} ({} rows), and {untriaged} of them carry axis '?' -- a row this \
             census has never seen. Open the file and give each one an axis ({}); '?' is not one of \
             them, and no regeneration can turn it into one.",
            rows.len(),
            AXES.iter().map(|(a, _)| *a).collect::<Vec<_>>().join("/"),
        );
        return;
    }

    assert!(
        !prior_text.is_empty(),
        "{CENSUS_FILE} is missing or empty -- regenerate it with:\n  {UPDATE_CMD}"
    );

    let mut added = Vec::new();
    for (key, value) in &rows {
        let quoted = format!("{value:?}");
        if !prior.contains_key(&(key.clone(), quoted.clone())) {
            added.push(format!("{key} {quoted}"));
        }
    }
    let current: std::collections::BTreeSet<(String, String)> = rows
        .iter()
        .map(|(k, v)| (k.clone(), format!("{v:?}")))
        .collect();
    let removed: Vec<String> = prior
        .keys()
        .filter(|k| !current.contains(*k))
        .map(|(k, v)| format!("{k} {v}"))
        .collect();

    assert!(
        added.is_empty() && removed.is_empty(),
        "the inline policy values shipped in rules/dsl/** have drifted from {CENSUS_FILE}.\n  \
         added ({}):\n    {}\n  removed ({}):\n    {}\n\n\
         An ADDED row is a policy value that entered the packs with no triage record -- exactly what \
         this census exists to force a moment for. A REMOVED row is a value that other decisions may \
         have been resting on. Triage each added row (axes: {}), then regenerate:\n  {UPDATE_CMD}\n\
         Regenerating does NOT make this pass: a row it has never seen is written with axis '?', which \
         is not a legal axis.",
        added.len(),
        added.join("\n    "),
        removed.len(),
        removed.join("\n    "),
        AXES.iter().map(|(a, _)| *a).collect::<Vec<_>>().join("/"),
    );

    let legal: Vec<&str> = AXES.iter().map(|(a, _)| *a).collect();
    let bad: Vec<String> = prior
        .iter()
        .filter(|(_, (axis, _))| !legal.contains(&axis.as_str()))
        .map(|((k, v), (axis, _))| format!("{k} -> {axis} ({v})"))
        .collect();
    assert!(
        bad.is_empty(),
        "{} census row(s) carry no legal axis (a '?' is what regeneration writes for an untriaged \
         row):\n    {}\n\nEach line must read `<pack>:<rule>:<field> <axis> <value>` with axis one of \
         {}. See crates/core/src/dsl/inline_census_tests.rs for what each means; when in doubt write \
         `convention`.",
        bad.len(),
        bad.join("\n    "),
        legal.join("/"),
    );

    // The declared line shape ends in an optional `[# rationale]`, and until 2026-08-10 only the
    // `convention` axis had anything look at what sat there (the `-> <config-key>` column below, whose
    // parse already rejects any text between the key and the ` #` — measured). Every other axis had its
    // tail treated as opaque: `split_value_and_tail` swallows everything after the closing quote and
    // `render` writes a nonempty tail back verbatim, so a tail that never was a rationale is not
    // corrected by regeneration — it is PERPETUATED by it. That is not hypothetical: three rows carried
    // a second copy of their own quoted value, pasted in during the hand-triage that created this file
    // (b9dfa3f), and survived every regeneration since because nothing ever read the tail's shape.
    let bad_tail: Vec<String> = prior
        .iter()
        .filter(|(_, (axis, _))| axis != "convention")
        .filter(|(_, (_, tail))| !tail.is_empty() && !tail.starts_with('#'))
        .map(|((k, v), (axis, tail))| format!("{k} {axis} {v} {tail}"))
        .collect();
    assert!(
        bad_tail.is_empty(),
        "{} census row(s) carry text after the value that is not a `# rationale`. Each line below is \
         reproduced verbatim -- grep {CENSUS_FILE} for it:\n    {}\n\nThe declared line shape is \
         `<pack>:<rule>:<field> <axis> <value, Rust-debug-quoted> [# rationale]` (convention-axis rows \
         additionally carry `-> <config-key>` before the `#`). Anything after the closing quote of the \
         value must therefore begin with `#`. Regenerating will NOT fix this -- a nonempty tail is \
         carried forward verbatim -- so edit the line: keep the rationale, delete the rest.",
        bad_tail.len(),
        bad_tail.join("\n    "),
    );

    // Every `convention` row must carry its `-> <config-key>` column — the sibling policy census's
    // convention format, carried in the tail here because the value column is quote-delimited. A row
    // without it is a value the census calls config-shaped while refusing to say which key would hold
    // it, which is exactly the unfalsifiable claim the column exists to close.
    let missing_key: Vec<String> = prior
        .iter()
        .filter(|(_, (axis, _))| axis == "convention")
        .filter(|(_, (_, tail))| convention_config_key(tail).is_none())
        .map(|((k, v), _)| format!("{k} {v}"))
        .collect();
    assert!(
        missing_key.is_empty(),
        "{} convention-axis census row(s) carry no `-> <config-key>` column:\n    {}\n\nA convention is \
         a value that BELONGS IN CONFIG, so its row must name the config key that holds (or would \
         hold) it, after the value: `... convention \"<value>\" -> vocabulary.<key> # rationale`. If \
         no key exists on the vocabulary surface yet, write `-> (none yet)` — that spelling is the \
         debt list, not an exemption.",
        missing_key.len(),
        missing_key.join("\n    "),
    );

    let conventions = prior
        .values()
        .filter(|(axis, _)| axis == "convention")
        .count();
    assert!(
        conventions <= CONVENTION_CEILING,
        "{conventions} census row(s) carry the convention axis, above the ratchet of \
         {CONVENTION_CEILING}. Every convention row is standing debt (a config key nobody can set \
         yet, or one the pack is not wired to read), so this count may only shrink. If the new value \
         genuinely cannot be config from day one, raise CONVENTION_CEILING in the same change and \
         make that case where you raise it."
    );

    let unreviewed = prior
        .values()
        .filter(|(axis, _)| axis == "unreviewed")
        .count();
    // `saturating_sub`, not `<=`: the ceiling is 0 today, and `x <= 0` on a `usize` is an absurd
    // comparison clippy denies. Written as "how many rows are OVER the ceiling" so the assertion keeps
    // its meaning unchanged the day the ceiling is a non-zero number being ratcheted down.
    assert_eq!(
        unreviewed.saturating_sub(UNREVIEWED_CEILING),
        0,
        "{unreviewed} census row(s) carry axis `unreviewed`, above the ratchet of \
         {UNREVIEWED_CEILING}. That axis is an honest refusal, not a parking space: it may shrink, \
         never grow. Triage the new rows properly, or make the case for raising the ceiling in the \
         same change that raises it."
    );

    // Fixed axis order (never a hash-map iteration), so the summary a reader compares between two runs
    // cannot shuffle — the same reason `check-policy-census.sh` spells its axes out in order.
    let by_axis: Vec<String> = AXES
        .iter()
        .map(|(axis, _)| {
            let n = prior.values().filter(|(a, _)| a == axis).count();
            format!("{axis}={n}")
        })
        .collect();
    println!(
        "dsl-inline-census: OK ({} inline policy values tracked across {} packs; {})",
        rows.len(),
        raw_packs().len(),
        by_axis.join(" "),
    );
}

/// The `-> <config-key>` column a `convention` row's tail must open with: `-> ` followed by either the
/// literal `(none yet)` or a single spaceless key (e.g. `vocabulary.moneyTokens`), optionally followed
/// by ` # rationale`. Returns the claimed key, or `None` when the column is absent or malformed.
fn convention_config_key(tail: &str) -> Option<&str> {
    let rest = tail.strip_prefix("-> ")?;
    let target = rest.split(" #").next().unwrap_or(rest).trim();
    if target == "(none yet)" || (!target.is_empty() && !target.contains(' ')) {
        Some(target)
    } else {
        None
    }
}

/// Whether a rendered line's axis column equals `axis` — used only to count `?` rows during a
/// regeneration run.
fn is_axis(line: &str, axis: &str) -> bool {
    if line.starts_with('#') {
        return false;
    }
    line.split_once(' ')
        .and_then(|(_, rest)| rest.split_once(' '))
        .is_some_and(|(a, _)| a == axis)
}

/// Pin: the snapshot's rows are in the same order the extraction produces them (sorted, deduplicated),
/// so a new row lands next to its siblings and a diff shows added lines rather than a reshuffle. Same
/// reason `scripts/policy-census.txt` is `sort -u`'d.
#[test]
fn the_committed_snapshot_is_sorted_and_deduplicated() {
    let census_path = census_path();
    let text = std::fs::read_to_string(&census_path).expect("census snapshot must exist");
    let keys: Vec<(String, String)> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| {
            let (key, rest) = line.split_once(' ')?;
            let (_, rest) = rest.split_once(' ')?;
            let (value, _) = split_value_and_tail(rest)?;
            Some((key.to_string(), value))
        })
        .collect();

    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        keys, sorted,
        "{CENSUS_FILE} is not in sorted, deduplicated key order -- regenerate it: {UPDATE_CMD}"
    );
    assert!(
        !keys.is_empty(),
        "{CENSUS_FILE} parsed to ZERO rows. Either every line is malformed or the reader above stopped \
         matching the writer's line shape -- both leave this census green over nothing."
    );
}
