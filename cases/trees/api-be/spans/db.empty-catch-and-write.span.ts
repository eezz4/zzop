// db/empty-catch-and-write on the span-boundary axis (see ./README.md). Also previously unscored in
// this corpus: the TP control below is its first labeled detection.
//
// FP PROBE: `recordEvent` writes a telemetry row and lets its own failures propagate to the caller;
// `closeSocket` swallows a close error on purpose, because a socket that is already gone is exactly
// the case a teardown path must not turn into an exception. The discarded error belongs to the socket,
// not to the write.

type EventRow = { name: string; at: string };
declare const prisma: { telemetryEvent: { create: (args: unknown) => Promise<unknown> } };
declare const auditLog: { create: (args: unknown) => Promise<unknown> };

export class TelemetrySink {
  private socket: { close(): void } | null = null;

  recordEvent = async (row: EventRow) => {
    await prisma.telemetryEvent.create({ data: row });
  };

  closeSocket = () => {
    try {
      this.socket?.close();
    } catch {}
    this.socket = null;
  };
}

// TP CONTROL — the empty catch swallows THIS write's own failure, in one function: the row is silently
// lost and the caller is told nothing.
export async function persistAuditRow(row: EventRow) {
  try {
    await auditLog.create({ data: row });
  } catch {}
}
