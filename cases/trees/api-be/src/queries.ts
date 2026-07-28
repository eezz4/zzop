// Prisma query call sites scanned by the schema x usage JOIN rules (`scan_query_call_sites` walks
// `<treeRoot>/src` for `getPrisma().<model>.<method>(...)`). Each call targets a distinct model so the
// three join findings don't pile onto one line.
declare function getPrisma(): {
  ticket: { findMany(a?: unknown): Promise<unknown> };
  job: { findMany(a?: unknown): Promise<unknown> };
  order: { findMany(a?: unknown): Promise<unknown> };
};

export async function liveTickets() {
  // soft-delete-bypass: Ticket has `deletedAt`, this findMany never mentions it → tombstoned rows leak in.
  return getPrisma().ticket.findMany({ where: { open: true } });
}

export async function jobsByState() {
  // enum-string-drift: `state` resolves to enum TicketState {OPEN, CLOSED}; 'PENDING' is not a member.
  return getPrisma().job.findMany({ where: { state: 'PENDING' } });
}

export async function orderedOrders() {
  // orderby-unindexed: Order.price has no @id/@unique/@@index coverage — an unindexed sort.
  return getPrisma().order.findMany({ orderBy: { price: 'asc' } });
}
