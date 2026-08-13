// DECOY for code-hygiene/console-in-be. In scope, provably: the rule's file_pattern gates on a
// `(api|server|backend|be|routes|controllers|services)/` path segment, and this file sits under `api/`.
// Its line_pattern lists exactly `console.(log|error|warn|info)`; `debug`/`trace` are outside it, a
// structured logger is the intended replacement, and a commented-out call is skipped by
// `skip_comment_lines`. All three are deliberate probes of the rule's stated boundary.
export declare const logger: { info(m: string): void; error(m: string, e?: unknown): void };
export declare const console: { debug(m: string): void; trace(m: string): void };

export function reportStartup(port: number): void {
  logger.info(`listening on ${port}`);
  console.debug('startup diagnostics enabled');
  console.trace('startup trace');
  // console.log('left behind on purpose — a commented-out call must not be reported')
}
