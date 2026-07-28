// be-reliability/console-in-be — bad: console.* on a backend path (services/). good: a structured logger.
declare const logger: { info(message: string): void };

export function bad() {
  console.log('order processed');
}

export function good() {
  logger.info('order processed');
}
