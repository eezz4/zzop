// sql/raw-sql-check-then-write — bad: a raw SELECT that decides whether to INSERT, with nothing holding
// the gap. Two requests both read "absent" and both insert. good: one statement that lets the database
// decide, via the unique constraint.
//
// The rule is a method-scan whose `absent` list is the guard vocabulary (`FOR UPDATE`, `serializable`,
// `ON CONFLICT`, `ON DUPLICATE KEY`, `INSERT OR IGNORE/REPLACE`), so the halves have to be separate
// functions — the good half's `ON CONFLICT` would otherwise silence the bad one.
//
// The table name is tree-unique (`sqlraw_members`) for the reason trees/rust-svc/src/queries.rs
// measured: a plain name here becomes a `db-table` consume and can make a cross-tree rule fire in
// ANOTHER tree, so a fixture added for one rule silently moves a different tree's expected set.
declare const db: { query(sql: string, params: readonly unknown[]): Promise<{ rowCount: number }> };

export async function bad(email: string): Promise<void> {
  const found = await db.query('SELECT id FROM sqlraw_members WHERE email = $1', [email]);
  if (found.rowCount === 0) {
    await db.query('INSERT INTO sqlraw_members (email) VALUES ($1)', [email]);
  }
}

// The good half KEEPS the read (review ledger V335). Without it this function was silent for two
// reasons at once — no `SELECT` to satisfy `after`, and an `ON CONFLICT` in the `absent` list — so it
// could not tell which of the two a widening had broken. Now the read-then-write shape is present and
// the ONLY thing holding the rule back is the conflict clause, which is what the header claims.
export async function good(email: string): Promise<void> {
  const found = await db.query('SELECT id FROM sqlraw_members WHERE email = $1', [email]);
  if (found.rowCount === 0) {
    await db.query('INSERT INTO sqlraw_members (email) VALUES ($1) ON CONFLICT (email) DO NOTHING', [
      email,
    ]);
  }
}
