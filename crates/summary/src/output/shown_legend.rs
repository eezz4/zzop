//! `shownMeaning` — the legend for `findings.shown` / `crossLayerFindings.shown`, written to the same
//! contract as `byRuleMeaning`: a shaped field ships beside a sentence saying WHAT IT IS, because the
//! field alone is read as being something else.
//!
//! # The misread this exists to stop
//! `shown` is an ORDER. Readers take it for a RANKING — that row 1 is the finding most likely to be
//! real and most worth acting on, and that a window of forty is "the forty worst". Neither is true and
//! neither was ever claimed on the wire, which is the defect: the claim was absent, not false, and an
//! absent claim gets supplied by the reader.
//!
//! Measured 2026-09-05, on three real projects, by opening all 120 first-screen rows against their
//! source: 86 of the 120 produced no edit, and the evaluator's own summary of the window was *"the
//! first screen does not lead with its best material"*. That sentence is about a ranking — of a list
//! that is not one.
//!
//! # Why the repair is a sentence and not a sixth key
//! The obvious repair — order by how likely a rule is to be right — is refused twice over, and both
//! refusals are older than this legend.
//!
//! * A per-finding likelihood is a scalar a caller thresholds on, and `output-philosophy` §12 makes
//!   its absence permanent: the number would promise a calibration nothing in this engine backs.
//! * A per-RULE quality order would have to be fitted to the trees it was measured on, which makes the
//!   shipped ordering a function of one corpus. The rules that yielded nothing in that measurement are
//!   not a property of zzop; they are a property of those three trees plus today's rule set.
//!
//! What is left is the honest half: say what the order IS, and say — in the same breath, since this is
//! the half a reader supplies for themselves — what it does not claim. The lever that genuinely moves a
//! rule up or down is its declared severity, which is a per-rule catalog value and a configuration
//! override, not an ordering key invented here.

/// The full text. Interpolates nothing: every key it names is spelled the same on every reply, and the
/// numbers it cites are about a past measurement rather than about this run — which is exactly what
/// makes it foldable (see [`super::legends`]).
pub(crate) const SHOWN_MEANING: &str =
    "`shown` is an ORDER, and the thing it is NOT is a ranking. Five keys decide the sequence, and \
     every one of them is shaping only: this ORDER drops nothing, no count moves, and the `total` / \
     `bySeverity` / `byRule` beside it are over the FULL set rather than over this window. What CAN \
     narrow the window is not the order, and each of the two ways announces itself in its own key: \
     the list cap writes `truncated`, and a `severity`/`rule` filter you passed writes `filtered` \
     with what it removed. Neither is present when it did not act, so their absence is the statement \
     that this window is the whole matching set.\n\n\
     THE KEYS, outermost first. (1) DEPLOYMENT ROLE, descending — code that ships, then test paths, \
     then build and release surface. This key demotes, so it announces itself: `testPaths` and \
     `buildPaths` ride the same reply with their own counts and sentences whenever they have \
     something to say, because a demotion nobody is told about is a silent filter. (2) SEVERITY, \
     descending, inside a role. (3) SITE — how many findings, of ANY rule, already pointed at this \
     exact file and line; every distinct site takes a slot before any site takes a second. (4) \
     RULE-IN-FILE — the same question one step coarser: every rule-and-file pair before any pair \
     takes a second. (5) RULE — every rule's Nth finding before any rule's N+1th. Ties fall back on \
     the engine's own order, which is itself severity, then file, then line, then rule id, so two \
     runs over one tree produce the same bytes.\n\n\
     WHY THREE OF THE FIVE ARE DIVERSITY RATHER THAN RANK. Keys 3 to 5 demote nothing and rank \
     nothing — they interleave. They exist because a forty-row window was opened row by row against \
     the source on real projects and found to hold fewer than forty pieces of information: one \
     configuration file's dictionary took three rows, one lock file's package name took two, one \
     deliberately mirrored route took three. Interleaving is what makes the fortieth row worth \
     reading. It is not a claim that the first row matters more than the fortieth.\n\n\
     WHAT THIS ORDER DOES NOT CLAIM, stated here rather than left to be worked out. It does not say \
     the first row is likelier to be a real defect than the last, or worth more of your time. Nothing \
     in this engine scores a finding by how likely it is to be true — a per-finding confidence number \
     is a deliberate and permanent absence, because a number a caller can threshold on promises a \
     calibration nothing here backs. Inside one severity band the sequence is a spread across \
     distinct places and rules, and after that it is the letters in a file path. A rule that is right \
     about your tree every time and a rule that is wrong every time reach this window on the same \
     terms. The one lever that does move a rule up or down is its SEVERITY, which the rule catalog \
     declares and your configuration can override; the ordering holds no second opinion about it.\n\n\
     So read `shown` as: a deterministic spread across this band's places and rules. Never as: the N \
     worst things in this tree.";
