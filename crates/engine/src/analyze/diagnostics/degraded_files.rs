//! The CAUSE axis of the degraded-file census: files this engine read and then could not project
//! structure from, split by the three reasons that can happen — because the three are three different
//! LEVERS, and a reader holding only a count cannot pull any of them.

use crate::analyze::assemble::DegradedFile;
use crate::pipeline::DegradeCause;
use crate::EngineConfig;

/// Capability self-report: files this run produced no structural projection for, counted per CAUSE,
/// with the lever each cause hands the caller.
///
/// The summary sentence deliberately does NOT say "fell back to a lexical projection", true as that is
/// for two of the three arms: an unreadable file got no projection of any kind, not a lesser one, and a
/// lead sentence that generalizes the two quiet causes over the loud one is the same overclaim the
/// per-cause paragraph below exists to prevent.
///
/// ## The gap, and why no existing report held it
/// A degraded file is the most surprising way to fall out of the flow — the code is there, zzop read it,
/// and it produced little or nothing. The fact was already published twice, and neither channel could
/// say why: `AnalyzeOutput::degraded` is a sorted PATH LIST (capped at 50 with a `degradedTruncated`
/// disclosure by the summary layer) and `coverage.degraded` is its uncapped COUNT. Both answer WHICH and
/// HOW MANY; the three causes were collapsed into one boolean before either could see them, so an
/// oversized file (a `sizeCap` decision the caller can change), an unreadable one (an environment fault)
/// and a parse failure (a bug report, or a syntax level this frontend does not accept) arrived
/// indistinguishable. `unparsed_extension_warning` owns the ADJACENT and orthogonal fact — a
/// normal-sized file whose extension has no native parser at all — and the `language-unparsed`
/// blindness class already states that an oversized file of such an extension lands in BOTH. Nothing
/// else came close: `minified_files_warning` owns a line-shape classification that skips DSL packs while
/// structural extraction still runs (the mirror image of this), `uncovered_extension_warning` and
/// `rule_vetoed_files_warning` own PATH-admission facts about the rule set, and `scoring_scope_warning`
/// owns files the caller excluded on purpose.
///
/// ## What it may claim, and what it must not
/// "Skipped" would be a lie for two of the three causes. An oversized or parse-failed file still runs
/// every `line-scan` DSL rule against its raw text and still contributes a lexically counted `loc`; what
/// it loses is the STRUCTURAL projection, and with it the matchers that read one (`symbol-scan`,
/// `method-scan`, `call-scan`, `literal-scan`, `io-scan`) plus its outgoing dep-graph edges. Only
/// `Unreadable` lost everything, and it is the one arm allowed to say so. The message states the split
/// out loud rather than letting a reader generalize the loud case over the quiet ones.
///
/// ## Only files a frontend was going to parse
/// A file `crate::dispatch::dispatch` never claimed has no structural projection to lose — an oversized
/// `.png` or a 4MB `.json` dump degrades, and nothing about the run got worse for it: no parser was ever
/// going to run, `loc` is still counted, line-scan rules still ran. Counting those would inflate the
/// number with files that lost nothing, which is the same overclaim `minified_files_warning` documents
/// filtering out for its own subject (a PNG is not minified source). `Unreadable` is the deliberate
/// exception and is reported at ANY extension: that path lost the raw text itself, so its line-scan
/// findings and its `loc` are gone too, which is a real loss for a file of any kind, and it points at
/// the machine rather than at the code.
///
/// ## Silences
/// * no degraded file survived the filter above — including a run whose only degrades were undispatched
///   oversized data files, which is silence on purpose, not a missed report.
/// * envelope mode (`analyze_envelope`). Its `degraded` list is copied from what an external producer
///   DECLARED about files this engine never read, so none of the three causes here is knowable there and
///   none of the three levers would be actionable. That lane does not run `assemble` at all, so the
///   exclusion is structural rather than a filter anyone has to maintain.
///
/// ONE aggregate entry with a per-cause example list, never one line per file
/// (`pack_scope::scope_warnings`'s `zero_scope_packs_warning` doc owns that rule). The cap is
/// [`SAMPLE`](super::SAMPLE) — bounded, not re-declared, so no second censused policy name exists for
/// the one cap that constant already owns. It is applied PER CAUSE rather than once over the merged
/// list, and that is the deliberate deviation: the sample exists to be acted on, each cause is a
/// different action, and a run with 200 oversized files and 1 parse failure would otherwise spend all
/// three slots on the lever the reader already understands and hide the one they do not.
///
/// Deterministic: the input arrives in `rel` order (`assemble` sorts it before the sweep) and this
/// function only partitions and truncates it.
pub(in crate::analyze) fn degraded_files_warning(
    degraded: &[DegradedFile],
    config: &EngineConfig,
) -> Option<String> {
    let subjects: Vec<&DegradedFile> = degraded
        .iter()
        .filter(|d| d.dispatched || d.cause == DegradeCause::Unreadable)
        .collect();
    if subjects.is_empty() {
        return None;
    }
    let cap = super::SAMPLE;
    // Cause order is the message's reading order, and it is fixed rather than count-sorted: a self-report
    // whose sentence order moves with the data is one a reader cannot diff between two runs.
    let mut clauses: Vec<String> = Vec::new();
    for (cause, phrase) in [
        (
            DegradeCause::Oversized,
            "over the size cap, so no parser was invoked",
        ),
        (
            DegradeCause::ParseFailure,
            "dispatched to a native parser that failed to parse them",
        ),
        (
            DegradeCause::Unreadable,
            "unreadable (permission error, or deleted/replaced during the run)",
        ),
    ] {
        let hits: Vec<&str> = subjects
            .iter()
            .filter(|d| d.cause == cause)
            .map(|d| d.rel.as_str())
            .collect();
        if hits.is_empty() {
            continue;
        }
        let mut sample = hits
            .iter()
            .take(cap)
            .copied()
            .collect::<Vec<&str>>()
            .join(", ");
        if hits.len() > cap {
            sample.push_str(&format!(", +{} more", hits.len() - cap));
        }
        clauses.push(format!("{} {phrase}: {sample}", hits.len()));
    }
    Some(format!(
        "{total} file(s) this run walked and got NO STRUCTURAL PROJECTION from -- the `degraded` list, \
         which until now carried the count and the paths but never the reason. By cause, first {cap} by \
         path each -- {breakdown}. What that costs is NOT the same \
         for each cause. An oversized or unparseable file KEEPS its lexically counted line total and still \
         has every `line-scan` DSL rule run against its raw text, so a line-shaped finding on one of \
         them is real; what it loses is the structural projection -- symbols, imports, IO facts, call \
         sites, string literals, loop/function/test spans -- and with it the `symbol-scan` / \
         `method-scan` / `call-scan` / `literal-scan` / `io-scan` matchers, which go SILENT rather than \
         clean, plus its outgoing dep-graph edges. An UNREADABLE file lost all of that AND the raw text: \
         no rule of any kind ran on it and its line total is 0. The lever differs too. Oversized is your \
         decision and you can change it: `sizeCap` (zzop.config.jsonc; embedders: the facade request's \
         `sizeCap`), currently {size_cap} bytes. Unreadable is the environment, not the tree -- check \
         permissions, or a file the walk listed and something removed mid-run. A parse failure is \
         either a syntax level this frontend does not accept or a bug worth reporting with the path; \
         re-running will not change it. Files with no native parser for their extension are a DIFFERENT \
         fact with its own self-report, and a normal-sized one of those is not counted here; an \
         oversized one is counted by both, because the two are orthogonal. Data and asset files no \
         frontend claimed are not counted here either -- nothing structural was ever going to run on \
         them, so their size cost this run nothing.",
        total = subjects.len(),
        breakdown = clauses.join("; "),
        size_cap = config.size_cap,
    ))
}

#[cfg(test)]
mod tests;
