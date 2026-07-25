//! Cross-boundary policy-vocabulary pins — one policy spelled twice because the boundary it straddles
//! admits no shared symbol, so the RELATIONSHIP between the two spellings is asserted here instead.
//! Every pin reads both sides from what actually ships (a hand-copied list in this file would just be a
//! third mirror to drift). Two boundaries so far: pack↔pack (a DSL rule cannot reference another pack's
//! pattern) and crate↔pack (a JSON pack cannot reference a Rust constant).

use regex::Regex;

use zzop_core::{Matcher, RuleDef, RulePackDef};
use zzop_parser_typescript::PROMISE_CONTINUATION_METHODS;

use crate::load_all_packs;

/// Finds a loaded pack by id, panicking with a clear message if it's missing — same "fail loudly" spirit as
/// `load_all_packs`.
fn find_pack<'a>(packs: &'a [RulePackDef], id: &str) -> &'a RulePackDef {
    packs
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("pack `{id}` not loaded"))
}

/// Finds a rule by id within a pack, panicking with a clear message if it's missing.
fn find_rule<'a>(pack: &'a RulePackDef, id: &str) -> &'a RuleDef {
    pack.rules
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule `{}/{id}` not loaded", pack.id))
}

/// Extracts a `Matcher::MethodScan` rule's `patterns[]` entry with the given `label`, panicking if the rule
/// isn't a method-scan rule or has no pattern with that label — both are authoring errors this pin exists to
/// catch, not conditions worth silently tolerating.
fn method_scan_pattern_by_label<'a>(rule: &'a RuleDef, label: &str) -> &'a str {
    match &rule.matcher {
        Matcher::MethodScan(m) => m
            .patterns
            .iter()
            .find(|lp| lp.label == label)
            .unwrap_or_else(|| panic!("{}: no patterns[] entry labeled `{label}`", rule.id))
            .pattern
            .as_str(),
        other => panic!("{}: expected a MethodScan matcher, got {other:?}", rule.id),
    }
}

/// Policy pin: `reliability/sync-fs-in-handler` and `db/client-per-request` both approximate "this
/// function looks like a request handler" with a `patterns[]` entry labeled `handler-context` — the SAME
/// evidence definition, deliberately duplicated across the two packs (a DSL rule can't reference another
/// pack's pattern). Nothing else stops one pack's copy drifting from the other's during an unrelated edit —
/// each pack's own fixtures only exercise its own copy, so a silent fork of what counts as "handler context"
/// (e.g. one pack keeping the naive `res` bare-word evidence a mono-hub 0.10.0 field review found false-
/// positives on, while the other adopts the tightened one) would ship unnoticed. This test loads both
/// shipped DSL packs fresh (via `load_dsl_packs`, same helper every other contract here uses — never a hand-
/// copied inline fixture), extracts each rule's own `handler-context` pattern string, and asserts they are
/// byte-identical, so a future edit to one without the other fails loudly here instead of drifting unnoticed.
#[test]
fn handler_context_pattern_is_identical_across_reliability_and_db() {
    let packs = load_all_packs();
    let reliability = find_pack(&packs, "reliability");
    let db = find_pack(&packs, "db");

    let sync_fs_rule = find_rule(reliability, "sync-fs-in-handler");
    let client_per_request_rule = find_rule(db, "client-per-request");

    let sync_fs_pattern = method_scan_pattern_by_label(sync_fs_rule, "handler-context");
    let client_per_request_pattern =
        method_scan_pattern_by_label(client_per_request_rule, "handler-context");

    assert_eq!(
        sync_fs_pattern, client_per_request_pattern,
        "reliability/sync-fs-in-handler and db/client-per-request's `handler-context` patterns have \
         drifted — they encode the same handler-evidence policy and must stay byte-identical (see this \
         test's own doc comment)"
    );
}

