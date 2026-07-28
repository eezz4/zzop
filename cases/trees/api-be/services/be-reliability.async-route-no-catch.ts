// be-reliability/async-route-no-catch — bad: an async route handler with no try/catch/next(err)/.catch().
// good: the same handler wrapped in try/catch that forwards to next.
declare const router: { get(path: string, h: (req: unknown, res: { send(x: unknown): void }, next: (e?: unknown) => void) => unknown): void };
declare function load(): Promise<string>;

export function bad() {
  router.get('/items', async (_req, res) => {
    const data = await load();
    res.send(data);
  });
}

export function good() {
  router.get('/items', async (_req, res, next) => {
    try {
      const data = await load();
      res.send(data);
    } catch (e) {
      next(e);
    }
  });
}
