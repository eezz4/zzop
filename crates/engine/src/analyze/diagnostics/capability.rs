//! Capability self-reports, one fixed message shape each: git-not-requested, zero-DSL-packs,
//! dead-rule (uncompilable OR structurally empty), and the per-extension "bring an adapter"
//! disclosure. Message strings are a user-visible contract (docs and tests pin their shape) — extend
//! in the existing voice, never rewrite the pinned head. The two pack-APPLICABILITY reports and the
//! census behind them live in the sibling `pack_scope` module.
//!
//! Reach differs by report: the dead-rule one is config-derived, so it fires on `analyze_envelope`
//! too (as do both of `pack_scope`'s); git-not-requested and the per-extension disclosure need a
//! filesystem walk that path never performs. `docs/modules/facade.md` states that split for consumers.

use std::collections::BTreeMap;

use crate::EngineConfig;

/// Capability self-report: git history was never requested (`config.git` is `None`), so every
/// git-derived output channel is null. Distinct from `collect_git`'s own warning, which fires only when
/// git WAS requested but collection failed — a consumer can always tell "never asked" apart from
/// "asked, failed" by which of the two strings is present. Returns `None` when git was requested.
pub(in crate::analyze) fn git_not_requested_warning(config: &EngineConfig) -> Option<String> {
    if config.git.is_some() {
        return None;
    }
    Some(
        "git history not requested (git option omitted): scores, health, recommendations, criticality, seams and layerCoChurn are null. Pass git: {} to enable them."
            .to_string(),
    )
}