/// Extracts the members of a regex pattern's single property-alternation group — the `then|catch|finally`
/// of `\.(?:then|catch|finally)\s*\(` — in source order. Panics if the pattern carries no such group, or
/// more than one: either means the shape this pin reads its vocabulary out of has changed, which is
/// precisely the moment a human should look rather than a moment to silently pin nothing.
fn property_alternation_members(rule_id: &str, pattern: &str) -> Vec<String> {
    // Matches the LITERAL characters `\`, `.`, `(`, `?`, `:` as they appear inside the pattern string.
    let group = Regex::new(r"\\\.\(\?:([^)]*)\)").expect("static regex");
    let groups: Vec<&str> = group
        .captures_iter(pattern)
        .map(|c| c.get(1).expect("group 1 always participates").as_str())
        .collect();
    assert_eq!(
        groups.len(),
        1,
        "{rule_id}: expected exactly one `\\.(?:…)` property-alternation group in `{pattern}`, found \
         {} — this pin reads its vocabulary out of that group, so a changed shape must fail loudly \
         instead of pinning nothing",
        groups.len()
    );
    groups[0].split('|').map(str::to_owned).collect()
}

/// Policy pin (T2 — the boundary admits no shared symbol): the TypeScript parser's
/// `PROMISE_CONTINUATION_METHODS` and the continuation arm of `react/setstate-after-await-unmounted`'s
/// `async-boundary` pattern are ONE policy — "which member calls take a callback that runs on the RESUMED
/// continuation" — spelled twice only because a JSON pack cannot reference a Rust crate's constant.
///
/// They are consumed on different axes, and that difference is not divergence: the parser MERGES such a
/// callback's span into its call site's line (`extract_function_spans`), while the rule TREATS the token as
/// an async boundary. That is why the rule's pattern also carries a `\bawait\b` arm with no counterpart in
/// the constant — `await` schedules no callback, so it can never be a property name to merge. The mirrored
/// thing is the alternation group this test extracts, not the whole pattern.
///
/// Why it must not drift, and why nothing else would catch it: the rule only ever sees a continuation
/// boundary BECAUSE the parser merged the callback into the scheduling call's line. Drop `finally` from the
/// constant and `p.finally(() => setX(1))` still matches the rule's boundary token, but the setter now sits
/// alone in its own span, `after_in_same_function` rejects the pairing, and the finding disappears with
/// nothing turning red — the rule's fixtures exercise whatever spans the parser happens to produce, and the
/// parser's own tests never look at a rule. The safety-critical direction is therefore rule-arm ⊆ constant
/// (a violation buys silence); the reverse is only dormant capacity. Equality is pinned anyway, because it
/// is the one relation that makes a one-sided edit fail in BOTH directions.
///
/// Order is not pinned: alternation order and slice order are equally meaningless to their consumers
/// (a regex alternation and a `.contains()` lookup), so both sides are sorted before comparison.
#[test]
fn the_promise_continuation_vocabulary_is_identical_in_the_parser_and_the_react_pack() {
    let packs = load_all_packs();
    let react = find_pack(&packs, "react");
    let rule = find_rule(react, "setstate-after-await-unmounted");
    let boundary = method_scan_pattern_by_label(rule, "async-boundary");

    let mut from_pack = property_alternation_members(&rule.id, boundary);
    let mut from_parser: Vec<String> = PROMISE_CONTINUATION_METHODS
        .iter()
        .map(|m| (*m).to_owned())
        .collect();
    from_pack.sort();
    from_parser.sort();

    assert_eq!(
        from_pack, from_parser,
        "the promise-continuation vocabulary has forked: react/setstate-after-await-unmounted's \
         `async-boundary` pattern accepts {from_pack:?} as continuation boundaries while the TypeScript \
         parser's PROMISE_CONTINUATION_METHODS merges {from_parser:?} — a token the rule accepts but the \
         parser does not merge leaves the continuation callback in its own function span, so \
         `after_in_same_function` silently drops every finding on that shape (see this test's own doc)"
    );
}

