// db/multi-write-no-tx on the span-boundary axis (see ./README.md). This rule had never fired anywhere
// in this corpus before the module below, in either direction — the TP control here is its first
// scored detection, which is the other half of why the file exists.
//
// FP PROBE: `enqueueWelcomeEmail` appends one row to an outbox; `markSeatsReconciled` stamps one column
// on a team. They are called from different jobs, minutes apart, and neither depends on the other
// succeeding — there is no unit of work here for a transaction to protect.

type Delegate = {
  create: (args: unknown) => Promise<unknown>;
  update: (args: unknown) => Promise<unknown>;
};
declare const prisma: {
  outboxMessage: Delegate;
  team: Delegate;
  membership: Delegate;
};

export class SubscriptionAdmin {
  enqueueWelcomeEmail = async (userId: string) => {
    await prisma.outboxMessage.create({ data: { userId, kind: 'welcome' } });
  };

  markSeatsReconciled = async (teamId: string) => {
    await prisma.team.update({ where: { id: teamId }, data: { seatsReconciledAt: new Date() } });
  };
}

// TP CONTROL — two writes from different verb families in ONE function, and they really are one unit
// of work: a team that goes active without its owner row is a corrupt state.
export async function activateTeam(teamId: string, userId: string) {
  await prisma.team.update({ where: { id: teamId }, data: { active: true } });
  return prisma.membership.create({ data: { teamId, userId, role: 'owner' } });
}
