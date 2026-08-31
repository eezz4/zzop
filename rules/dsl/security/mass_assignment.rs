use crate::{assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir};

// --- mass-assignment ---

#[test]
fn req_body_passed_as_data_into_update_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "mass-assignment");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

/// §27 pin (2026-08-29). The disqualifier carries a MEASURED counterexample -- `logger.info({
/// ...req.body })` followed by a `create` whose `data` is all constants, with the finding anchoring on
/// the LOGGER line -- and that counterexample is what lifts it out of §27's excluded "bare hedge"
/// category ("this is co-occurrence, not dataflow" on its own does not qualify) and makes it a judgment
/// about a named construct. It sat behind "Whitelist the fields you accept, or use a validated DTO.",
/// so the reader staring at that logger line was told to whitelist fields on a write that had none.
///
/// WHAT MOVED: the imperative, not the clause. Two sentences downstream of the clause take their
/// antecedents from it -- "That is why it is `warning` and not `critical`" and "The reverse also holds"
/// -- so lifting the clause over the remedy would have stranded both, while sending the remedy to the
/// tail strands nothing. 1180 characters before and after, character multiset identical.
///
/// INVALIDATION PROBE: put the imperative back after the opening sentence. Every token stays present and
/// spelled exactly once, a `contains` pin stays green, and this assertion alone goes red.
#[test]
fn mass_assignment_measured_counterexample_precedes_the_whitelist_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "mass-assignment");
    // Sentence order only -- the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_disqualifier_summary_precedes_imperative(
        "mass-assignment",
        &h[0].message,
        "an unrelated write on another line produce exactly this finding",
        "Whitelist the fields you accept",
        "anchors on the LOGGER line",
    );
}

#[test]
fn req_body_spread_into_updatemany_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function patchUsers(req: any) {\n  return prisma.user.updateMany({ ...req.body });\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "mass-assignment").len(), 1, "{:?}", out.findings);
}

#[test]
fn whitelisted_field_passed_into_create_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/items.ts",
        "declare const prisma: any;\nexport async function createItem(req: any) {\n  return prisma.item.create({ data: { name: req.body.name } });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mass_assignment_ok_marker_above_the_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/users.ts",
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  // zzop-mass-assignment-ok: internal admin-only migration endpoint, body pre-validated upstream\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}
