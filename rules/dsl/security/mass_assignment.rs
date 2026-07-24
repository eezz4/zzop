use crate::{hits, scan, TempDir};

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
        "declare const prisma: any;\nexport async function updateUser(req: any) {\n  // mass-assignment-ok: internal admin-only migration endpoint, body pre-validated upstream\n  return prisma.user.update({ data: req.body });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mass-assignment").is_empty(),
        "{:?}",
        out.findings
    );
}
