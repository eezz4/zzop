// security/mass-assignment on the span-boundary axis (see ./README.md). This rule's message is the
// strongest claim of the set — "`req.body` passed DIRECTLY into a database write" — so a pairing that
// crosses two members is not softened by any co-occurrence disclaimer.
//
// FP PROBE: `recordSubmission` copies the raw payload into an in-memory audit buffer, which is the
// point of an audit buffer and reaches no table; `applyStatus` writes exactly one allow-listed column
// and never touches `req`. There is no field here an attacker can set that the handler did not intend.

type InvoiceDelegate = {
  update: (args: unknown) => Promise<unknown>;
  create: (args: unknown) => Promise<unknown>;
};
declare const prisma: { invoice: InvoiceDelegate };

interface InvoiceRequest {
  body: Record<string, unknown>;
  params: Record<string, string>;
}

export class InvoiceAdminService {
  private readonly pendingAudit: Record<string, unknown>[] = [];

  recordSubmission = (req: InvoiceRequest) => {
    this.pendingAudit.push({ ...req.body });
    return this.pendingAudit.length;
  };

  applyStatus = async (id: string, status: string) => {
    await prisma.invoice.update({ where: { id }, data: { status } });
  };
}

// TP CONTROL — the payload goes straight into the write, in one function.
export function createInvoiceFromRequest(req: InvoiceRequest) {
  return prisma.invoice.create({ data: req.body });
}
