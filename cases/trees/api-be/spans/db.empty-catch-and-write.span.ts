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

// ACCESSOR ANCHOR (2026-08-13) — the SETTER half of a same-name get/set pair. Until the
// `(is_static, name)` dedup learned to tell the two apart, only ONE of them projected a leaf, so the
// getter's span made `drop_outer_spans` discard the class-wide span and this setter body sat in NO
// span at all: a genuine swallowed write, invisible to every method-scan rule. The getter below is
// deliberately trivial — it is not the subject, it is what makes the class span droppable.
export class RetentionPolicy {
  private _days = 30;

  get days(): number {
    return this._days;
  }

  set days(next: number) {
    try {
      void auditLog.create({ data: { setting: "retentionDays", next } });
    } catch {}
    this._days = next;
  }
}

// OVERLOAD ANCHOR (2026-08-13) — the IMPLEMENTATION of an overloaded method. Three `foo`-shaped
// members share one dedup key, and until that key preferred the member that HAS a body it kept the
// FIRST — an overload SIGNATURE, which has none. So the leaf shipped with `bodyStart`/`bodyEnd` of
// `None`, the sibling `flush` leaf still made `drop_outer_spans` discard the class-wide span, and the
// implementation body below sat in NO span at all: a genuine swallowed write, invisible to every
// method-scan rule. `flush` is deliberately trivial — it is not the subject, it is what makes the
// class span droppable.
export class ArchiveWriter {
  flush = () => {};

  archive(row: EventRow): void;
  archive(row: EventRow, retries: number): void;
  archive(row: EventRow, retries = 0) {
    try {
      void auditLog.create({ data: { ...row, retries } });
    } catch {}
  }
}
