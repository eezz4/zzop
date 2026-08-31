// be-redis/flushall-in-code — bad: FLUSHALL erases every key in the database, including keys this
// service does not own. good: an explicit, scoped unlink of the one key being invalidated.
//
// GATE: the rule carries `require_file: (?i)flush(all|db)`, satisfied by the call on the bad line.
//
// The client is a bare `declare` with no `createClient(`/`new Redis(` call and no `'ioredis'` import
// specifier, on purpose: either would make this module a `redis/client-no-error-listener` as well (that
// rule's `require_file` is the import specifier and its `line_pattern` is the constructor), and this is
// the single-rule fixture for the flush shape.
declare const client: any;

export async function bad(): Promise<void> {
  await client.flushAll();
}

export async function good(id: string): Promise<void> {
  await client.unlink('session:' + id);
}
