//! S1: controller-decorator silence tripwire (provide side).

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// DELIBERATELY independent of the parsers' own controller/decorator vocabularies (a pinned
/// policy-value divergence, not an oversight): this tripwire exists to catch EXTRACTOR blindness, so
/// sharing the extractor's vocabulary would blind it to exactly the gaps it guards — an idiom the
/// extractor's vocab misses must still look "controller-shaped" to this independent regex for the
/// self-report to fire. Do not unify with `controller_decorators`/`provides` vocab.
fn controller_decorator_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"@\w*(?:Controller|Mapping|Get|Post|Put|Delete|Patch)\b").unwrap()
    })
}

/// A decorator counts as coverage evidence only when it LEADS its line (first non-whitespace token) —
/// where a real framework registration conventionally puts it (`@GetMapping("/x")`, `  @Controller('/a')`,
/// a Java `@RequestMapping` above a class). (A modifier-preceded annotation on the same line, `public
/// @GetMapping …`, is the rare exception this misses — an accepted FN, since the tripwire needs ≥3 files
/// and such annotations conventionally sit on their own line.) This single lexical anchor discounts the
/// two shapes that are mentions rather than routes, without any parser vocabulary: a doc/line comment
/// (`/// … @GetMapping`,
/// ` * @Controller`, `// see @Post`) trims to `/`/`*`, and a decorator embedded in a STRING LITERAL
/// (`dir.write("a.ts", "@FastController('/a')")` — a SAST tool's own test fixtures) trims to the
/// surrounding code, so neither begins with the match. It matters acutely when the scanned tree is
/// itself a tool that documents/fixtures these decorators (dogfooding zzop on its own source went 57 →
/// ~a handful of files this way). A real decorator on a real code line — including the accepted
/// MapStruct-`@Mapping` FP surface — still leads its line and still counts.
fn decorator_leads_line(line: &str, re: &Regex) -> bool {
    re.find(line.trim_start()).is_some_and(|m| m.start() == 0)
}

/// Minimum distinct decorator-matching files before the S1 self-report fires — enough evidence mass
/// that extractor blindness is plausible rather than one stray annotation.
///
/// Do not unify with `FANOUT_MIN_FILES` (rules-cross-layer) or `SEAMS_MIN_FILES` (zzop-metrics)
/// (policy inventory T3 — coincidental equality): all three are 3 today, but each gates a different
/// signal on a different tuning axis. This one gates a coverage SELF-REPORT tuned toward sensitivity
/// (silence is fatal, over-disclosure is safe); the others gate an Info finding (precision-tuned) and
/// a metric eligibility floor. Each is free to move without re-justifying the others.
const MIN_FILES: usize = 3;
const MAX_SAMPLES: usize = 3;

/// Near-zero (not exact-zero) floor shared by S1's `http_provides_count` gate, S2's `http_provides_count`
/// gate, S3's `io_provides`/`io_consumes_keyed` gates, and S4's `http_consumes_count` gate. Round 9's blind
/// be-express tree still had 1 extracted `http` provide — an exact-zero gate misses it entirely — and
/// round 14's blind Spring-BE tree (17/19 routes lost to a parser limit) still had 2 lexically-extracted
/// provides tree-wide, silencing S1's own original exact-zero gate the identical way. A near-zero floor
/// still fires on both while a real micro-BE with 1-2 genuinely-extracted routes (or a real micro-FE with
/// 1-2 genuinely-extracted consumes) gets a gracefully-worded disclosure it can read and ignore, rather
/// than silence. `pub(crate)` (not just `pub(super)`) so `analyze::assemble` can precheck S5's keyed
/// consume gate before building the sorted walked-rel list its census needs — the same precheck
/// convention `IO_NEAR_ZERO_FLOOR` documents for S3.
pub(crate) const MIN_PROVIDES_FLOOR: usize = 3;

/// Returns a ready-to-push `warnings` entry if `candidate_rels` show a controller-decorator-looking line
/// in at least `MIN_FILES` distinct files while `http_provides_count` sits below `MIN_PROVIDES_FLOOR` —
/// NEAR-zero, not exact-zero. Round 14's Angular-FE x Spring-BE pair lost 17/19 routes to a parser limit
/// but still had 2 lexically-extracted provides tree-wide; an exact-zero gate silently passes right over
/// that tree — the identical failure shape S2's `MIN_PROVIDES_FLOOR` near-zero floor was built to catch,
/// so S1 now shares it rather than gating on exactly zero. Cheap on the success path: skips the disk
/// re-read entirely once `http_provides_count >= MIN_PROVIDES_FLOOR`.
///
/// Only line-LEADING decorators count ([`decorator_leads_line`]): a match inside a comment or a string
/// literal is discounted, so a tree that merely DOCUMENTS or fixtures these decorators (a SAST tool's
/// own source is the sharp case) no longer inflates the file count into a false silence report.
///
/// Mild FP surface (accepted): `MIN_FILES`+ files with a line-leading decorator while provides stay
/// below the floor can over-fire on, e.g., a MapStruct `@Mapping` mapper cluster (the regex's `Mapping`
/// alternative matches `@Mapping` too) alongside a genuinely tiny (1-2 route) controller. Accepted under
/// the coverage self-report's governing principle: over-disclosure is safe, silence is fatal.
///
/// Determinism: relies on `candidate_rels` already being sorted/deduped by the caller
/// (`analyze::assemble`) — this function performs no re-sort, so an unsorted input would yield a
/// non-deterministic sample.
pub fn controller_silence_warning(
    root: &Path,
    candidate_rels: &[String],
    http_provides_count: usize,
) -> Option<String> {
    if http_provides_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let re = controller_decorator_re();
    let mut matched: Vec<&str> = Vec::new();
    for rel in candidate_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if text.lines().any(|line| decorator_leads_line(line, re)) {
            matched.push(rel.as_str());
        }
    }
    if matched.len() < MIN_FILES {
        return None;
    }
    let sample: Vec<&str> = matched.iter().take(MAX_SAMPLES).copied().collect();
    let mut sample_str = sample.join(", ");
    if matched.len() > MAX_SAMPLES {
        sample_str.push_str(&format!(", +{} more", matched.len() - MAX_SAMPLES));
    }
    Some(format!(
        "{} file(s) carry controller-style route decorators/annotations but only {http_provides_count} \
http route(s) were extracted tree-wide — the framework's registration idiom may be unsupported; \
cross-layer joins will be silent for this tree (e.g. {sample_str}) — project this tree's routes with a \
Mode B overlay adapter (see the adapter examples) to restore cross-layer visibility: a partial envelope \
covering just the provide channel is enough; contract: `zzop contract envelope-guide` on MCP hosts, \
docs/NORMALIZED_AST.md in the repo.",
        matched.len()
    ))
}
