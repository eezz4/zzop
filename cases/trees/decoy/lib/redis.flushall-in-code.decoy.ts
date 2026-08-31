// DECOY for redis/flushall-in-code. In scope, provably: the rule's require_file (`(?i)flush(all|db)`) is
// satisfied by the command names below, so the file was scanned.
//
// This is a VETO-ARM decoy, the strongest shape this tree has (see ../README.md): the denylist line
// MATCHES the rule's line_pattern — `[(\[]\s*['"`]flush(all|db)['"`]` sees `['flushall'` — and stays
// silent only because the rule's exclude_pattern recognizes three consecutive quoted words as a string
// list rather than a call. Delete that exclude and this file reports. The `unlink` call is the ordinary
// half: a scoped delete is what the rule is telling people to write instead, so it must never fire.
declare const client: any;

// A guard's own vocabulary. Naming a destructive command is not issuing one.
export const FORBIDDEN_COMMANDS = ['flushall', 'flushdb', 'shutdown'];

export function isForbidden(command: string): boolean {
  return FORBIDDEN_COMMANDS.includes(command.toLowerCase());
}

export async function invalidate(id: string): Promise<void> {
  await client.unlink('session:' + id);
}
