// security/html-response-from-request on the span-boundary axis (see ./README.md). Three patterns
// again — an HTML-shaped marker, request input, and a `res.send`-family write — plus an `absent`
// sanitizer veto that is ALSO evaluated over the whole span, so this module probes both directions of
// the axis at once: the pairing can reach too far, and so can the veto that would clear it.
//
// FP PROBE: `renderShell` sends a constant string and never reads the request; `currentRegion` reads
// the request and returns a value to other server code. No request-derived value is spliced into any
// markup in this class.
//
// NOTE for whoever edits this file: keep prose about escaping/sanitizing on OWN-LINE comments only.
// `skip_comment_lines` drops a full comment line before the `absent` regex sees it, but a TRAILING
// comment shares its line with code and would satisfy the sanitizer veto, silencing the probe for a
// reason that has nothing to do with the span.

interface StatusRequest {
  query: Record<string, string>;
}

interface StatusResponse {
  send(body: string): void;
  setHeader(name: string, value: string): void;
}

export class StatusPageRenderer {
  private readonly shell = '<div class="status-shell" data-hydrate="status"></div>';

  renderShell = (res: StatusResponse) => {
    res.setHeader('Content-Type', 'text/html; charset=utf-8');
    res.send(this.shell);
  };

  currentRegion = (req: StatusRequest) => req.query.region ?? 'eu-west-1';
}

// TP CONTROL — one function, and the request value is interpolated straight into the markup.
export function renderGreeting(req: StatusRequest, res: StatusResponse) {
  res.send(`<div class="greeting">Hello, ${req.query.name}</div>`);
}
