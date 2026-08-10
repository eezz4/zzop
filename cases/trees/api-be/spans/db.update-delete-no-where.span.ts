// db/update-delete-no-where on the span-boundary axis, FN DIRECTION (see ./README.md). This is a
// C-veto-only rule: one pattern, so it cannot mis-pair — but its `absent` veto is tested over the same
// oversized span, and a bigger span is more likely to find a suppressor. The rule is `critical`, which
// is what makes this the sharpest FN target in the set.
//
// FN PROBE: `purgeExpiredSessions` deletes the WHOLE sessions table — the defect the rule exists to
// catch, and the same statement the control below fires on. `countStaleDrafts` is an ordinary filtered
// read whose `where:` has nothing to do with it. If the delete goes silent here, a `where:` on an
// unrelated member suppressed a critical finding, and that is a confirmed false negative.

type Delegate = {
  deleteMany: (args?: unknown) => Promise<unknown>;
  count: (args?: unknown) => Promise<number>;
};
declare const prisma: { session: Delegate; draft: Delegate };

const STALE_CUTOFF = new Date('2026-01-01T00:00:00Z');

export class RetentionJob {
  purgeExpiredSessions = () => prisma.session.deleteMany();

  countStaleDrafts = () => prisma.draft.count({ where: { updatedAt: { lt: STALE_CUTOFF } } });
}

// TP CONTROL — the identical unfiltered delete, alone in its own function span. This must fire: it is
// what proves the probe's silence is the span and not the matcher.
export function purgeAllSessions() {
  return prisma.session.deleteMany();
}
