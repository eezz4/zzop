// be-db/manual-tx-no-rollback — bad: a hand-driven transaction that can only commit. If the work between
// BEGIN and COMMIT throws, the connection is returned to the pool still inside an open transaction.
// good: the same shape with the failure path spelled.
//
// The rule is a method-scan whose `absent` is the rollback vocabulary, so the two halves have to be
// separate functions — a ROLLBACK anywhere in `bad`'s body would silence it.
declare const client: { query(sql: string): Promise<unknown> };
declare function work(): Promise<void>;

export async function bad(): Promise<void> {
  await client.query('BEGIN');
  await work();
  await client.query('COMMIT');
}

export async function good(): Promise<void> {
  await client.query('BEGIN');
  try {
    await work();
    await client.query('COMMIT');
  } catch (e) {
    await client.query('ROLLBACK');
    throw e;
  }
}
