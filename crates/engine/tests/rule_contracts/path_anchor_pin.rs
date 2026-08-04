//! Policy pin (T2 — the boundary admits no shared symbol): no shipped DSL rule's `file_pattern` locks
//! its scan surface to the TREE ROOT by placing a literal directory segment immediately after `^`.
//!
//! Split out of `policy_pins.rs` (its natural home, already past 300 lines) rather than compressed into
//! it — the structural analyzer below is most of this file and belongs next to the doc explaining what
//! it can and cannot prove, not squeezed into a sibling pin's margins.
//!
//! # The defect this exists to make impossible again (2026-08-01)
//!
//! `sql/nplus1` shipped `^(?:domains/[^/]+/routes/.+|api/.+)\.ts$`. Anchored at `^`, so it matched
//! `api/orders.ts` and **never** `src/api/orders.ts` — a flagship N+1 rule structurally silent in the
//! most common project layout. Its siblings saying the same "this is a backend path" thing
//! (`sql/race-condition-toctou`, plus `http/get-route-no-cache-marker` until that rule's 2026-08-02
//! deletion) used `(?:^|/)`. The spelling was
//! aligned by hand and proven with the release binary (the rule now fires under `src/api/`); nothing
//! stopped the next author writing `^api/` again on green.
//!
//! # Why no existing guard could see it
//!
//! Both of the policy-value censuses that existed then keyed on a NAME: `scripts/check-policy-census.sh`
//! counts Rust `const` names, and `zzop_core::dsl::tests_fragments::name_census` counts `${NAME}`
//! fragment names. A policy value written inline in a matcher field has no name, so it was invisible to
//! both. That blind spot was larger than this pin, and it is now closed one level up by a THIRD census —
//! `zzop_core::dsl::inline_census_tests` -> `scripts/dsl-inline-census.txt` (2026-08-02), which registers
//! every `(pack/rule, field, value)` triple with a triage axis. The two are complementary and neither
//! subsumes the other: the census forces a HUMAN VERDICT on any inline value that appears or changes;
//! this pin forbids one specific SHAPE outright, with no triage moment available. The anchor that
//! motivated both censuses now carries axis `convention` on its census row.
//!
//! # Why a pin and not a shared symbol (T1 is structurally unavailable)
//!
//! Two spellings of the anchor idiom remain, and both live in the SAME pack (`sql/nplus1` and
//! `sql/race-condition-toctou` — the third, `http/get-route-no-cache-marker`'s, left with that rule's
//! 2026-08-02 deletion), so "separate packs" is no longer what blocks a shared symbol. What still
//! does: the only sharing mechanism is a `${NAME}` fragment, and `fragment_ref_name` resolves a
//! reference only when the WHOLE field value is `${NAME}`
//! (`crates/core/src/dsl/fragments.rs`) — but the shared thing here is a path-anchor IDIOM
//! inside a larger pattern, not the whole `file_pattern`. The DSL cannot express that, so the
//! relationship is asserted here instead.
//!
//! # Scope boundary — read this before citing a green run
//!
//! * Subject set: `file_pattern` ONLY. `file_exclude_pattern` carries the mirror defect (a root-anchored
//!   exclusion silently fails to exclude), but its failure mode is noise, not silence, and it is a
//!   separate judgment; it is deliberately not in this pin's subject set.
//! * This is a structural analyzer over regex SYNTAX, not a regex semantics engine. It reasons about
//!   what can follow an `^`/`\A` anchor, descending through groups and alternations. It does not
//!   evaluate the pattern, and a root-lock expressed by some means it does not model would pass.
//! * `load_all_packs` returns packs whose `${NAME}` fragments are already expanded (`parse_dsl_pack`
//!   calls `RulePackDef::expand_fragments` at the disk-load boundary), so the analyzer always reads the
//!   pattern the engine actually compiles — a future `file_pattern: "${some-fragment}"` is judged at its
//!   resolved value, not skipped as an opaque reference.

