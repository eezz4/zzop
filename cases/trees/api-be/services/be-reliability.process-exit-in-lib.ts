// be-reliability/process-exit-in-lib — bad: process.exit() in a library function (kills the whole server,
// skips cleanup). good: throw and let the caller decide.
export function bad(fatal: boolean) {
  if (fatal) {
    process.exit(1);
  }
}

export function good(fatal: boolean) {
  if (fatal) {
    throw new Error('fatal condition');
  }
}
