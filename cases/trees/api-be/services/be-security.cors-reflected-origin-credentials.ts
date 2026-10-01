// be-security/cors-reflected-origin-credentials — bad: the request's own origin echoed back WITH
// credentials allowed, which is every origin with cookies attached — the wildcard the browser refuses,
// spelled in a way it accepts. good: an explicit origin, credentials still allowed.
//
// The origin is `true` (the middleware's "reflect whatever asked") rather than `'*'` on purpose: a
// literal star would make this a `be-security/cors-credentials-wildcard`, which has its own fixture and
// is the case browsers already block. This rule exists for the spelling they do not.
declare function cors(o: unknown): unknown;

export const bad = cors({ origin: true, credentials: true });

export const good = cors({ origin: 'https://app.example.com', credentials: true });
