// security/taint-flow on the span-boundary axis (see ./README.md). Source-and-sink co-occurrence is
// explicitly a coarse v1 approximation of dataflow, and the rule says so — but the unit it approximates
// over is "the same function". A property-only class widens that unit to the whole class, which is a
// different error from the one the message discloses.
//
// FP PROBE: `renderFixedPreview` spawns a fixed binary with an argv array built from two internal
// paths; `requestedWidth` reads a request value and turns it into a number that no member passes to
// anything. There is no path from the request to the process spawn.
//
// Same own-line-comments rule as the sibling html-response module: the `absent` sanitizer veto
// (`escape|sanitize|validate...`) is evaluated over this whole span too.

import { spawn } from 'node:child_process';

interface ThumbnailRequest {
  query: Record<string, string>;
}

export class ThumbnailWorker {
  private readonly binary = '/usr/bin/vipsthumbnail';

  renderFixedPreview = (source: string, dest: string) => {
    spawn(this.binary, ['--size', '320', source, '-o', dest]);
  };

  requestedWidth = (req: ThumbnailRequest) => Number(req.query.width ?? 320);
}

// TP CONTROL — one function, and the request value is handed to the child process as an argument.
export function renderRequestedThumbnail(req: ThumbnailRequest, source: string) {
  spawn('/usr/bin/vipsthumbnail', ['--size', req.query.size, source]);
}
