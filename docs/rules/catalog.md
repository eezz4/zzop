# Rule catalog

Everything the engine ships today, read directly from `rules/dsl/**/*.json` and
`zzop_engine::register_all_native` (which composes `zzop_rules_graph`/`zzop_rules_http`/`zzop_rules_cross_layer`/`zzop_rules_schema`/`zzop_metrics`'s own
`register_native_analyses` — the kernel, `crates/core`, registers no ids itself). Schema/matcher
semantics: [dsl-reference.md](dsl-reference.md). How to add to this list:
[authoring-guide.md](authoring-guide.md).

**Totals** (machine-checked by `crates/engine/tests/rule_contracts/`'s `catalog_totals_match_loaded_rule_and_analysis_counts`): 14 DSL packs, 112 DSL rules, 43 native analysis ids. 11 packs ship rules; 3 are stub packs (see "Stub packs" below).

## DSL packs (`rules/dsl/<pack>/<pack>.json`)

### `be-db` (12 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `update-delete-no-where` | critical | method-scan | `no-where-ok` | `updateMany`/`deleteMany` called with no `where:` clause anywhere in the enclosing function — a whole-table write. |
| `pagination-no-orderby` | warning | method-scan | `pagination-ok` | `skip`/`take` pagination used with no `orderBy` anywhere in the enclosing function — page boundaries can shift between requests without a stable sort. |
| `client-per-request` | warning | method-scan | `prisma-client-ok` | `new PrismaClient()` constructed inside a function that also looks like a request handler — exhausts the DB connection pool under load. |
| `external-call-in-tx` | warning | method-scan | `tx-egress-ok` | A network call (`fetch`/`axios`/`got`) in the same function as a `$transaction(` — extends transaction lock hold time across a network round-trip. |
| `unawaited-write` | warning | line-scan | `unawaited-ok` | A DB write (`create`/`update`/`delete`/`upsert`) on a DB-client-shaped receiver (`prisma`/`db`/`tx`/`client`/`repo`/...) whose promise is neither awaited, returned, nor chained — fire-and-forget; a failed write looks identical to a successful one. |
| `unbounded-user-limit` | warning | line-scan | `limit-ok` | A `take`/`limit` pagination size read directly from user input (`req.query`/`req.params`/`req.body`) with no upper-bound clamp — unbounded page size, a cheap memory/CPU exhaustion vector. |
| `find-then-create-no-unique` | warning | method-scan | `find-create-ok` | A `findFirst`/`findOne`/`findUnique` read followed by `.create(` in the same function with no `connectOrCreate`/`upsert`/`ON CONFLICT` anywhere — check-then-act race, concurrent requests can create duplicate rows (a bare `$transaction` does not close it). |
| `float-money-compare` | info | line-scan | `money-ok` | A money-named identifier (`price`/`amount`/`balance`/`fee`/`cost`) compared with `==`/`===` against a float literal — IEEE754 rounding makes strict equality on monetary values unreliable. |
| `empty-catch-on-write` | warning | method-scan | `empty-catch-ok` | A DB write (`create`/`update`/`delete`/`upsert`/`updateMany`/`deleteMany`) in the same function as an empty `catch {}` — write failure is silently discarded. |
| `multi-write-no-tx` | warning | method-scan | `multi-write-tx-ok` | A create-family write (`create`/`createMany`/`insert`) and a mutate-family write (`update`/`delete`/`upsert`/...) in the same function with no `$transaction(`/SQL `BEGIN` — a failure between the two leaves partial state (co-occurrence heuristic; independent writes suppress with the marker). |
| `non-atomic-counter-update` | warning | method-scan | `atomic-counter-ok` | A `findUnique`/`findFirst`/`findOne`/`findById` read plus a `field: value +/- 1` arithmetic update in the same function with no atomic `increment:`/`decrement:`/`$inc`/`FOR UPDATE` — a read-modify-write counter that loses updates under concurrency. |
| `connection-no-release` | warning | method-scan | `connection-release-ok` | `pool.connect(`/`getConnection(`/`acquireConnection(` with no `.release(`/`.destroy(`/`.end(`, no `return conn`, and no `using`/`await using` declaration in the same function — a connection leak under load (release in a callee still fires; verify before refactoring). |

### `be-reliability` (13 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `async-route-no-catch` | warning | method-scan | `route-catch-ok` | Async Express/router handler registered with no try/catch, `next(err)`, or `.catch()` — an unhandled rejection can crash the process or hang the request. |
| `sync-fs-in-handler` | warning | method-scan | `sync-io-ok` | Synchronous fs/child_process call alongside request-handler context (`req`/`res`/`ctx`/...) — blocks Node's single event loop for every concurrent request. |
| `await-in-map` | warning | method-scan | `map-async-ok` | `.map(async ...)` used without `Promise.all`/`Promise.allSettled` — rejections become unhandled, ordering/completion guarantees are lost. |
| `env-nonnull-assert` | warning | line-scan | `env-assert-ok` | `process.env.X!` non-null assertion — defers a missing-config crash from startup to first use. |
| `debug-true-committed` | warning | line-scan | `debug-ok` | Debug flag or disabled TLS verification (`debug: true`, `NODE_TLS_REJECT_UNAUTHORIZED=0`, `rejectUnauthorized: false`) committed to source. |
| `promise-all-writes` | warning | method-scan | `promise-all-ok` | `Promise.all(...)` used alongside DB write calls (`create`/`update`/`delete`/`upsert`) — partial-failure non-atomicity, no rollback for writes that already committed. |
| `json-parse-no-try` | warning | method-scan | `json-parse-ok` | `JSON.parse(...)` called on apparent external input (`req`/`body`/`params`/`query`/...) with no surrounding `try` — malformed input throws instead of producing a handled 4xx. |
| `fetch-no-timeout` | warning | method-scan | `fetch-timeout-ok` | Outbound HTTP call (`fetch`/`axios`/`got`) on a backend-looking path with no timeout/`AbortController` visible in the same function — a hung upstream hangs the request indefinitely. |
| `process-exit-in-lib` | warning | method-scan | `process-exit-ok` | `process.exit(...)` called inside a function outside `scripts/`/`tools/`/`bin/` — skips cleanup and kills the whole server process, not just the current request. |
| `body-limit-missing` | info | line-scan | `body-limit-ok` | Body parser (`express.json`/`urlencoded`/`bodyParser.*`) configured with no explicit `limit` — relies on the implicit 100kb default: too small for some endpoints, unexamined for the rest. |
| `console-in-be` | info | line-scan | `console-ok` | `console.*` call in backend-path source (`api/`/`server/`/`backend/`/`be/`/`routes/`/`controllers/`/`services/`) — unstructured, synchronous, not queryable. |
| `interval-no-clear` | warning | line-scan | `interval-ok` | `setInterval(...)` with no matching `clearInterval(...)` anywhere in the file — a leaked timer keeps the process/page alive and re-fires forever. |
| `env-outside-config` | info | line-scan | `env-access-ok` | `process.env.X` accessed outside a config module (files/dirs named `config`/`env`/`settings` exempt) — scatters env parsing/validation instead of centralizing it. |

### `be-security` (41 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `hardcoded-secret` | warning | line-scan | `secret-ok` | Hardcoded secret-shaped literal (API key/password/token assignment, or a known cloud-key prefix). |
| `mass-assignment` | warning | method-scan | `mass-assignment-ok` | `req.body` (or a spread of it) passed directly into a database write in the same function — lets a caller set fields the handler never intended to expose. |
| `raw-query-interpolation` | critical | line-scan | `raw-sql-ok` | `$queryRawUnsafe`/`$executeRawUnsafe` called — no parameterization, so any interpolated request-derived string is a SQL injection. |
| `insecure-cookie` | warning | method-scan | `cookie-ok` | A cookie is set (`res.cookie`/`setCookie`/`cookies.set`) with no `httpOnly` anywhere in the same function body. |
| `cors-wildcard` | warning | line-scan | `cors-ok` | CORS origin set to `*` — defeats the same-origin protection CORS exists to provide. |
| `weak-password-hash` | critical | line-scan | `weak-hash-ok` | Password hashed/compared with MD5/SHA-1, or bcrypt configured at a single-digit cost factor. |
| `api-key-in-url` | warning | line-scan | `url-key-ok` | A secret-shaped query parameter (`api_key`/`access_token`/`token`/`secret`) appears in a URL — leaks via proxy/browser/`Referer` logs. |
| `annotation-sql-concat` | critical | line-scan | `query-concat-ok` | JPA `@Query` annotation built via string concatenation — attacker-controlled input spliced into SQL/JPQL. |
| `open-redirect` | warning | method-scan | `redirect-ok` | `redirect(...)` called in a function that also reads `req.query`/`req.params`/`req.body` — unchecked request-derived redirect target, a phishing/OAuth-callback token-theft vector. |
| `ssrf-user-url` | warning | method-scan | `ssrf-ok` | An outbound HTTP call (`fetch`/`axios`/`got`) made in a function that also reads `req.query`/`req.params`/`req.body` — a request-derived value can steer the server to attacker-chosen hosts (SSRF). |
| `path-traversal` | warning | method-scan | `traversal-ok` | A filesystem call (`fs`/`fsp.*`/`readFile`/`writeFile`/`createReadStream`) reads a `path.join(...)`-built path in a function that also reads `req.params`/`req.query`/`req.body` — unvalidated `..` segments escape the intended directory. |
| `cors-credentials-wildcard` | warning | line-scan | `cors-cred-ok` | `credentials: true` in a file that also configures `origin: '*'` — the wildcard origin removes the check credentialed CORS relies on, exposing cookies/auth headers cross-origin. |
| `jwt-no-expiry` | warning | method-scan | `jwt-expiry-ok` | `jwt.sign(...)` called in a function where `expiresIn` never appears — a token with no expiry, valid forever if it leaks. |
| `weak-token-random` | warning | line-scan | `weak-random-ok` | `Math.random()` used on the same line as a token/otp/nonce/session-id/secret-shaped identifier — a predictable, non-cryptographic PRNG for a security-sensitive value. |
| `timing-unsafe-compare` | info | line-scan | `timing-ok` | A secret/token/signature/hmac/api-key-shaped identifier compared with `===`/`!==` — short-circuiting equality leaks a timing side-channel. |
| `error-leak-to-client` | warning | line-scan | `error-leak-ok` | A raw error object sent directly to the client (`res.status(5xx).send/json(err)`, Hono `c.json(err)`) — stack traces/paths/SQL fragments help an attacker map internals. |
| `secret-env-in-fe` | warning | line-scan | `fe-env-ok` | A server-only-shaped env var (`SECRET`/`PRIVATE`/`SERVICE_ROLE`/`SERVICE_KEY`) referenced from frontend code — inlined into the shipped JS bundle, readable via devtools. |
| `localstorage-jwt` | warning | line-scan | `ls-token-ok` | A token/JWT-shaped value written to `localStorage` — readable by any script on the page, so one XSS bug anywhere on the origin exfiltrates it. |
| `hardcoded-password` | warning | line-scan | `java-pwd-ok` | A password-shaped literal hardcoded (direct assignment, or a JDBC `getConnection(url, user, password)` call) — a credential committed to source, can't be rotated without a code change. |
| `xxe-no-guard` | critical | method-scan | `xxe-ok` | `DocumentBuilderFactory`/`SAXParserFactory.newInstance()` with no XXE guard (`disallow-doctype-decl`/`FEATURE_SECURE_PROCESSING`) — default XML parsing resolves external entities (file read/SSRF/billion laughs). |
| `unsafe-deserialization` | warning | method-scan | `deser-ok` | `ObjectInputStream.readObject()` called — native Java deserialization of an attacker-controlled byte stream can trigger remote code execution via gadget chains. |
| `java-path-traversal` | warning | method-scan | `java-traversal-ok` | `new File(...)` constructed in a function that also reads `request.getParameter(...)` — unvalidated `..` segments escape the intended directory. **Id keeps its `java-` prefix** (documented exception): dropping it would collide with this pack's JS `path-traversal` rule above. |
| `weak-random` | warning | line-scan | `java-random-ok` | `new Random()` used on the same line as a token/session/otp/nonce-shaped identifier — a predictable, non-cryptographic PRNG for a security-sensitive value. |
| `stacktrace-to-response` | warning | method-scan | `stacktrace-ok` | An exception's stack trace/message (`printStackTrace()`/`.getMessage()`) produced in a method that also touches the HTTP response — internal class names/paths/SQL fragments can reach the client. |
| `trust-all-tls` | critical | line-scan | `trust-all-ok` | TLS certificate/hostname verification disabled (trust-all `X509TrustManager`, `ALLOW_ALL_HOSTNAME_VERIFIER`, or an always-`true` hostname-verifier lambda) — accepts any certificate for any host, opening a MITM path. |
| `conn-string-credentials` | critical | line-scan | `conn-cred-ok` | Connection-string URL with a password in the userinfo slot (`scheme://user:pass@host` — redis/postgres/mongodb/amqp/...) committed to source — repo readers own the datastore and git history preserves it; move to env/secret config AND rotate. Scans test paths too (a committed credential is leaked regardless). |
| `private-key-committed` | critical | line-scan | `private-key-ok` | A PEM private-key header (`-----BEGIN [RSA/EC/DSA/OPENSSH/...] PRIVATE KEY-----`) committed to source — the key is compromised the moment the repo is shared; rotate it and move it to a secret store. Scans test paths too. |
| `vendor-token-committed` | critical | line-scan | `vendor-token-ok` | A format-identified LIVE vendor credential (Stripe `sk_live_`/`rk_live_`, GitHub `ghp_`/`gho_`, Slack `xox[bpars]-`, Google `AIza...`) committed to source — rotate immediately; committed means leaked. Test-mode keys (`sk_test_`) deliberately do not fire. Scans test paths too. |
| `jwt-none-algorithm` | critical | line-scan | `jwt-none-ok` | `algorithm(s): 'none'` in a JWT-adjacent file — alg=none turns signature verification off entirely; no legitimate production use. |
| `shell-exec-interpolation` | critical | line-scan | `shell-exec-ok` | `exec`/`execSync` whose command string carries a `${...}` interpolation or `+`-concat — a dynamic segment inside a shell line is command injection; use `execFile`/`spawn` with an argv array (those APIs deliberately do not fire). |
| `jwt-sign-literal-secret` | critical | line-scan | `jwt-secret-ok` | `jwt.sign(payload, '<string literal>')` — a positional committed signing secret lets anyone forge tokens (the `hardcoded-secret` rule needs a `key: value` shape, so this positional form was uncovered); placeholder-word and interpolation shapes are vetoed. Scans test paths too. |
| `jwt-verify-bypass` | warning | line-scan | `jwt-verify-ok` | `ignoreExpiration: true` or `verify: false` in a JWT-library-adjacent file — token validation partially disabled. |
| `sendfile-from-request` | warning | line-scan | `sendfile-ok` | `sendFile(`/`download(` handed a `req.params/query/body` value directly — path traversal via a file-serving API (`path-traversal` covers the fs+path.join shape; this covers the serving APIs). A `path.basename(...)`-wrapped arg does not fire. |
| `cors-reflected-origin-credentials` | warning | line-scan | `cors-reflect-ok` | `credentials: true` together with `origin: true` (reflect-any-origin) or `origin: req.headers...` on one line — any site can make credentialed requests (`cors-wildcard`/`cors-credentials-wildcard` cover the literal `'*'`; this covers reflection, which those matchers miss). Multi-line option objects are a documented miss. |
| `template-unescaped-output` | warning | line-scan | `template-unescaped-ok` | Template-engine unescaped-output syntax (EJS `<%- %>`, Handlebars `{{{ }}}`, Mustache `{{& }}`) — server-rendered stored/reflected XSS if any interpolated value is user-influenced; use the escaped form or a vetted sanitizer. Scoped to `.ejs/.hbs/.mustache/.njk` template extensions (Pug `!=` is deliberately uncovered — indistinguishable from the inequality operator). |
| `html-response-from-request` | warning | method-scan | `html-response-ok` | A `res.send/write/end` of HTML-shaped content in the same function as `req.query/params/body/headers` with no sanitizer — reflected XSS (co-occurrence heuristic; the res.send-HTML sink `security/taint-flow` does not list). |
| `dangerous-html-concat` | warning | line-scan | `html-concat-ok` | An HTML tag string literal concatenated with a variable (`"<div>" + userVar`) in a response-context file — an injection sink if the variable is user-influenced; use an auto-escaping template engine or sanitizer. |
| `csp-disabled` | warning | line-scan | `csp-disabled-ok` | Content-Security-Policy turned off or wide-open (`contentSecurityPolicy: false`, `unsafe-inline`, `default-src *`) — removes the browser's last-line XSS mitigation; keep a restrictive policy. |
| `sql-taint` | warning | line-scan | `sql-taint-ok` | SQL built by string concatenation — injection risk. |
| `weak-crypto` | warning | line-scan | `weak-crypto-ok` | Weak/deprecated cryptography — MD5/SHA-1/DES/RC4/ECB. |
| `cmd-injection` | warning | method-scan | `cmd-injection-ok` | `Runtime.exec`/`ProcessBuilder` co-occurring with string concatenation in the same method — command-injection risk. |

### `browser` (9 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `no-system-dialogs` | info | line-scan | `browser-ok` | Blocking system dialog (`alert`/`confirm`/`prompt`) — freezes the page/tab (main-thread block), can't be styled or tested. |
| `no-document-write` | warning | line-scan | `document-write-ok` | `document.write`/`writeln` — breaks HTML parsing post-load, blocked under many CSP/PWA setups. |
| `postmessage-wildcard` | warning | line-scan | `postmessage-target-ok` | `postMessage(..., '*')` — a wildcard targetOrigin broadcasts the payload to whatever origin currently holds the window (opener/embedder swaps included); pass the intended origin literal. |
| `unsafe-html-sink` | warning | line-scan | `unsafe-html-ok` | A non-literal value assigned to `innerHTML`/`outerHTML`, passed to `insertAdjacentHTML`, or set as `dangerouslySetInnerHTML`'s `__html` — the standalone XSS sink check (`security/taint-flow` fires on these sinks only with a request-derived source in the same `.ts/.tsx` function; this rule needs no source — component props/state are the common carrier — and covers `.js/.jsx`). Plain string-literal assignments stay silent. |
| `javascript-url` | warning | line-scan | `javascript-url-ok` | A literal `javascript:` scheme URL in an `href`/`src` attribute, DOM property assignment, or `setAttribute` — executes arbitrary script on click/load; validate the scheme against an http(s) allowlist. Catches the literal form only (a dynamic href is a separate, harder class). |
| `location-assign-dynamic` | warning | line-scan | `location-assign-ok` | A non-literal assigned to `location`/`location.href` or passed to `location.assign/replace` — client-side open-redirect / DOM-XSS navigation sink (`be-security/open-redirect` covers the server `res.redirect(req.*)` side). Literal/absolute-path targets and `const location = useLocation()` stay silent. |
| `jquery-html-sink` | warning | line-scan | `jquery-html-ok` | A non-literal passed to jQuery `.html()`/`.append()`/`.prepend()`/... in a jQuery file — the same HTML-injection surface as `innerHTML`; use `.text()` for plain text or sanitize. |
| `vue-v-html` | warning | line-scan | `vue-v-html-ok` | Vue's `v-html` directive renders raw HTML — XSS if the bound value is user-influenced (the Vue analog of `dangerouslySetInnerHTML`); prefer `{{ }}` interpolation or sanitize. |
| `unsanitized-markdown-html` | warning | method-scan | `markdown-html-ok` | A markdown renderer (`marked`/`markdown-it`/`remark`/...) whose output reaches an HTML sink in the same function with no sanitizer — markdown renderers emit raw HTML by default; run output through DOMPurify/sanitize-html. (`.vue` SFCs are not span-projected today, so same-file `<script>`/`<template>` co-occurrence does not fire — `.ts/.tsx/.js/.jsx` does.) |

### `fullstack` (4 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `mixed-content-egress` | warning | line-scan | `mixed-content-ok` | Plain-`http://` URL literal — mixed content/MITM risk; excludes localhost/private-IP/XML-namespace lookalikes. |
| `localhost-egress-committed` | warning | line-scan | `localhost-ok` | Committed localhost/private-IP endpoint — breaks outside the dev machine. |
| `get-with-body` | warning | method-scan | `get-body-ok` | A GET request carrying a body (`method: 'get'` alongside a `body:` property in the same function) — servers/proxies may silently drop the body on a GET. |
| `ws-no-auth` | info | method-scan | `ws-auth-ok` | WebSocket opened/upgraded (`new WebSocket(...)`/`.upgrade(...)`) with no auth material (`token`/`auth`/`session`/`cookie`/`jwt`) visible in the same function — unauthenticated realtime channel. |

### `http` (3 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `read-model-path` | info | line-scan | `read-model-ok` | `apiRoutes.get(...)` with no cache-strategy marker (`// cache:`, `// no-cache:`, `// read-model-ok:`) on the same line. |
| `auth-gates` | warning | line-scan | `auth-gate-ok` | Route under a protected path segment (`/admin/`, `/internal/`, `/dev/`) whose handler identifier carries no admin/role/guard/protect keyword. |
| `route-exposure` | warning | line-scan | `route-exposure-ok` | Route under a dev/debug/internal/test/playground path segment whose handler identifier carries no guard-hint keyword. |

### `perf` (1 rule)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `api-in-loop` | warning | method-scan | `api-in-loop-ok` | Network call made inside a loop or array-iteration callback — the HTTP analogue of N+1. |

### `redis` (3 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `flushall-in-code` | critical | line-scan | `redis-flush-ok` | `flushall`/`flushdb` call or quoted command literal reachable from application code — wipes every key in the database/instance; one bug or exposed admin endpoint away from total data loss. |
| `keys-glob-scan` | warning | line-scan | `redis-keys-ok` | `KEYS` scan (`.keys('pattern')` with a string argument, or a quoted `'KEYS'` command) — O(N) walk that blocks the single-threaded server; use the SCAN cursor family or an index set. |
| `client-no-error-listener` | warning | line-scan | `redis-error-ok` | redis/ioredis client created in a file with no `.on('error', ...)` anywhere in it — node-redis emits `error` on an EventEmitter, so an unhandled listener crashes the process on the first connection blip. |

### `security` (2 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `taint-flow` | warning | method-scan | `taint-ok` | A tainted-source access and a dangerous sink call in the same function body (coarse v1 co-occurrence, not real dataflow — see the rule's own `message` for the three documented precision limits). Capped at `warning`, not `critical`: co-occurrence approximates dataflow, it doesn't prove it. |
| `eval-dynamic-code` | warning | line-scan | `eval-dynamic-ok` | `eval(` with a non-literal argument, or any `new Function(` — constructing code from strings at runtime defeats CSP and every static analyzer (`taint-flow` covers eval+request-source in `.ts/.tsx`; this rule is source-free and `.js`-inclusive). |

### `sql` (12 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `query-logic-density` | info | line-scan | `query-logic-ok` | SQL literal embeds 2+ conditional-logic constructs (`CASE WHEN` branches / `IF`·`IIF` calls). |
| `nplus1` | warning | method-scan | `n+1-ok` | `await` on a store/ORM call inside a loop or array-iteration callback — N+1 query pattern. |
| `count-in-loop` | warning | method-scan | `count-in-loop-ok` | `store.count()`/`prisma.<model>.count()` called inside a loop or array-iteration callback. |
| `app-side-aggregation-reduce` | info | method-scan | `app-agg-ok` | A `findMany()`/`prepare(...).all()` result is reduced in application code (`.reduce(...)`). |
| `app-side-aggregation-filter-length` | info | method-scan | `app-agg-filter-ok` | A `findMany()`/`prepare(...).all()` result is counted via `.filter(...).length`. |
| `race-condition-toctou` | warning | method-scan | `toctou-ok` | A read (`findOne`/`findById`/`findUnique`) feeds a branch that calls `create`/`upsert`/`insert` — TOCTOU race under concurrent requests. |
| `sql-delete-no-where` | critical | line-scan | `sql-delete-no-where-ok` | A closed SQL string literal `DELETE FROM <table>` with no `WHERE` in application code — a whole-table delete. Fires only when the entire statement is one closed literal with no interpolation/concat, so the missing `WHERE` is statically provable; migration paths are `destructive-migration`'s turf at disclosure severity. |
| `sql-update-no-where` | critical | line-scan | `sql-update-no-where-ok` | A closed SQL string literal `UPDATE <table> SET ...` with no `WHERE` in application code — a whole-table update; same closed-literal proof discipline and migration-path handoff as `sql-delete-no-where`. |
| `truncate-in-app-code` | critical | line-scan | `sql-truncate-app-ok` | A quote-anchored `TRUNCATE [TABLE] <table>` reachable from application code (migration paths excluded) — a full-table wipe one call away, the SQL analog of `redis/flushall-in-code`. |
| `destructive-migration` | info | line-scan | `sql-destructive-migration-ok` | `DROP TABLE`/`DROP COLUMN`/`TRUNCATE` — plus closed-literal whole-table `DELETE FROM`/`UPDATE ... SET` — inside a migration path. Info/non-gating by design: migrations are usually deliberate; the value is review-time attention on NEW migrations, with a two-phase (deprecate, then drop) recommendation. |
| `select-star` | info | line-scan | `sql-select-star-ok` | `SELECT * FROM` inside a string literal — over-fetch plus silent schema-drift coupling; select explicit columns (`SELECT COUNT(*)` does not fire). |
| `like-leading-wildcard` | info | line-scan | `sql-like-leading-wildcard-ok` | A `LIKE` pattern literal starting with `%` — the leading wildcard defeats index use, forcing a full scan on every call. |

### `typescript` (12 rules)

| Rule id | Severity | Matcher | Suppress marker | Detects |
|---|---|---|---|---|
| `no-explicit-any` | info | line-scan | `any-ok` | `any` type used. |
| `as-cast` | info | line-scan | `as-ok` | Flags only `as any` and `as unknown as X` (hard escapes), not every `as` cast (import-alias `as` excluded via `exclude_pattern`). |
| `unhandled-promise-use-effect` | warning | line-scan | `unhandled-promise-ok` | `useEffect` callback declared `async` — React drops the returned Promise, no cleanup possible. |
| `async-handler-no-try` | warning | method-scan | `async-handler-ok` | An `on<Event>={async ...}` JSX handler has an `await` but no `try`/`catch`. |
| `float-equality` | warning | line-scan | `float-eq-ok` | A decimal-fraction/exponent numeric literal compared with `==`/`===`/`!=`/`!==` (either order) — IEEE754 equality on computed floats is unreliable (`0.1 + 0.2 !== 0.3`); use an epsilon comparison, integer minor units, or a decimal library. Money-named identifiers are `be-db/float-money-compare`'s turf (no double-fire). |
| `always-false-comparison` | warning | line-scan | `always-false-ok` | A constant-result comparison: `NaN` on either side of `==`/`===`/`!=`/`!==` (never equal to anything — use `Number.isNaN`), and strict `=== []` / `=== {}` (reference-compares a fresh literal — use `.length === 0` / `Object.keys`). Loose `== []` is deliberately out of scope (coercion can make it true). The NaN half overlaps ESLint's `use-isnan` — zzop's value is zero-config coverage. |
| `numeric-string-comparison` | warning | line-scan | `numeric-string-cmp-ok` | A numeric-looking string literal on either side of `<`/`>`/`<=`/`>=` — string comparison is lexicographic (`'10' < '9'` is true; `'10.0.0' < '9.0.0'` is true); convert with `Number(...)` or use a semver library. Generic type arguments (`Extract<T, '1'>`) do not fire. |
| `tofixed-arithmetic` | warning | line-scan | `tofixed-arith-ok` | `.toFixed()` returns a STRING — using the result in `-`/`*`/`/`/`%` arithmetic silently coerces it back, losing the rounding intent; do arithmetic first, format last (`+`-concat for display is deliberately out of scope). |
| `date-pitfalls` | info | line-scan | `date-pitfall-ok` | Three date footguns: a date-only ISO string (`new Date('2024-01-15')` parses as UTC midnight, the slash form as local — off-by-one-day across timezones), a 10-digit epoch literal (UNIX seconds where Date wants milliseconds — yields a 1970 date), and `86400000`/`24*60*60*1000` day arithmetic (breaks across DST's 23/25-hour days). Context-dependent (fine in UTC-only systems) — hence info. |
| `foreach-async-callback` | warning | line-scan | `foreach-async-ok` | `.forEach(async ...)` ignores the returned promises entirely — the loop "completes" before any callback runs and errors become unhandled rejections; use `for...of` with `await`, or `Promise.all(items.map(async ...))` (`be-reliability/await-in-map` covers the `.map(async` sibling). |
| `promise-async-executor` | warning | line-scan | `promise-async-exec-ok` | `new Promise(async ...)` — the Promise constructor never observes the async executor's returned promise, so its rejections are swallowed; usually a redundant wrapper (overlaps ESLint's `no-async-promise-executor` — zzop's value is zero-config coverage). |
| `parseint-no-radix` | info | line-scan | `parseint-radix-ok` | Single-argument `parseInt` — the radix parameter documents intent and guards hex/legacy-octal parsing surprises; write `parseInt(x, 10)`. Nested-call arguments are a documented miss (never guessed). |

### Stub packs (0 rules — roadmap)

`conventions`, `react`, `routes` load successfully (valid, empty `rules: []`)
but currently ship no detections — each needs either declaration→use tracking, cross-repo/cross-file
joins, or JSX/AST structure the DSL can't express (see
[authoring-guide.md#when-a-rule-does-not-fit-the-dsl](authoring-guide.md#when-a-rule-does-not-fit-the-dsl)).

## Native analyses (`register_native_analyses`, one per owning crate — see below)

Whole-graph/whole-repo analyses, registered under `RuleKind::Native` so they share one
enable/severity/suppression gating surface with DSL and JS rules (`RuleConfig`). Each id is registered by
its owning crate's own `register_native_analyses` (`crates/core` itself registers none — the kernel is
rule-vocabulary-free): `rules/native/rules-graph` owns `circular`, `unreachable`, `dead-candidates`,
`dead-exports` (dependency/dead-code graph rules); `rules/native/rules-http` owns `duplicate-route`,
`unsafe-read-endpoint`/`non-idempotent-write` (the 2 call-graph scanners), `route-shadowing`,
`mutating-route-no-auth`, `unprovided-consume` (single-tree HTTP/route rules);
`rules/native/rules-cross-layer` owns the 23 `cross-layer/*` ids (multi-tree cross-layer join rules);
`rules/native/rules-schema` owns `schema-structural`, `schema-usage`, `soft-delete-bypass`,
`orderby-unindexed`, `enum-string-drift`; `crates/metrics` owns `seams`, `criticality`, `scores`,
`health`, `recommendations` (score computations, not findings-producing rules — they only ride the same
toggle/gating surface). `zzop_engine::register_all_native` composes all five. The 23
`cross-layer/*` ids are the MULTI-TREE exception: they run over `zzop_engine::analyze_trees`'s joined
`CrossLayerResult` (every other row here runs per-tree), exposed as `crossLayerFindings` alongside
`crossLayer` in `analyzeTrees`'s output. None of the 23 honor an inline suppression marker (disable-only, via
`disabledRules`) — see `rules/native/rules-cross-layer/src/cross_layer/mod.rs`'s module doc.

| Id | Default severity | Detects |
|---|---|---|
| `circular` | warning | Import cycles in the dependency graph (Tarjan SCC, `graph.rs`). |
| `unreachable` | info | Closed "dead islands" — files imported in-repo (fan-in > 0) yet unreachable from any entrypoint (`unreachable.rs`). |
| `dead-candidates` | info | File-level dead-code candidates: fan-in == 0 and not an entry-point pattern (tests/Storybook/dev-tool config/`.d.ts` excluded) (`dead_candidates.rs`). |
| `dead-exports` | info | Symbol-level dead-export detection — exported symbols never imported anywhere, with unused-vs-in-file-only reasons (dev-tool config files excluded) (`dead_exports.rs`). Per-tree: when a monorepo is analyzed as multiple separate trees, this rule reasons within each tree — a symbol imported only from another tree via a deep path (no barrel `export *`) can read as never-imported in its own tree and be reported dead. Barrel re-exports (`export *`) keep such public API alive. Recommendation: rely on barrels for cross-tree public API, or analyze cross-tree-shared packages as their own tree whose entry (index) exposes them. |
| `seams` | info | Strangler-seam scoring — folders that are self-contained (few boundary-crossing import edges), i.e. good first-extraction candidates (`seams.rs`). |
| `criticality` | warning | Transitive blast-radius scoring — surfaces stable-but-critical files a churn-weighted risk score underweights (`criticality.rs`). |
| `scores` | info | 17 structural health scores, 0–100 (`scores/compute.rs`). |
| `health` | info | Composite structural-health index rolling the per-metric scores up into one number (`health.rs`). |
| `recommendations` | info | ROI-ranked improvement recommendations derived from `FileNode`s, coupling, and circular deps (`recommendations.rs`). |
| `schema-structural` | warning | Prisma schema structural rules — god-model, missing timestamps, FK with no index, nullable FK, implicit FK, float-as-money, temporal-as-string, redundant index, stale `updatedAt` (`rules/native/rules-schema/src/structural.rs`). |
| `schema-usage` | warning | Prisma schema usage-aware cross-check — dead model (a model whose name is never referenced in source, unless a `bound-model` attribute is injected on its symbol) / dead field, plus migration churn (a Mode-B-injected `model-churn` attribute on the model's symbol), layered on the structural rules (`rules/native/rules-schema/src/usage.rs`). |
| `unsafe-read-endpoint` | warning | A GET/HEAD ("safe") endpoint whose handler reaches a database/store write via call-graph BFS — violates the safe-method contract. |
| `non-idempotent-write` | warning | A write endpoint (PUT/DELETE always; POST/PATCH for accumulation only) that reaches a non-idempotent create, atomic-accumulate, or counter-bump operation via call-graph BFS — a retry duplicates or doubles the effect. |
| `duplicate-route` | warning | The same `(METHOD, path)` HTTP route provided 2+ times across the tree. |
| `soft-delete-bypass` | warning | A `findMany`/`findFirst`/`findUnique`/`count` call site on a model with a `deletedAt`/`deleted_at` marker field whose argument span never mentions that field — a soft-deleted row can leak back into a "live" read (`rules/native/rules-schema/src/join.rs`). |
| `orderby-unindexed` | warning | A single-field literal `orderBy: { field: 'asc' }` naming a field with no `@id`/`@unique`/leading-`@@index` coverage on the target model — an unindexed sort that gets slower as the table grows (`rules/native/rules-schema/src/join.rs`). |
| `enum-string-drift` | warning | A literal-object `field: 'Literal'` at a query call site whose field resolves to exactly one declared schema enum, where `'Literal'` is not one of that enum's members — a string that drifted out of sync with the enum (`rules/native/rules-schema/src/join.rs`). |
| `route-shadowing` | warning | Within one file, a param-segment route (`/x/{}`) registered earlier than a same-shape literal-segment route of the same method makes the later literal route unreachable in a first-match router (Express/Koa/Fastify-style) (`rules/native/rules-http/src/route_shadowing.rs`). |
| `mutating-route-no-auth` | info | A POST/PUT/PATCH/DELETE route whose handler's call-graph BFS never reaches a callee named like an auth guard (e.g. `auth`/`guard`/`verify`/`session`/`token`/`permission`/`acl`/`owner`/`admin`/`role` — non-exhaustive; the full vocabulary is `DEFAULT_AUTH_GUARD_PATTERN` in `rules/native/rules-http/src/mutating_route_no_auth.rs`). The BFS can't see route-level middleware, but common Express guard registrations (`app`/`router.use(guard)`, a route-level guard argument, well-known callees like `passport.authenticate`) are now recognized natively and exempt a route through the same `auth-guarded` attribute (on the route's `ioKey` or a `pathScope` prefix) as the generic entity-attribute channel — registration order isn't modeled, so a recognized guard covers its scope regardless of where it sits relative to the route. A non-Express framework or a project's own custom middleware naming still needs a producer/adapter to inject the attribute; native vocab, native middleware recognition, and injected evidence all compose (`rules/native/rules-http/src/mutating_route_no_auth.rs`). |
| `unprovided-consume` | info | An HTTP `IoConsume` whose key matches no `IoProvide` anywhere in the analysis, gated to trees that provide at least one HTTP route themselves — a typo'd path, a renamed/removed backend route, or a route this analysis failed to parse. Unmatched consumes are split by first-path-segment overlap with the tree's own provided key space: overlapping ones stay individual; "foreign" ones (no overlap) fold into ONE aggregate finding once 3+ accumulate, enumerating every folded key (below 3, foreign consumes also stay individual) (`rules/native/rules-http/src/unprovided_consume.rs`). |
| `cross-layer/unconsumed-endpoint` | info | A `crossLayer.unconsumedProvides` `http` entry — an endpoint no tree in this `analyzeTrees` run calls. Caveats consumers outside the analysis (another repo, a mobile client, an unresolved dynamic URL) may still exist. A provide identified as a tRPC mount route (a literal `trpc` path segment, e.g. `/api/trpc/{}`) is excluded when this run produced 1+ `trpc`-kind cross-layer edge — the mount route IS the transport those edges flow through, so "unconsumed" would be tone noise; the exclusion is disclosed via a `warnings` entry on the owning tree, never silent (`rules/native/rules-cross-layer/src/cross_layer/unconsumed_endpoint.rs`). |
| `cross-layer/method-mismatch` | warning | A `crossLayer.unprovidedConsumes` `http` consume whose path exactly matches a provide somewhere in the analysis, but the method differs (e.g. FE calls `POST /api/users`, only `GET /api/users` is provided) (`rules/native/rules-cross-layer/src/cross_layer/method_mismatch.rs`). |
| `cross-layer/version-skew` | warning | A `crossLayer.unprovidedConsumes` `http` consume whose key differs from a provide only in one version-shaped path segment (`/v1/` vs `/v2/`) (`rules/native/rules-cross-layer/src/cross_layer/version_skew.rs`). |
| `cross-layer/path-near-miss` | info | A `crossLayer.unprovidedConsumes` `http` consume whose key matches a provide once `{}` parameter positions are allowed to differ, but is otherwise segment-identical — strict elsewhere (a plural/typo literal difference does not count). Gated on the consume side: a consume whose path is ALL `{}` placeholders (a head-drop artifact, e.g. `GET /{}`) carries no literal evidence and never fires here; an all-`{}` provide is ungated and stays a legitimate suggestion target (`rules/native/rules-cross-layer/src/cross_layer/path_near_miss.rs`). |
| `cross-layer/route-near-miss` | info | A `crossLayer.unprovidedConsumes` `http` consume whose key differs from a same-method provide by EXACTLY ONE structural dimension — `case` (letter casing) or `prefix` (an all-literal 1-2 segment leading base path added/removed, e.g. `/api`) — disjoint from `path-near-miss`'s same-count parameter-generalization case; names the exact dimension so the fix is actionable. Same consume-side all-`{}`-placeholder gate as `path-near-miss` (`rules/native/rules-cross-layer/src/cross_layer/route_near_miss.rs`). |
| `cross-layer/prefix-drift` | info | An aggregate over `route-near-miss`: when 3+ `http` consumes from one tree all near-miss providers in another tree by the SAME missing/extra base path prefix (e.g. every FE call omits the `/api` a NestJS `setGlobalPrefix` adds), reports ONE finding naming the single likely base-path/gateway/baseURL cause and enumerating every folded route, instead of N near-identical per-route findings. The subsumed per-route `route-near-miss` findings are replaced (not silently dropped — the aggregate lists them). Derived from `route-near-miss`, so it only fires when that rule is enabled (`rules/native/rules-cross-layer/src/cross_layer/prefix_drift.rs`). |
| `cross-layer/shared-db-table` | warning | The same `db-table` key CONSUMED (not provided) by 2+ distinct source trees — evidence of a naming collision or a genuinely shared database; message says to verify which (`rules/native/rules-cross-layer/src/cross_layer/shared_db_table.rs`). |
| `cross-layer/duplicate-route` | warning | The same `http` `(method, path)` key PROVIDED by 2+ DISTINCT source trees — the cross-tree counterpart to `duplicate-route` above (`rules/native/rules-cross-layer/src/cross_layer/duplicate_route.rs`). |
| `cross-layer/external-shadow-internal` | warning | A `crossLayer.externalConsumes` consume (absolute URL) whose normalized method+path matches a route an analyzed tree provides — the caller hardcodes one environment's host instead of the relative/proxied path (`rules/native/rules-cross-layer/src/cross_layer/external_shadow_internal.rs`). |
| `cross-layer/external-secret-in-url` | warning | A `crossLayer.externalConsumes` consume whose URL query string carries a secret-named parameter (`token`/`key`/`apikey`/`secret`/...) — credentials in URLs leak through logs, referrers, and history, whether the value is a literal or interpolated (`rules/native/rules-cross-layer/src/cross_layer/external_secret_in_url.rs`). |
| `cross-layer/external-duplicated-integration` | warning | The same external host called directly from 2+ distinct source trees — a duplicated third-party integration; centralize behind one client or a backend proxy (`rules/native/rules-cross-layer/src/cross_layer/external_duplicated_integration.rs`). |
| `cross-layer/external-host-fanout` | info | The same external host called directly from 3+ distinct files — vendor calls scattered across the codebase instead of centralized in one client module (`rules/native/rules-cross-layer/src/cross_layer/external_host_fanout.rs`). |
| `cross-layer/external-base-url-drift` | info | The same external path consumed against 2+ different hosts (port included) — base-URL/config drift for what looks like one logical service (`rules/native/rules-cross-layer/src/cross_layer/external_base_url_drift.rs`). |
| `cross-layer/external-version-inconsistent` | info | One external host consumed through both version-shaped (`/v1/...`) and versionless paths — inconsistent API version pinning against the same vendor (`rules/native/rules-cross-layer/src/cross_layer/external_version_inconsistent.rs`). |
| `cross-layer/external-ip-literal` | warning | A `crossLayer.externalConsumes` consume whose host is a raw IP literal (loopback excluded — committed localhost URLs are the DSL `localhost-egress-committed` rule's turf) — environment-specific addressing committed into code (`rules/native/rules-cross-layer/src/cross_layer/external_ip_literal.rs`). |
| `cross-layer/ambiguous-consume` | warning | A consume whose key is provided by 2+ distinct trees (`crossLayer.ambiguousConsumes`) — which provider actually serves the call depends on deploy-time routing the analysis cannot see (`rules/native/rules-cross-layer/src/cross_layer/ambiguous_consume.rs`). |
| `cross-layer/unconsumed-mutation-endpoint` | warning (downgraded to info when the run has a blind source) | A `crossLayer.unconsumedProvides` `http` entry with a write method (POST/PUT/PATCH/DELETE) — an unconsumed mutation endpoint is standing attack surface, not just dead code; intentionally co-fires with `cross-layer/unconsumed-endpoint`. Severity is conditional: when this run has 1+ source whose `http` consumes are majority-unresolved (the same blindness `cross-layer/unresolved-consume-ratio` self-reports), a confident "unconsumed" verdict is not warranted, so the finding fires at info instead and the message names the blind source(s) plus the quantified unresolved-consume count — the finding still fires either way (never suppressed), only the severity and framing change. Same tRPC mount-route exclusion as that rule (see its catalog entry) applies here too (`rules/native/rules-cross-layer/src/cross_layer/unconsumed_mutation_endpoint.rs`). |
| `cross-layer/unprovided-mutation-call` | warning (downgraded to info when the run has a provide-blind source) | A `crossLayer.unprovidedConsumes` `http` consume with a write method — a state-changing call whose target no analyzed tree provides; intentionally co-fires with the unprovided-diagnosis rules above. Severity is conditional: when this run has 1+ source that imports a server framework yet extracted almost no `http` routes tree-wide (the same near-zero condition the engine's framework-silence self-report fires on), a confident "no matching provide anywhere" verdict is not warranted, so the finding fires at info instead and the message names the blind source(s) — the finding still fires either way (never suppressed), only the severity and framing change. The provide-side mirror of `cross-layer/unconsumed-mutation-endpoint`'s consume-blind downgrade above (`rules/native/rules-cross-layer/src/cross_layer/unprovided_mutation_call.rs`). |
| `cross-layer/route-shadowing` | warning | A `{}`-parameter route pattern provided by one tree that would shadow a same-method, same-shape literal route provided by a DIFFERENT tree if both are served behind one first-match gateway — the cross-tree counterpart to `route-shadowing` above (`rules/native/rules-cross-layer/src/cross_layer/cross_tree_route_shadowing.rs`). |
| `cross-layer/unresolved-consume-ratio` | info | A tree whose `http` consumes are majority-unresolved (dynamic URLs, generated SDK clients, wrapper functions) — self-reports that the cross-layer join is mostly blind for that tree instead of staying silent (`rules/native/rules-cross-layer/src/cross_layer/unresolved_consume_ratio.rs`). |
| `cross-layer/sdk-import-no-visible-consume` | info | A tree importing an SDK-shaped package (`@scope/sdk`, `*-sdk`, `openapi*`, `*api-client*`) from 3+ files, OR an opaque HTTP client library (`superagent`, `got`, `node-fetch`, `oazapfts`, ...) the egress extractor cannot trace at all, from 1+ files, while having fewer visible `http` consumes than `unresolved-consume-ratio`'s floor — consumption flows through a client the egress extractor cannot see; the not-even-visible half of the blind-spot partition. `oazapfts` joined the opaque-client list once its native recognition retired in favor of `examples/oazapfts-adapter` (Mode B). Rule id kept for compatibility even though scope now covers both classes (`rules/native/rules-cross-layer/src/cross_layer/sdk_import_no_visible_consume.rs`). |
| `cross-layer/unconsumed-procedure` | info | A tRPC procedure (kind `trpc`, key `"VERB dotted.path"`, composed at assembly from cross-file router fragments) that no analyzed tree calls — TypeScript's compiler catches calls to nonexistent procedures but not unused definitions. Caveats server-side `createCaller`/SSR consumers this analysis cannot see (`rules/native/rules-cross-layer/src/cross_layer/unconsumed_procedure.rs`). |
| `cross-layer/body-field-drift` | warning | A matched `http` edge whose FE-witnessed request-body literal (`body-shape-v1`) disagrees with the BE handler's resolved `@Body()` DTO: a required field the DTO declares but the FE literal never sets (only when the FE literal is otherwise exhaustive at that level), an undeclared key the FE sends (only when the DTO's own field list is complete), or a missing `@Body('subKey')` wrapper key entirely. Anchored at the consume, citing the DTO's `file:line`; caveats that this is a witnessed-literals-only comparison — interceptors/transforms can add or strip fields (`rules/native/rules-cross-layer/src/cross_layer/body_field_drift.rs`). |

### Roadmap

Architecture rules (layer-violations, feature-envy) are not yet implemented — no crate is scaffolded for
them yet; the placeholder `rules/native/rules-architecture` crate was removed since it carried no code (see
`rules/README.md`). When this work starts, it lands as a new `rules/native/` crate at that point, not before.
Other detections not yet shipped in either layer: cognitive/nested-loop complexity
scoring, precise `taint-flow` dataflow (the current `security/taint-flow` is a documented coarse v1
co-occurrence check), an auth-state-machine analysis, additional cross-file HTTP graph checks (API
churn, frontend/backend spec drift), a JSX/React structural rule pack, and env/i18n sync checks. Each
needs either a whole-graph join the DSL can't express or real AST/JSX
shape — see [authoring-guide.md#when-a-rule-does-not-fit-the-dsl](authoring-guide.md#when-a-rule-does-not-fit-the-dsl).
(Raw-Worker route extraction — manual `url.pathname` dispatch in framework-less Workers/Node servers —
shipped as the parser's `pathname-dispatch` provide vocabulary, so those routes now feed the cross-layer
join like any framework's.)
