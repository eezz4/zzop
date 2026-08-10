// security/ssrf-user-url on the span-boundary axis (see ./README.md). The rule's message claims the
// egress call and the request read are "in a function" together; a property-only class hands it a span
// that is a whole CLASS, so two unrelated members pair.
//
// FP PROBE: `refreshUpstreamSnapshot` fetches a compile-time constant host and never sees a request;
// `describeIncident` reads the request and never leaves the process. Neither is an SSRF, and no
// attacker-chosen value can reach any URL in this class.

interface StatusRequest {
  query: Record<string, string>;
  params: Record<string, string>;
}

export class StatusPageService {
  private readonly upstream = 'https://status.internal.example.com';

  refreshUpstreamSnapshot = async () => {
    const res = await fetch(`${this.upstream}/snapshot.json`);
    return res.json();
  };

  describeIncident = (req: StatusRequest) => ({
    id: req.params.id,
    severity: req.query.severity ?? 'unknown',
  });
}

// TP CONTROL — a standalone function gets its own leaf span, and here the request value really does
// choose the host. This must fire; it is what makes the probe's verdict readable either way.
export async function fetchIncidentPreview(req: StatusRequest) {
  const res = await fetch(req.query.target);
  return res.text();
}
