// The SERVER half of the same front end, exactly as trees/fe-axios/server/loader.ts is for its pair:
// plain `fetch` with full literal paths, because the generated client is a browser-side artifact.
//
// Same two jobs. (1) CONTRAST — these four calls reach the same four be-invoices routes the generated
// client reaches, and every one of them JOINS, so the boundary is visible inside a single tree. (2) The
// product's since-deleted distinct-bucket-key cap: `crates/summary/src/output/bucket_keys.rs` used to
// cap a bucket's distinct-key list at 20 and snapshot.mjs aborted on truncation, with this corpus at 19
// distinct `unconsumedProvides` keys. Keeping these routes consumed is true to the fixture and stays
// worth keeping on that merit. See trees/fe-axios/server/loader.ts for the measured detail.
declare function fetch(u: string, init?: unknown): Promise<unknown>;

export async function loadInvoicesPage() {
  await fetch('/api/invoices');
  await fetch('/api/shipments');
  await fetch('/api/carriers');
  await fetch('/api/invoices/exports');
}