/// §0 disclosure for a Spring Security config this build FOUND, READ, and then deliberately declined to
/// draw a posture from — one line per such file, naming the bail. Empty when no Java file got past
/// "not a security config at all", which is every tree that has no Spring Security config.
///
/// # Why this exists, and why its absence was the defect
/// `extract_spring_security_posture` is parse-all-or-nothing ON PURPOSE: exempting a route wrongly hides
/// a real finding, so an unrecognized clause yields NO posture and every route keeps reporting as
/// unguarded. That refusal is correct and is not what changed here. What was wrong is that the refusal
/// was SILENT: the extractor has always returned a NAMED bail so this report could exist, and until
/// 2026-09-05 nothing read it, so the reply said "no auth evidence" about routes whose auth config had
/// been located and parsed. Measured on macrozheng/mall (2026-09-05): its chain ends
/// `.access(mgr == null ? authenticated() : mgr)` — `any-request-access-not-provable`, because the live
/// arm could GRANT — and 114 of its 127 `mutating-route-no-auth` findings ride that silence.
///
/// The message states the DIRECTION of the consequence rather than leaving the reader to infer it: this
/// bail makes findings appear, never disappear. A reader who thinks a missing posture might have
/// SUPPRESSED something has the safety property backwards, and a disclosure that leaves that open invites
/// the wrong repair.
pub(in crate::analyze) fn spring_posture_bail_warnings(
    bails: &[(String, &'static str, String)],
) -> Vec<String> {
    bails
        .iter()
        .map(|(file, name, detail)| {
            // The DETAIL is the actionable half, rendered rather than folded into the name because
            // several bail families hold many members that take entirely different work to support:
            // `lambda-body` alone covers if_statement, chain-not-on-parameter, arguments, body,
            // parameters and expression_statement. The first version of this warning printed the name
            // alone and shipped that way for exactly one commit, which sent its reader to open the
            // config and guess which member they hit — the guessing the bail enum exists to end.
            // Empty for the variants that carry no payload; those already name one shape.
            let shape = if detail.is_empty() {
                String::new()
            } else {
                format!(", at: {detail}")
            };
            format!(
                "Spring Security config read but NOT applied: {file} ({name}{shape}). This build located an \
                 authorization chain in that file and declined to derive a route-auth posture from it, because \
                 deriving one requires proving the chain is authenticated-by-default AND recognizing every clause \
                 that configures it — anything less could clear a route that is actually open. NOTHING WAS \
                 SUPPRESSED BY THIS: with no posture, every route stays unexempted, so route-auth findings here \
                 are MORE numerous than they would be with the config understood, never fewer. To clear the \
                 routes this config really does guard, inject the `auth-guarded` attribute for them through an \
                 adapter overlay (Mode B) — the same channel a recognized guard writes to."
            )
        })
        .collect()
}

/// Capability self-report: no DSL rule packs are loaded (`config.packs` is empty), so only the built-in
/// native analyses ran. `pub(crate)` because it is shared between `assemble` and
/// `envelope::analyze_envelope`, which gate DSL packs identically on `config.packs`. Per this codebase's
/// kernel-agnostic-no-rule-data principle the message names no rule/vocab, only the native-analysis
/// count and the `packsDir` config hint. Returns `None` when at least one pack is loaded.
pub(crate) fn zero_packs_warning(config: &EngineConfig) -> Option<String> {
    if !config.packs.is_empty() {
        return None;
    }
    let mut registry = zzop_core::RuleRegistry::new();
    crate::register_all_native(&mut registry);
    let native_count = registry.ids().len();
    Some(format!(
        "no DSL rule packs loaded: only the {native_count} built-in native analyses ran. If you expected the bundled packs, reinstall/check the package (the bundled packs directory may be missing); to add your own, set `packs: {{ extraDirs: [...] }}` in zzop.config.jsonc (embedders: `packsDir`)."
    ))
}

/// Every loaded rule whose own pattern will not compile, as one warning line each. A rule like that is
/// not "quiet" — it is DEAD: the evaluator skips it and the run reports clean, which is exactly the
/// misleading-diagnosis failure this engine refuses to commit. `validate-rule-pack` catches it ahead of a
/// scan, but nothing forces that call: an inline `packDefs` entry, a `packsDir` pack, or a bundled pack
/// all reach the evaluator unvalidated.
///
/// Derived from `zzop_core::pack_regex_issues`, the same judgment `validate-rule-pack` reports (regex
/// fields AND the two structural dead-rule shapes), over the LOADED pack set — so it names a rule that
/// can never fire even when no file this run visited would have matched its `file_pattern` anyway.
/// (`zzop_core::dsl::RuleDiag` reports the narrower run-scoped fact from the compile sites themselves;
/// see `dsl::eval_pack`'s doc for why the shipped engine discloses at load scope instead.)
///
/// `packs` is the LOADED set, before `disabled_rules` gating — the same convention `compute_dsl_scope`
/// documents. So a rule inside a wholly disabled pack is still named here: the message says only that
/// THIS rule cannot fire, never that the rest of the pack is running.
///
/// One line per bad FIELD, not per rule: a rule with two uncompilable patterns produces two lines, each
/// naming its own field. That is the shape `validate-rule-pack` has always emitted and what an author
/// fixing patterns one at a time wants.
pub(crate) fn uncompilable_rule_warnings(packs: &[zzop_core::RulePackDef]) -> Vec<String> {
    packs
        .iter()
        .flat_map(zzop_core::pack_regex_issues)
        .map(|issue| {
            format!(
                "{issue} — that rule is SKIPPED and can never fire; the pack's other rules are unaffected by it. Catch this before a scan with `zzop validate-rule-pack <pack.json>` (CLI binary) / the `validate_rule_pack` MCP tool."
            )
        })
        .collect()
}

/// Capability self-report: the "bring an adapter" per-extension disclosure — one line per distinct
/// extension among files `dispatch::dispatch` returned `None` for, that are not a non-source extension
/// (`dispatch::is_non_source_extension` — question 1 of `dispatch::NonSourceKind`'s doc, and the ONLY
/// question this channel asks; "was anything lost" is question 2, answered by the coverage-gap surfaces
/// through `dispatch::extraction_can_lose_facts`, and the two lists differ on purpose) and not already
/// covered by an adapter overlay (the overlay IS the
/// parser for those; see `analyze::assemble`'s collection site for the overlay-exclusion rationale). Before
/// this change, such a file vanished from every self-report: `degraded: false`, no `io`/symbols, extension
/// recorded nowhere — this closes that gap without naming a rule/language vocabulary, only a raw extension
/// and a count. `unparsed` must already carry each extension's TOTAL count in `.0` and its first (in
/// artifact-visitation, i.e. `rel`-sorted) up-to-3 sample paths in `.1` — the caller (`analyze::assemble`)
/// caps the sample during collection rather than here, so a huge tree never holds more than 3 rels per
/// extension in memory. No-extension files (README, Dockerfile) are deliberately excluded from
/// `unparsed` altogether by the collection site, not here — see that site's own doc for why (ambiguous by
/// construction: often config/docs, no reliable language signal).
///
/// ## The order is UNREAD-COUNT-descending, not extension-ascending
/// These lines used to come out in `BTreeMap` key order, which is deterministic but ranks the gaps by
/// how their extension is SPELLED. Nothing is dropped either way — this channel's own decision record
/// rejects shortening it — so the whole remaining cost of the disclosure is which end of it a reader
/// reaches first, and a name sort spends that on an accident. Measured on the dogfood corpus: nocodb's
/// `.vue` line (962 unread files) was the 42nd of 49 `warnings` entries, under eight extensions of 1-10
/// files each, because "v" sorts last; koel's `.php` (1412 files) sat 10th, one line below a single
/// `.psd`; immich's `.svelte` (415) sat 25th of 38.
///
/// The key is the count each entry ALREADY carries — no roster of interesting extensions to fall out of
/// date, and an extension this build learns about tomorrow is ranked by the same arithmetic as the rest.
/// Ties break on the extension name, so the order is TOTAL: no pair is left to insertion order, and two
/// runs over one map stay byte-identical (`two_calls_over_the_same_map_are_byte_for_byte_identical`).
///
/// It is a reading order, NOT a severity: a large unread count means this run read less of the tree, not
/// that the tree is worse. The channel still states one fact per extension and judges none of them.
///
/// The sibling `coverageGaps` table keeps its extension-ascending order deliberately — it is floor-gated
/// to a handful of principal filetypes, so there is no first-screen to lose there, and it is pinned that
/// way by `coverage_gaps_tests`. Two surfaces of one subject, ordered for the two different problems
/// they have.
///
/// ## One fact line per extension, ONE guidance line per run
/// The adapter on-ramp ([`adapter_on_ramp_note`]) is emitted ONCE, as the last entry, instead of being
/// repeated inside every per-extension line. A field run on a repo with `.env.development`/`.env.example`/
/// `.env.production`/`.sh` printed the entire four-sentence prescriptive tail four times over — the same
/// remedy restated until it read as noise, which is how a genuine capability gap loses the reader. The
/// per-extension entries keep only their own facts (count, extension, sample paths); the funnel is not
/// weakened, only de-duplicated — see [`adapter_on_ramp_note`] for the reachability contract it carries.
pub(in crate::analyze) fn unparsed_extension_warning(
    unparsed: &BTreeMap<String, (usize, Vec<String>)>,
) -> Vec<String> {
    if unparsed.is_empty() {
        return Vec::new();
    }
    // Biggest unread population first, name breaking the ties. `BTreeMap::iter` is already
    // name-ascending, and `sort_by_key` is stable, so the tie-break needs no second comparator.
    let mut ordered: Vec<(&String, &(usize, Vec<String>))> = unparsed.iter().collect();
    ordered.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    let mut out: Vec<String> = ordered
        .iter()
        .map(|(ext, (count, sample_rels))| {
            let mut sample_str = sample_rels.join(", ");
            if *count > sample_rels.len() {
                sample_str.push_str(&format!(", +{} more", count - sample_rels.len()));
            }
            format!(
                "{count} file(s) with extension .{ext} have no native parser — no io/symbol facts were \
                 extracted from them: {sample_str}."
            )
        })
        .collect();
    // The note SAMPLES this same order rather than re-deriving one: the five it names must be the five a
    // reader has just read, or the one line a skimmer does read points away from the largest gap.
    out.push(adapter_on_ramp_note(
        &ordered
            .iter()
            .map(|(ext, _)| ext.as_str())
            .collect::<Vec<_>>(),
    ));
    out
}

/// Extensions named inline in the single on-ramp note before it collapses to a `+N more` count — the note
/// points at the per-extension entries above it, so it never needs the full list. Which five it names is
/// not this constant's business: it takes the FIRST five of the order its caller emitted, so the sample
/// and the entries can never disagree about which gaps are the big ones.
const ON_RAMP_EXT_SAMPLE: usize = 5;

/// The gap-to-creation funnel, stated once per run (`output-philosophy`, §2 capability gaps): a gap must
/// not end at disclosure — it chains the reader to BUILDING an adapter, and the default on-ramp is a
/// minimal Mode B overlay, never a full parser. Every named surface must be one a reader can actually
/// reach, in BOTH dialects (a binary-only MCP user has no `examples/` or `docs/` checkout, so each repo
/// path carries an embedded-contract twin): the guide (`contract envelope-guide`), the checker
/// (`validate_envelope` / `zzop validate-envelope`), and a runnable example (`contract example-envelope`).
/// Dropping one is a partial-claim regression; naming an unreachable one is the same regression in the
/// other direction (the removed napi `analyzeEnvelope` binding is deliberately absent).
///
/// ## The forwarding address is SCOPED, and the anecdote behind it does not ride the wire
/// This string is built in the engine and reaches every lane that walks a tree, so a bare "…rides the
/// `coverageGaps` field of this same reply" was true on only some of them: that field is the analyze
/// SHAPER's invention (`zzop_summary::analyze::shape`), and neither `zzop_summary::cross`'s per-source
/// entries (which forward `warnings` and `coverage`, not this) nor the raw `zzop-facade` output has it.
/// A reader on those lanes followed a name that is not in their reply — the same failure the sentence
/// exists to prevent, one level up. Naming the coverage view's `unreadExtensions` instead does not fix
/// it either: that reply has no MCP twin, and `unparsed_extension_tests`'s CLI-vocabulary leg holds
/// this file to it. So the pointer states WHICH reply carries the population and says plainly that the
/// others do not.
///
/// The measurement that justifies the exclusion (macrozheng/mall: this line named 8 extensions /
/// 21 files of mind-maps and binaries while 114 `.xml` mappers holding 906 SQL statements went
/// unnamed) was **170 bytes** of one foreign repository's statistics paid on EVERY reply that hits
/// this path, and the clause carrying it went from 345 bytes to 188. It has two owners in the source
/// already — `dispatch::NonSourceKind`'s doc and `unparsed_extension_tests`' own — and a third copy on
/// the wire bought the reader nothing the surrounding sentence does not say without it. What stays on
/// the wire is the CLAIM (the count is not the tree's unread filetypes, and here is what it leaves
/// out); what left is the evidence for it, which belongs where someone changing this code will read
/// it. Pinned by
/// `unparsed_extension_tests`.
///
/// `ordered` is the extension list IN THE ORDER the fact lines above were emitted (unread-count
/// descending — see [`unparsed_extension_warning`]), not the raw map: this note takes its sample off the
/// front of that list so the five it names are the five the reader just passed. Taking them off the map
/// instead put the five alphabetically-first extensions in the one line a skimming reader does read,
/// which on nocodb meant naming `.bash`/`.bats`/`.db`/`.env`/`.eta` — 18 files between them — while 962
/// unread `.vue` files went unnamed here.
fn adapter_on_ramp_note(ordered: &[&str]) -> String {
    let named: Vec<String> = ordered
        .iter()
        .take(ON_RAMP_EXT_SAMPLE)
        .map(|ext| format!(".{ext}"))
        .collect();
    let more = ordered.len() - named.len();
    let more_note = if more > 0 {
        format!(", +{more} more")
    } else {
        String::new()
    };
    format!(
        "No native parser exists for {} extension(s) in this tree ({}{more_note}) — one entry above per \
         extension with its own count and sample paths. THAT COUNT IS NOT THE TREE'S UNREAD FILETYPES: \
         this channel asks only \"is a parser adapter worth asking for\", so filetypes classed non-source \
         (documents, structured data, configuration, styles, images, media, archives) are EXCLUDED from \
         it however large they are here. A SHAPED analyze reply carries PART of that excluded \
         population in its `coverageGaps` field — the source and structured-data halves, and only above \
         two share floors; filetypes with nothing to project at all (prose, styles, images, media, \
         archives) are named by NEITHER channel, so an empty `coverageGaps` is not a statement about \
         them. The cross-tree join and the raw engine output carry no `coverageGaps` at all. \
         If any of the languages named above matter for the analysis, \
         provide a Mode B adapter overlay via `overlays: [...]` in zzop.config.jsonc (embedders: \
         `adapterOverlays`) — a partial overlay covering just the missing channel/files is enough to \
         start (a tens-of-lines script; see the examples/ adapters in the repo (embedded: `zzop contract \
         adapter-guide` / MCP resource `zzop://contract/adapter-guide`), or `zzop contract \
         example-envelope` / `zzop://contract/example-envelope` for a complete sample). The contract \
         ships inside the binary: `zzop contract envelope-guide` / MCP resource \
         `zzop://contract/envelope-guide` (machine-checkable schema: `zzop contract envelope-schema` \
         / `zzop://contract/envelope-schema`; check your overlay against it with \
         `zzop validate-envelope <file>` / MCP tool \
         `validate_envelope` before wiring it in); repo users, see docs/NORMALIZED_AST.md. (Mode A \
         full-envelope analysis: `zzop analyze-envelope <file>` / MCP tool `analyze_envelope`.)",
        ordered.len(),
        named.join(", ")
    )
}
