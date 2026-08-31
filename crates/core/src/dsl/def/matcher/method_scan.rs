//! `Matcher::MethodScan`'s shape — the multi-pattern, same-span co-occurrence matcher and its
//! structural gates (`trigger_in_loop` / `after` / `after_in_same_function` / `require_call_kind`) plus
//! its two vetoes: `absent`, scoped to the whole body, and `trigger_call_exclude_pattern`, scoped to
//! the trigger call's own parentheses.
//! Split out of the parent `def/matcher.rs` for the same reason that file was split out of `def/mod.rs`:
//! the repo's per-file line cap. This one struct's field docs carry the measured evidence behind every
//! gate (why co-occurrence was not enough, what each gate deletes, what it degrades to on a parser that
//! projects no spans) and ran to four fifths of the parent file; the split follows the seam the
//! EVALUATOR side already uses (`dsl/line_scan.rs` / `dsl/method_scan.rs` / `dsl/ir_scan.rs`), so a
//! reader looking for method-scan finds its shape and its evaluation under the same name.
//!
//! `def/mod.rs` re-exports `MethodScan` unchanged, so `zzop_core::dsl::MethodScan` is unaffected.

use serde::Deserialize;

use super::LabeledPattern;

/// Multi-pattern co-occurrence within a symbol's body span (e.g. a command-injection detector requiring
/// `Runtime.exec`/`ProcessBuilder` to co-occur with string concatenation in the *same* method). Every
/// pattern in `patterns` must match somewhere in the span; `trigger` anchors the finding's line + snippet.
/// Spans come from `SourceFile.symbols`, projected by the parser; files with no symbols are skipped.
#[derive(Debug, Clone, Deserialize)]
pub struct MethodScan {
    /// Target file-path regex (e.g. `(?i)\.java$`).
    pub file_pattern: String,
    /// Cheap pre-skip: only scan a file whose text contains this regex (if absent, always scan).
    #[serde(default)]
    pub require_file: Option<String>,
    /// Additional pre-skip regexes, ALL of which must match the file text (see `LineScan::require_file_all`).
    #[serde(default)]
    pub require_file_all: Vec<String>,
    /// Negated mirror of `require_file_all` — see `LineScan::require_file_absent` (e.g. `process.exit(...)`
    /// with no `process.on('SIG...` signal-handling registration anywhere in the file).
    #[serde(default)]
    pub require_file_absent: Vec<String>,
    /// Skip lines whose trim_start begins with `//` `*` `/*` (comments) when testing any pattern.
    #[serde(default)]
    pub skip_comment_lines: bool,
    /// Mask closed string-literal interiors on each line to spaces before testing any `patterns`/`absent`
    /// regex, so a token inside a string literal (a code-gen template like `'process.exit(2)'`, an example
    /// in a docstring) does not false-fire. The original line is kept for the snippet + `marker_suppresses`.
    /// Opt-in per rule (default `false` = byte-identical to today) — see `LineScan::strip_string_literals`
    /// and `crate::dsl::string_mask::mask_string_literals`.
    #[serde(default)]
    pub strip_string_literals: bool,
    /// All of these must match somewhere within a symbol's body span for a finding.
    pub patterns: Vec<LabeledPattern>,
    /// `patterns[].label` whose first match (top-down) supplies the finding's line + snippet.
    pub trigger: String,
    /// Structural containment gate on the trigger pattern: when `true`, a trigger-pattern line match
    /// only counts (for both satisfaction and the finding's line) if it falls within one of the file's
    /// `SourceFile::loop_spans` — i.e. the call is textually INSIDE a loop statement or an
    /// array-iteration callback body, not merely co-occurring with loop tokens somewhere in the same
    /// function (the co-occurrence approximation behind the mono-hub 11/11 api-in-loop FP class).
    /// Non-trigger patterns are unaffected. A file with no projected loop spans (external parser,
    /// lexical fallback) can never satisfy the trigger, so the rule is silent there — graceful degrade,
    /// same policy as method-scan on a file with no symbol spans.
    #[serde(default)]
    pub trigger_in_loop: bool,
    /// Lexical-ORDER gate on the trigger pattern: names another `patterns[].label` that must already have
    /// matched BEFORE a trigger match counts (for both satisfaction and the finding's line). "Before"
    /// means textually earlier within the same span — an earlier line, or an earlier start offset on the
    /// SAME line (so a one-liner like `p.then(r => setX(r))` counts, with `.then(` preceding `setX(`).
    ///
    /// SAME-LINE NESTING RESIDUAL — the negative direction of that same offset comparison, and the one a
    /// new rule author has to price in before setting this field. `order_ok` (`dsl/method_scan/gates.rs`)
    /// compares the two patterns' FIRST-MATCH start offsets and nothing else, so it cannot tell a callee
    /// that merely follows the trigger from one nested INSIDE the trigger's own argument list — which
    /// evaluates FIRST. `p.then(r => setX(r))` counts because the boundary token happens to start first;
    /// invert the nesting and the finding disappears. Measured, not hypothesised:
    /// `await redis.set(key, Number(await redis.get(key)) + 1)` (`redis/counter-get-set`, a genuine
    /// lost-update read-modify-write) and `setData(await fetch(url));`
    /// (`react/setstate-after-async-unguarded`, silent while the two-statement spelling on the next
    /// fixture fires) are both dropped. This is structural, not a bug to fix here: separating "nested
    /// argument" from "later statement" needs nesting awareness two independent regexes over a flat line
    /// do not have. The policy is therefore DISCLOSE, not repair — every rule whose trigger regex can
    /// span a nested call publishes the residual in its own message (`redis/counter-get-set`,
    /// `redis/lock-get-then-set`, `react/setstate-after-async-unguarded`,
    /// `db/find-then-create-no-unique`, `db/check-then-act-in-loop`, `reliability/fs-check-then-use`,
    /// `sql/race-condition-toctou`). A trigger regex that CANNOT span a nested call is immune by
    /// construction and stays silent about it: `db/non-atomic-counter-update`'s trigger is
    /// `<ident>: <word-or-dots> ± 1`, which no nested call fits, so the inline form fails the TRIGGER and
    /// never reaches this gate at all.
    ///
    /// This is the ORDER counterpart of `trigger_in_loop`'s containment gate, and exists for the same
    /// reason: a rule whose ID or message asserts a SEQUENCE ("setState after await") must not settle for
    /// plain co-occurrence, which is satisfied just as well by a setter that runs BEFORE the await. Without
    /// it, `patterns` proves only "both tokens appear somewhere in this span", and `trigger` anchors on the
    /// FIRST trigger match — routinely a line before the ordering token (measured on mono-hub: 9 of a
    /// 15-finding `react/setstate-after-async-unguarded` sample anchored before the first `await` in the
    /// whole file). Setting `after` fixes the anchor as a side effect: the finding lands on the first
    /// trigger match that actually follows, not on the first one anywhere.
    ///
    /// What it proves and what it does NOT: lexical order in the source text, not execution order. A
    /// trigger inside an `else` branch, or in a callback declared after the ordering token but invoked
    /// before it, still counts. It is a strictly stronger claim than co-occurrence and a strictly weaker
    /// one than dataflow — a rule using it should say "lexically after" in its message. Naming a label
    /// that no `patterns` entry declares is malformed and skips the rule, exactly like `trigger`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Structural PAIRING gate on `after`: when `true`, the ordering label's match must fall INSIDE the
    /// innermost `SourceFile::function_spans` entry containing the trigger — not merely inside the same
    /// symbol body span. No-op without `after`.
    ///
    /// Containment, NOT span identity. An enclosing function's own `await` stays visible to a trigger in
    /// that same function even when the `await`'s line is also covered by a nested merged continuation
    /// span (`await import(m).then((x) => x.f());` on one line, the setter on the next). Identity
    /// matching lost exactly that shape on mono-hub; a sibling closure's boundary is still rejected,
    /// since a sibling's lines are outside the trigger function's span by construction.
    ///
    /// Why it exists: a method-scan span is a DECLARED symbol's body, so a React component's whole
    /// function is one span and every anonymous closure inside it shares that scope. `after` then pairs a
    /// setter in one closure with an `await` in an unrelated SIBLING closure — measured as 4 of a
    /// 15-finding `react/setstate-after-async-unguarded` sample. This gate requires the two matches to be
    /// in the same function, which is the scope an async continuation actually resumes into.
    ///
    /// Why it needs a PARSER fact and not just "nearest function": the naive partition splits a promise
    /// continuation away from the `.then(` that schedules it, destroying the 11 measured true positives
    /// of exactly that shape. `function_spans` merges a `.then`/`.catch`/`.finally` callback into its
    /// call-site line for this reason — see that field's doc for the merge rule and its narrowness.
    ///
    /// LINE granularity, same as `trigger_in_loop`: a trigger whose line also opens an inline callback
    /// resolves to THAT callback's span even when the trigger token itself is outside it. Measured
    /// consequence on mono-hub: `setFiles((prev) => [...prev, ...entries]);` inside an async function
    /// stops anchoring, and the finding re-anchors on the next plain setter in the same function — the
    /// finding survives, the reported line moves by one. An accepted imprecision, not a silent drop.
    ///
    /// Absent fact (`function_spans` empty — non-TypeScript, external parser, lexical fallback): every
    /// line resolves to `None`, so all lines count as the same function and the gate is a NO-OP, leaving
    /// the rule's pre-gate behavior intact. That is the opposite direction from `trigger_in_loop`, which
    /// silences its rule on a file with no spans: this gate only ever REMOVES pairings, so degrading to
    /// "no removal" keeps coverage rather than dropping it.
    ///
    /// The degrade is per LINE, not only per file, and that is deliberate: a trigger line inside no
    /// projected span is read as "no gate on this line", never as "no pair". `None` is absence of
    /// EVIDENCE, not evidence of separation — this gate only deletes pairings the projection PROVES sit
    /// in different functions, so treating a silent projection as such a proof would delete real
    /// findings on the strength of a missing fact. Reachable with spans PRESENT: a class body's own top
    /// level. A class symbol's body span is scanned whenever the class declares no
    /// method/constructor/function-property sub-symbol — since 2026-08-09 function-VALUED properties
    /// project their own leaves (TS `symbol_shapes::emit_class`), so the surviving case is a class of
    /// NON-function initializers (e.g. call-valued fields), where an initializer line sits inside no
    /// function span and a setter on it still pairs with a `.then(` boundary in a sibling field's
    /// initializer, exactly as before the gate. An external parser that projects spans only partially
    /// lands in the same place. Pinned by
    /// `a_class_property_setter_outside_every_function_span_keeps_the_pre_gate_pairing`
    /// (`rules/dsl/react`).
    #[serde(default)]
    pub after_in_same_function: bool,
    /// After every `patterns` entry is satisfied, the finding is vetoed if ANY of these also matches a
    /// line in the SAME span — e.g. a try/catch guarding a TOCTOU race, or a `$transaction(...)` wrapper.
    #[serde(default)]
    pub absent: Vec<LabeledPattern>,
    /// The NARROW veto [`Self::absent`] is the wide one: a trigger match does not count when this
    /// pattern matches inside the TRIGGER CALL'S OWN PARENTHESES — the match's line plus the
    /// continuation lines its unclosed parens hold open, capped, matched in multi-line mode
    /// (`crate::dsl::veto_window::trigger_call_excluded`). Like `trigger_in_loop` and `after`, a
    /// vetoed trigger match neither satisfies the trigger nor anchors the finding, so a SECOND,
    /// unmitigated trigger call elsewhere in the same span still fires.
    ///
    /// Why a second veto field rather than another `absent` entry: `absent` reads the whole symbol
    /// BODY, which is the wrong scope for "this call's argument was checked". Measured on cal.com, where
    /// 17 `res.redirect(` sites have their argument opened by the project's own origin-allowlist
    /// helper (`security/open-redirect`: 28 findings before this field, 15 after): a body-scoped `absent` on the same
    /// vocabulary also waives a redirect whose neighbour statement merely LOGS a sanitized copy of the
    /// URL (`sanitizeUrlForLog`, measured), and it cannot tell a mitigator that wraps the target from
    /// one that wraps something else entirely. This field can, because the text it reads is bounded by
    /// the call.
    ///
    /// The WINDOW is why this is not `LineScan::exclude_pattern` in method-scan clothing: 16 of those
    /// 17 have the mitigator on a CONTINUATION line, inside the still-open `redirect(`, where a
    /// formatter put it. A line-local veto would have cleared 1 of the rule's 34 corpus findings.
    ///
    /// WHAT THE FIELD PROVES IS NOT THE FIELD'S BUSINESS, and getting that backwards is how the first
    /// version of `security/open-redirect`'s pattern shipped a false NEGATIVE. All this field supplies
    /// is the TEXT the call's own parentheses hold; the pattern decides what a match in it means. A
    /// pattern that stops at a guard call's opening `(` has proved only that the guard OPENS the
    /// argument, and a raw request value concatenated or interpolated after it is then waived by a
    /// veto that never looked. A rule claiming the guard produces the WHOLE value must spell the
    /// value's closure too — `redirect(safe(x) + req.query.q)`, and its template-literal twin where a
    /// second interpolation follows the guard's, are the shapes that separate the two claims. The
    /// repo's older spelling of the same closure is `browser.json`'s `${html-sink-sanitized}`
    /// fragment, which requires the sanitizer's balanced argument list to be followed by `[;,)}]|$`.
    ///
    /// What a rule setting this may CLAIM is bounded by what the window can see, and the residuals are
    /// the same ones `CallScan::line_exclude_pattern` carries plus one of its own: the cap
    /// (`veto_window::MAX_CALL_WINDOW_LINES`), a comment anywhere in the argument list ending the
    /// window there, a mitigator applied on a PRECEDING statement or by a wrapper being out of reach by
    /// construction, and — new here — a line carrying TWO trigger matches declining the window
    /// entirely. Every one of those leaves the site FIRING; this field only ever SUPPRESSES, so absence
    /// of evidence must never become evidence of a waiver.
    #[serde(default)]
    pub trigger_call_exclude_pattern: Option<String>,
    /// Structural PRESENCE gate over the projected call-site channel: when set, the symbol span must
    /// additionally contain at least one `SourceFile::call_sites` entry of exactly this `kind` (the
    /// site's own line within `body_start..=body_end`). It is the CO-OCCURRENCE axis a lexical
    /// `patterns` arm used to approximate with a bare token — W3's "structure only the exec witness,
    /// keep the co-occurrence evidence lexical": the rule's `trigger`/`patterns`/`absent` clauses stay
    /// regexes, and only the "did this method really use the process API" half becomes a parser fact
    /// (so a variable NAMED `exec`, or a mention in a string/comment, no longer counts as the witness).
    ///
    /// Degrade direction: SILENCE, `trigger_in_loop`'s family — the gate ALLOWS a finding on evidence,
    /// so a file with no projected call sites (degraded parse, external parser, lexical fallback, or a
    /// spelling the producer cannot resolve — a variable receiver like `rt.exec(cmd)`) can never satisfy
    /// it and the rule says nothing there. A rule setting this field trades exactly that recall for the
    /// bare-token false-positive class, and must disclose the trade in its own message. The kind
    /// spelling is bound by `RULE_READ_CALL_KINDS` via `call_kind_readers.rs`, the same contract
    /// `CallScan::kind` is under.
    #[serde(default)]
    pub require_call_kind: Option<String>,
    /// Optional path regex — a file whose `rel` path matches this is skipped entirely. Same rationale as
    /// `LineScan::file_exclude_pattern`.
    #[serde(default)]
    pub file_exclude_pattern: Option<String>,
    /// Max snippet length (truncates long lines).
    #[serde(default = "super::default_snippet_max")]
    pub snippet_max: usize,
}
