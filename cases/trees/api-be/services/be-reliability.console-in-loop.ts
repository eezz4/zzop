// be-reliability/console-in-loop — bad: a console write the parser places INSIDE the loop body, so it
// runs once per iteration. good: the same per-item detail through a structured logger, which is not a
// console write at all and is therefore invisible to this rule by construction, not by exclusion.
//
// `bad` co-fires code-hygiene/console-in-be as well, and that is correct rather than noise: the file sits
// under services/ and the call is a console.log, so both rules' premises hold at once. They are different
// claims — one about WHERE the write is, one about how many times it runs.
declare const logger: { info(message: string, fields?: Record<string, unknown>): void };

export function bad(orderIds: string[]) {
  for (const id of orderIds) {
    console.log('processing order ' + id);
  }
}

export function good(orderIds: string[]) {
  for (const id of orderIds) {
    logger.info('processing order', { id });
  }
}
