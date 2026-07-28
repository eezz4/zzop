// be-security/error-leak-to-client — bad: the raw error object sent to the client. good: a generic
// message to the client, the real error to the server log.
declare const logger: { error(e: unknown): void };
interface Res { status(code: number): Res; json(body: unknown): void }

export function bad(res: Res, err: Error) {
  res.status(500).json(err);
}

export function good(res: Res, err: Error) {
  logger.error(err);
  res.status(500).json({ error: 'internal error' });
}