use zzop_core::{Matcher, RulePackDef};

use crate::load_all_packs;

/// Every shipped DSL rule declares a `file_pattern` — every matcher kind carries the field as a
/// non-optional `String`. This exhaustive match is what makes the subject set DERIVED rather than
/// listed: a new matcher kind cannot be added without this failing to compile, so no rule can enter
/// the packs outside this pin's view. (§5.5 of the working agreements: derive, never enumerate.)
fn file_pattern(matcher: &Matcher) -> &str {
    match matcher {
        Matcher::LineScan(m) => &m.file_pattern,
        Matcher::MethodScan(m) => &m.file_pattern,
        Matcher::SymbolScan(m) => &m.file_pattern,
        Matcher::IoScan(m) => &m.file_pattern,
        Matcher::CallScan(m) => &m.file_pattern,
        Matcher::LiteralScan(m) => &m.file_pattern,
    }
}

/// `(pack/rule, file_pattern)` for every rule in every loaded pack, in pack -> rule declaration order.
fn every_file_pattern(packs: &[RulePackDef]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pack in packs {
        for rule in &pack.rules {
            out.push((
                format!("{}/{}", pack.id, rule.id),
                file_pattern(&rule.matcher).to_owned(),
            ));
        }
    }
    out
}

/// The absolute floor on the subject set. Well below today's count (140 across 12 packs) so it never
/// churns, and far enough above zero that a broken extraction or a pack directory that stopped loading
/// fails LOUDLY instead of leaving this pin vacuously green over an empty set — the failure mode §5.5
/// names as worse than an uncaught defect, because green then gets cited as coverage.
const SUBJECT_FLOOR: usize = 100;

/// A character that can begin a directory or file NAME. `/` is deliberately excluded: a pattern
/// beginning `^/` anchors on a leading separator (an absolute-path spelling), which is a different
/// judgment from naming a top-level directory.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Whether the token at `p[0]` is made optional by the quantifier right after it.
fn optional_after(p: &[char], idx: usize) -> bool {
    matches!(p.get(idx), Some('?') | Some('*'))
}

