// security/path-traversal on the span-boundary axis (see ./README.md). This rule needs THREE patterns
// (fs call, request input, `path.join`), which is the interesting case: the more patterns a rule
// requires, the more it looks like real evidence, and the more members an oversized span can harvest
// them from.
//
// FP PROBE: the join happens once, at construction, over two constants; the read is of that constant
// path; the request is only ever echoed back as a slug. Nothing a caller sends reaches the filesystem.
//
// SECOND RULE, FOR FREE: `reliability/sync-fs-in-handler` is A-exposed on the same axis and pairs a
// synchronous fs call with "request-handler evidence". It reads this module the same way the rule
// above does — `readFileSync` from one member, `req.params` from another — so the probe and the
// control below score BOTH rules, and its FP is anchored on the same line. That was not designed in;
// it is what an oversized span does when two rules happen to want tokens the same class contains.

import * as fs from 'node:fs';
import * as path from 'node:path';

interface ReportRequest {
  params: Record<string, string>;
}

const TEMPLATE_ROOT = '/srv/app/templates';

export class ReportBundleService {
  private readonly manifestPath = path.join(TEMPLATE_ROOT, 'manifest.json');

  loadManifest = () => JSON.parse(fs.readFileSync(this.manifestPath, 'utf8'));

  describeRequestedReport = (req: ReportRequest) => ({
    slug: req.params.slug,
    generatedAt: new Date().toISOString(),
  });
}

// TP CONTROL — all three in one function, and the request value really does select the file.
export function readRequestedTemplate(req: ReportRequest) {
  return fs.readFileSync(path.join(TEMPLATE_ROOT, req.params.slug), 'utf8');
}
