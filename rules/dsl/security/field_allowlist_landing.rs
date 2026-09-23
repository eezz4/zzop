//! §33/§37 LANDING for `security/mass-assignment`, whose remedy is "whitelist the fields you accept,
//! or use a validated DTO".
//!
//! ONE CARRIER, and that is a judgment rather than an accident. §37 declared a landing for a single
//! rule (`schema/fk-no-index`) and then found a second carrier; here the search ran the other way and
//! found none. The two nearest candidates are `db/unbounded-user-limit` ("clamp it against a maximum")
//! and `security/secret-env-in-fe` ("move its use behind a backend endpoint"), and neither shares this
//! noun: a clamp TRUNCATES a value the caller can see truncated, and moving a secret behind an
//! endpoint changes where code runs. What is unique here is a list of NAMES whose omissions are
//! invisible at the moment the list is written.
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). Swapping
//! `req.body` for an explicit field list is the right edit and its failure mode is the quietest in
//! this pack: an omitted field is not rejected, it is DROPPED. The write still succeeds, the response
//! is still a 200, and the value the caller sent is simply not there afterwards — a partial update
//! that looks saved and is not. A strict DTO is the kinder half of the same remedy precisely because
//! it rejects instead, and saying so is what makes the choice between the two remedies a choice.
//!
//! The completeness question also has a shape worth naming: the list has to cover what the CALLERS
//! send, which is not the same set as the model's fields and not the same set as any one form. One
//! handler serving two forms needs the union; a field a mobile client has been sending for a year is
//! on nobody's current diagram. That is the same "enumerate from traffic, not from memory" move
//! `CORS_ORIGIN_ALLOWLIST_LANDING` makes for origins — related enough to name here so the next author
//! sees the test was applied, and not the same noun: that list gates who may READ a response, this one
//! decides which fields are PERSISTED, and the failures land on opposite sides of the wire.
//!
//! NOT A DISQUALIFIER (§38's two axes). The finding does not become wrong because the list is hard to
//! complete — `req.body` still reaches a write. This rule's axis-A verdict is its own pin
//! (`mass_assignment_measured_counterexample_precedes_the_whitelist_imperative`, a §27 SUMMARY pin
//! against the measured `logger.info({ ...req.body })` counterexample) and is untouched: that repair
//! moved the imperative to the TAIL, so this landing is spliced immediately in front of it and both
//! orders hold at once.
//!
//! POSITION, not presence. The invalidation probe is to move this constant behind the imperative —
//! which, because the imperative is the last sentence, means appending it — where every token is still
//! spelled exactly once and only ORDER has changed.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const FIELD_ALLOWLIST_LANDING: &str = "AN ALLOW-LIST DROPS WHAT YOU FORGOT TO LIST, AND IT DROPS IT QUIETLY: the write still succeeds and still answers 200, the omitted field is simply not persisted, so an edit that reaches this handler looks saved and is not — which is the argument for the validated-DTO half of this remedy over the hand-written list, since a strict schema REJECTS an unlisted field instead of discarding it. Build the list from what this endpoint's callers actually send today rather than from the model: one handler serving two forms needs the union of both, and a field some client has been sending for a year is on nobody's current diagram.";

/// The one carrier. Same delivered fixture the axis-A pin uses, in a `fn` of its own so the two
/// verdicts stay independent — `delivered_pins` reads helper CALLS per block, and pooling the two
/// helpers in one `fn` is how a landing pin came to be counted as a disqualifier verdict before §38.
#[test]
fn mass_assignment_landing_precedes_the_whitelist_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "mass-assignment");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_landing_precedes_imperative(
        "mass-assignment",
        &h[0].message,
        FIELD_ALLOWLIST_LANDING,
        "Whitelist the fields you accept",
    );
}