/// Index of the `)` closing the group that opens at `p[0]`, tracking escapes and character classes so a
/// `)` inside `[)]` or after `\` is not mistaken for the close. `None` if unbalanced.
fn matching_paren(p: &[char]) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = 0usize;
    let mut in_class = false;
    while i < p.len() {
        match p[i] {
            '\\' => i += 1,
            _ if in_class => {
                if p[i] == ']' {
                    in_class = false;
                }
            }
            '[' => {
                in_class = true;
                // A `]` immediately after `[` or `[^` is a literal member, not the close.
                if p.get(i + 1) == Some(&'^') {
                    i += 1;
                }
                if p.get(i + 1) == Some(&']') {
                    i += 1;
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Index of the `]` closing the character class that opens at `p[0]`. `None` if unbalanced.
fn matching_bracket(p: &[char]) -> Option<usize> {
    let mut i = 1usize;
    if p.get(i) == Some(&'^') {
        i += 1;
    }
    if p.get(i) == Some(&']') {
        i += 1;
    }
    while i < p.len() {
        match p[i] {
            '\\' => i += 1,
            ']' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// How a group opening at `p[0]` should be read.
enum GroupHead {
    /// Body starts at this index: `(abc)`, `(?:abc)`, `(?i:abc)`, `(?<n>abc)`.
    Body(usize),
    /// Inline flag setting with no body — `(?i)`. Transparent: judge what follows the group.
    Flags,
    /// A form this analyzer does not model. Judged as NOT forcing a literal segment, so an unknown
    /// construct can never manufacture a failure it cannot justify.
    Unknown,
}

fn group_head(p: &[char], close: usize) -> GroupHead {
    if p.get(1) != Some(&'?') {
        return GroupHead::Body(1);
    }
    match p.get(2) {
        Some(':') => GroupHead::Body(3),
        Some('<') | Some('P') => match p[3..close].iter().position(|&c| c == '>') {
            Some(rel) => GroupHead::Body(3 + rel + 1),
            None => GroupHead::Unknown,
        },
        _ => {
            // `(?flags)` or `(?flags:body)`.
            let body = &p[2..close];
            if body.iter().all(|c| c.is_ascii_alphabetic() || *c == '-') {
                GroupHead::Flags
            } else {
                match body.iter().position(|&c| c == ':') {
                    Some(rel)
                        if body[..rel]
                            .iter()
                            .all(|c| c.is_ascii_alphabetic() || *c == '-') =>
                    {
                        GroupHead::Body(2 + rel + 1)
                    }
                    _ => GroupHead::Unknown,
                }
            }
        }
    }
}

/// Splits an alternation body on its top-level `|`, ignoring `|` inside nested groups, character
/// classes, or after an escape.
fn top_level_alternatives(body: &[char]) -> Vec<&[char]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_class = false;
    let mut i = 0usize;
    while i < body.len() {
        match body[i] {
            '\\' => i += 1,
            _ if in_class => {
                if body[i] == ']' {
                    in_class = false;
                }
            }
            '[' => {
                in_class = true;
                if body.get(i + 1) == Some(&'^') {
                    i += 1;
                }
                if body.get(i + 1) == Some(&']') {
                    i += 1;
                }
            }
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&body[start..]);
    out
}

/// Given the pattern text immediately AFTER an `^`/`\A` anchor, whether every way that anchor can be
/// satisfied begins with a literal directory/file NAME — i.e. whether the anchor locks the rule to the
/// tree root.
///
/// Deliberately answers `false` for:
/// * `|` / `)` / end — the anchor is one arm of an alternation. This is the `(?:^|/)` idiom the packs
///   converged on, and the whole point of the fix: a sibling arm admits a `/`-prefixed match.
/// * `^/` and `^\/` — anchoring on a leading SEPARATOR, not on a directory name.
/// * an optional first token (`^\.?/`, `^a?b/`) — judgment moves to what follows it.
/// * a class escape (`^\w`, `^\d`, `^\S`) or `.`/`[^…]` — a wildcard at the root still admits `src/…`,
///   so the rule is not silent there.
/// * a group where at least ONE alternative is permissive (`^(?:api/|.*)`) — one open arm is enough.
/// * any construct this analyzer does not model.
///
/// Answers `true` for a bare literal (`^api/`), an escaped literal (`^\.env`), a non-negated class of
/// name characters (`^[Aa]pi/`), and a group ALL of whose alternatives are themselves root-locked
/// (`^(?:domains/…|api/…)` — the exact shape that shipped).
fn forces_literal_segment(p: &[char]) -> bool {
    let Some(&c) = p.first() else {
        return false;
    };
    match c {
        '|' | ')' => false,
        '/' => false,
        '(' => {
            let Some(close) = matching_paren(p) else {
                return false;
            };
            match group_head(p, close) {
                GroupHead::Unknown => false,
                GroupHead::Flags => forces_literal_segment(&p[close + 1..]),
                GroupHead::Body(body_start) => {
                    if optional_after(p, close + 1) {
                        return forces_literal_segment(&p[close + 2..]);
                    }
                    let alts = top_level_alternatives(&p[body_start..close]);
                    alts.iter().all(|alt| forces_literal_segment(alt))
                }
            }
        }
        '[' => {
            let Some(close) = matching_bracket(p) else {
                return false;
            };
            if p.get(1) == Some(&'^') || p[1..close].contains(&'/') {
                return false;
            }
            if optional_after(p, close + 1) {
                return forces_literal_segment(&p[close + 2..]);
            }
            true
        }
        '\\' => {
            let Some(&next) = p.get(1) else {
                return false;
            };
            if next.is_ascii_alphanumeric() || next == '/' {
                return false;
            }
            if optional_after(p, 2) {
                return forces_literal_segment(&p[3..]);
            }
            true
        }
        _ if is_name_char(c) => {
            if optional_after(p, 1) {
                return forces_literal_segment(&p[2..]);
            }
            true
        }
        _ => false,
    }
}

/// Every root anchor in `pattern` that locks the pattern to the tree root, reported as the pattern text
/// following it (for the failure message). Both `^` and `\A` are treated as anchors; a `^` inside a
/// character class is class negation and is skipped, which is load-bearing — `[^/]+` ships today.
fn root_locked_anchors(pattern: &str) -> Vec<String> {
    let p: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut in_class = false;
    while i < p.len() {
        match p[i] {
            '\\' => {
                if !in_class && p.get(i + 1) == Some(&'A') && forces_literal_segment(&p[i + 2..]) {
                    out.push(p[i..].iter().collect());
                }
                i += 1;
            }
            _ if in_class => {
                if p[i] == ']' {
                    in_class = false;
                }
            }
            '[' => {
                in_class = true;
                if p.get(i + 1) == Some(&'^') {
                    i += 1;
                }
                if p.get(i + 1) == Some(&']') {
                    i += 1;
                }
            }
            '^' if forces_literal_segment(&p[i + 1..]) => out.push(p[i..].iter().collect()),
            _ => {}
        }
        i += 1;
    }
    out
}

/// Policy pin: no shipped DSL rule's `file_pattern` places a literal directory segment immediately
/// after a `^`/`\A` root anchor. See this module's own doc for the measured defect, why neither census
/// could see it, and exactly what the analyzer does and does not model.
///
/// There is no exemption list, and that is a decision rather than an omission: zero shipped patterns
/// need one today (every `^` in every shipped `file_pattern` is the `(^|/)` / `(?:^|/)` idiom), and an
/// empty exemption list is machinery guarding nothing. A rule that legitimately must anchor at the tree
/// root is a T3 — a deliberate divergence — and the exemption list gets born WITH its first entry and
/// its written reason, checked in both directions (`recognizer_drift.rs`'s
/// `no_exemption_names_a_module_that_no_longer_exists` is this repo's reference shape for that).
#[test]
fn no_shipped_file_pattern_anchors_a_literal_directory_segment_at_the_tree_root() {
    let packs = load_all_packs();
    let subjects = every_file_pattern(&packs);

    assert!(
        subjects.len() >= SUBJECT_FLOOR,
        "this pin found only {} shipped `file_pattern`(s) across {} pack(s) — it derives its subject \
         set from the loaded packs and expects at least {SUBJECT_FLOOR}, so a number this low means \
         pack loading or the extraction above is broken and the pin is vouching for NOTHING. Fix the \
         extraction; do not lower the floor to make this pass.",
        subjects.len(),
        packs.len(),
    );

    let offenders: Vec<String> = subjects
        .iter()
        .flat_map(|(who, pattern)| {
            root_locked_anchors(pattern).into_iter().map(move |anchor| {
                format!("  {who}\n    file_pattern: {pattern}\n    anchor: {anchor}")
            })
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "these rules' `file_pattern`s are anchored at the TREE ROOT by a literal directory segment \
         immediately after `^`, so they match `api/x.ts` but are structurally SILENT under `src/api/x.ts` \
         — the single most common project layout, and a silence the user never sees:\n{}\n\nWrite \
         `(?:^|/)` instead of `^` before a directory segment (`(?:^|/)api/…`, not `^api/…`), so the \
         segment matches at the tree root OR at any depth beneath it. This is the spelling every other \
         shipped path-anchored rule uses. If a rule genuinely MUST match only at the repository root, \
         that is a deliberate divergence (T3): add it to an exemption list here with the reason written \
         out, rather than weakening this check. Measured precedent: `sql/nplus1` shipped \
         `^(?:domains/[^/]+/routes/.+|api/.+)\\.ts$` and reported ZERO findings for identical N+1 code \
         placed under `src/api/`.",
        offenders.join("\n"),
    );
}