/// The one wording of `after_in_same_function`'s per-LINE residual that every rule carrying the gate must
/// splice into its message VERBATIM. Sealed as a whole SENTENCE rather than a keyword because no shorter
/// token is unique — "function span" occurs in unrelated prose across these packs, so a token pin would
/// pass on a sentence about something else entirely. Same reasoning `marker_window_phrase`
/// (`rules/native/rules-http/src/http_scan.rs`) records for its own phrase, which is this pin's precedent
/// in every respect except one: that phrase can be a Rust `fn` because both its consumers are Rust.
const ORDER_GATE_RESIDUAL: &str = "A trigger line that sits inside NO parser-projected function span \
     (a class-property initializer, a top-level statement) is read as ungated, and the pairing there \
     widens back to the whole scanned symbol span.";

/// Policy pin (T2 — the boundary admits no shared symbol): every DSL rule that turns
/// `MethodScan::after_in_same_function` ON must publish that gate's RESIDUAL in its own message, and all
/// of them must publish it in the SAME words.
///
/// The residual is a documented contract, not a rounding error. `method_scan.rs`'s
/// `innermost_function_start(abs_line).unwrap_or(0)` says so in place: a trigger line the parser projected
/// into NO function span reads as "no gate on this line", never as "no pair", so the floor drops to 0 and
/// every earlier ordering match in the scanned symbol span is readmitted — the pre-gate scope. It degrades
/// per LINE, not per file: a file WITH spans still has lines outside all of them (a class-property
/// initializer, a top-level statement), which `rules/dsl/react/setstate_after_await_unmounted.rs`'s
/// `a_class_property_setter_outside_every_function_span_keeps_the_pre_gate_pairing` pins as live, intended
/// behavior.
///
/// Why it must be said at all: without this sentence each of these messages asserts its "same function
/// means the NEAREST enclosing function, so a sibling closure does not pair" claim UNCONDITIONALLY —
/// wider than the matcher proves, and a reader holding one finding cannot falsify it. Nothing else would
/// catch the drift: a rule's own fixtures assert findings, never the sentence describing them.
///
/// Why a byte-identical sentence spliced seven times instead of one shared symbol: a rule `message` is not
/// a pattern-bearing field, so the fragment mechanism structurally cannot reach it
/// (`docs/contracts/rule-pack.schema.json` — a fragment reference resolves only in fields whose
/// description ends "fragment reference supported"), and the rules live in five separate packs that share
/// no vocabulary anyway. Same boundary `handler_context_pattern_is_identical_across_reliability_and_db`
/// straddles, same remedy: spell it out on each side and pin the RELATIONSHIP here.
///
/// Two-sided by construction: the subject set is DISCOVERED from the shipped matchers, never listed here,
/// so an eighth rule that switches the gate on and keeps a silent message fails on its first run.
#[test]
fn every_after_in_same_function_rule_publishes_the_order_gate_residual() {
    let packs = load_all_packs();

    let mut gated: Vec<String> = Vec::new();
    let mut silent: Vec<String> = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let Matcher::MethodScan(m) = &rule.matcher else {
                continue;
            };
            if !m.after_in_same_function {
                continue;
            }
            let name = format!("{}/{}", pack.id, rule.id);
            if !rule.message.contains(ORDER_GATE_RESIDUAL) {
                silent.push(name.clone());
            }
            gated.push(name);
        }
    }

    assert!(
        !gated.is_empty(),
        "no shipped rule turns on `after_in_same_function` — this pin reads its subject set off the \
         shipped matchers, so an empty set means it is silently vouching for nothing"
    );
    assert!(
        silent.is_empty(),
        "these `after_in_same_function` rules state their \"same function\" scope unconditionally, \
         without publishing the gate's per-line residual: {silent:?} (of {gated:?}). Splice this \
         sentence VERBATIM into each message — see this test's own doc for why it is spelled out rather \
         than shared: {ORDER_GATE_RESIDUAL}"
    );
}
