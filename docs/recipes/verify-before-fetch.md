# Verify before you fetch

**The situation.** You are an agent working inside the frontend repo. The code calls
`GET /api/articles/{id}/favorite`. The backend lives in another repo that you have not cloned — or
have cloned and cannot afford to read end to end. You want one thing: *does that endpoint exist over
there, and is it spelled the way my code spells it?*

This page is the **workflow** for that question: which calls to make, in which order, and — the part
that actually decides whether the answer is worth anything — **how to tell a real absence from zzop
not having seen it**.

It is not the tool reference. The tool surface (names, arguments, reply fields, caps) is
[modules/mcp.md](../modules/mcp.md); the reply contract behind it is
[modules/facade.md](../modules/facade.md). Both are cited inline rather than restated, because a
recipe that re-spells an argument list is a second copy that will rot.

Every command below has a CLI twin and an MCP twin running the *same* handler
(`zzop endpoint` ↔ `check_endpoint`, `zzop cross` ↔ `cross_repo` — see
[CLI surface](../modules/mcp.md#cli-surface)), so the shell examples and the tool calls answer
identically. Shell examples use the CLI because it is copy-pasteable; an MCP client sends the tool
call with the same arguments.

> On Windows Git Bash/MSYS, a leading-slash pattern (`/api/...`) is rewritten by the shell before
> zzop sees it. See the caveat at the end of [CLI surface](../modules/mcp.md#cli-surface).

---

## Step 0 — What your own repo can tell you, with nothing fetched

Point `check_endpoint` at **one** tree — yours. The cross-layer join still runs (over a single
source, intra-tree edges included), so you get a verdict without the other repo present:

```sh
zzop endpoint "/api/" ./web        # or: check_endpoint({ pattern: "/api/", path: "<abs>/web" })
```

Two things this is genuinely good for:

1. **Reading back the key your code will actually send.** The join keys on a normalized
   `"METHOD /path"` string, and composing it is not always trivial — an `axios.defaults.baseURL` or
   a NestJS global prefix is folded into the key before the join sees it. The `matches` entries
   carry `file`/`line`, so you can see which call site produced which key.
2. **Finding out whether your own consume side is visible at all** — see Step 2. A frontend whose
   HTTP calls go through a wrapper the extractor does not recognize produces few or no keys, and
   that is something you want to know *before* you draw conclusions from a join.

**What it cannot tell you: anything about the other repo.** With one tree in the request, nothing
provides your routes by construction, so a `consumed-unprovided` verdict here is a statement about
the request, not about the backend. Measured on `corpus/oss`, same pattern, same binary:

| Request | Verdict |
|---|---|
| `zzop endpoint "/api/articles/{}/favorite" ./fe-vite` | `consumed-unprovided` (2 unprovided consumes) |
| `zzop endpoint "/api/articles/{}/favorite" ./fe-vite ./be-express` | `linked` (2 edges) |

The route was there the whole time. This is the single most common way to misread this tool.

---

## Step 1 — Attach the other tree, then ask

Once the backend is on disk, put both roots in one request — `paths` (each root loads its own config, each tree tagged
by its directory name) or `configPath` (a `zzop.config.jsonc` whose `trees` define the join). The
exact argument rules, including "exactly one source form", are in
[modules/mcp.md](../modules/mcp.md#mcp-surface).

Run **two** calls, in this order:

```sh
zzop cross ./web ./api                       # once — the coverage/warning read (Step 2)
zzop endpoint "/api/articles/{}/favorite" ./web ./api   # per key — the verdict (Step 3)
```

The order is load-bearing. `check_endpoint`'s reply carries `pattern`, `verdict`, `counts`,
`matches`, `relatedFindings`, `suggestions` (on `not-found`), the run-global `disclosure` fold,
and this host's `config`/`configWarnings` — **and no per-tree `coverage` or `warnings`**. Those are
the blindness signals, and they ride `cross_repo`/`analyze_repo` replies instead
([Output contract](../modules/mcp.md#output-contract)). A verdict read without them is a verdict you
cannot calibrate.

---

## Step 2 — Read the coverage signals *before* the verdict

Four signals, all from the `cross_repo` reply, each ruling out a different way a "no" can be false.

| Signal | Where it appears | What a hit means |
|---|---|---|
| `coverage.joinContributionZero` | `sources[].coverage`, a boolean | That tree analyzed files but extracted **zero joinable io** — it is structurally invisible to the join. Any bucket verdict about it is meaningless. Field semantics: [facade.md](../modules/facade.md#the-zzop-facade-json-contract). |
| Framework-silence self-reports | `sources[].warnings`, prose strings | A lexical tripwire fired: the tree looks like it carries a server framework, an HTTP client, an ORM schema, a committed OpenAPI spec, or a hand-rolled `fetch` wrapper, while the matching io channel stayed near-zero. The tripwires and their gates are documented in `crates/engine/src/framework_silence.rs`. |
| Unparsed-extension diagnostics | `sources[].warnings`, prose strings | Files whose extension has no native parser, named per extension with counts and sample paths (`crates/engine/src/analyze/diagnostics/capability.rs`). If the other side's routes live in one of those, they were never candidates for the join. |
| `coverage.ioConsumesUnresolved` | `sources[].coverage`, a count | Call sites the extractor saw but could not statically key. They cannot join — they are not evidence of absence either way. |

A worked negative, measured on `corpus/oss`:

```
zzop endpoint "articles" ./fe-redux ./be-express
  → verdict: "provided-only", counts.unconsumedProvides: 11
```

Read alone, that says "the backend serves 11 article routes and the frontend calls none of them" —
which would be a real finding. The `cross_repo` reply for the same pair says otherwise:

```
sources[fe-redux].coverage: { ioProvides: 0, ioConsumesKeyed: 0, joinContributionZero: true, ... }
sources[fe-redux].warnings: [ "... http-client package(s) imported but only 0 http consume site(s)
                               were extracted tree-wide: superagent (1 file(s), e.g. src/agent.js) ..." ]
```

The frontend calls its backend through `superagent`, an idiom this extraction pass did not
recognize. `provided-only` was a fact about zzop's sightline, not about the code. **Report a
negative only when the tree that would have to supply the missing half was actually visible.**

There is also a run-global prior riding the `analyze_repo`/`cross_repo`/`check_endpoint` replies: a pinned
list of the ways zzop's own output can be silently wrong, each entry carrying a `status` of `asserted` /
`partial` / `notYetDetected`
([contract](../modules/facade.md#disclosure--silent-failure-class-registry-run-global); source of truth
`crates/engine/src/disclosure.rs`).

**Since 2026-07-29 the reply carries a FOLD of that list, not the list.** `disclosure` in a shaped reply is
an object — the counts (`classes`, `asserted`, `partial`, `notYetDetected`) plus a `note`, a `resource` and
a `command`. The list is identical on every run, which is exactly why the prose ships once instead of per
call, so **read the entries once from the contract lane and keep them**:

```sh
zzop contract disclosure-classes          # CLI
# MCP: resources/read  zzop://contract/disclosure-classes
```

`consume-side-unextracted`, `provide-side-unextracted`, `language-unparsed`,
`generated-client-unrecognized` and `key-mismatch-drift` are the entries this workflow leans on. What the
reply still tells you without asking is that gaps exist and how many — the counts move the moment the
registry grows, and a test enforces that.

---

## Step 3 — Act on the verdict

`verdict` is one token from a sealed vocabulary. What each token *means* — which join bucket it is
derived from — is stated once in [Endpoint queries: `queryIo`](../modules/facade.md#endpoint-queries-queryio)
and enforced in `crates/facade/src/query.rs`; that is the list, not this table. This table adds only
what to **do**, and each row assumes Step 2 came back clean for the tree that would carry the other
half.

| Verdict | Next action |
|---|---|
| `linked` | Done. `matches.edges[]` names the providing `file`/`line` on the other tree — read that, not the whole repo. |
| `provided-only` | The other side serves it, nothing in this run calls it. If *your* tree is the caller, your call site was not extracted (Step 2) — do not report dead code. |
| `consumed-unprovided` | The actionable drift case, **if** the provider tree was in the request and visible. Before asking for the route to be built, check `relatedFindings` (below) for a near-miss — a rename is far more likely than a missing route. |
| `unresolved-only` | The key could not be determined statically. This is a limit of the extraction, **not** evidence of absence. Make the key literal at the call site, or project it with an overlay (below), then re-run. |
| `external` | Third-party egress. Not your backend's contract; stop here. |
| `ambiguous` | 2+ trees in the request provide the key. Each matched entry carries its own `candidates` array (inside `matches.ambiguousConsumes[]`, not at the top level) listing every provider with `source`/`file`/`line` — pick which one your call actually reaches. |
| `mixed` | 2+ classes matched the pattern. `counts` disambiguates; usually the pattern was too broad. Re-query with the full `"METHOD /path"` key. |
| `not-found` | Nothing matched — the weakest possible signal, and the one most often over-read. Go to *Reading a `not-found`* below. |

The pattern is a **case-insensitive substring** over every io key, so a narrow-looking pattern can
match a neighbour: `"PUT /api/user"` against `fe-vite` + `be-express` returns `mixed`, because
`PUT /api/users/me` contains it. Query the full normalized key when you want a definitive answer
about one route.

### `relatedFindings` is where the rename shows up

A `consumed-unprovided` alone does not tell you *why*. The findings attached to the reply do.
Several cross-layer rules exist precisely to name the one dimension a key differs by — casing or a
base-path prefix (`cross-layer/route-near-miss`), a parameter position (`cross-layer/path-near-miss`),
the HTTP method (`cross-layer/method-mismatch`), a version segment (`cross-layer/version-skew`), and
the aggregate that fires when many routes all drift by the *same* prefix
(`cross-layer/prefix-drift`). Their exact conditions are in [rules/catalog.md](../rules/catalog.md).
Check these before concluding a route does not exist.

### Reading a `not-found`

`not-found` means no io key in this analysis contained your pattern. Four things produce it, and
they are distinguished by evidence you already have on hand:

1. **The provider tree is not in the request.** Check the reply's own `config` /`configWarnings`,
   and the source list on the `cross_repo` reply. Cheapest to rule out; check it first.
2. **The provider tree is in the request but was not seen.** Step 2's signals on that source.
3. **The key was never resolved.** `coverage.ioConsumesUnresolved` on the calling tree, or an
   `unresolved-only` verdict for a looser pattern over the same area.
4. **It exists under a different string.** `not-found` replies carry `suggestions`. Two passes
   produce them and they mean different things, so do not read the list without knowing which one
   answered. The facade's own pass is substring-driven over the io keys — its exact rule is the
   `suggestions` row in [facade.md](../modules/facade.md#endpoint-queries-queryio). When that pass
   returns nothing, `check_endpoint` falls back to a deterministic nearest-key ranking by edit
   distance (`crates/summary/src/suggest.rs`, which also states the distance cut-off that keeps a
   garbage pattern returning `[]`). Both are capped at the same limit, and neither is a guess the
   reply hides: an empty list means both passes found nothing.

   Two measured cases, opposite readings:

   * `"/api/payments"` against `fe-vite` + `be-express` → `not-found` with ten
     `articles`/`profiles`/`tags` suggestions from the **substring** pass. Read: *that surface is
     visible and yours is not on it* — a much stronger negative than a bare `not-found` with an empty
     suggestion list.
   * `"atricles"` against the same pair → `not-found` with ten `…/api/articles…` suggestions, none of
     which shares a segment with the pattern, so they came from the **nearest-key** pass. Read the
     opposite way: *the key exists and you spelled it wrong.* Compare each suggestion to your pattern
     before you conclude anything — a `not-found` next to a near-identical key is a typo report, not
     an absence.

Only when all four are ruled out is "the endpoint does not exist" the honest reading.

### One thing the join structurally cannot see

The join keys on what the **source code** says. Some in-code prefix sources are composed into the
key first (an axios `defaults.baseURL`, a NestJS global prefix). Anything that rewrites the path
*outside* the analyzed source — a dev-server proxy, an ingress rule, an API gateway — is not an
input to it, and there is no field in which its absence is disclosed for a specific route. The
symptom to watch for is a *systematic* prefix difference rather than a single missing route, which
is what `cross-layer/prefix-drift` reports.

---

## Step 4 — Restoring visibility instead of accepting the blind spot

When Step 2 says a tree is dark, the fix is to feed the missing channel rather than to distrust the
tool permanently. A Mode B overlay projects external symbols on top of a natively-parsed tree, and a
partial envelope covering just the consume (or provide) channel is enough to make the join work —
the contract is [NORMALIZED_AST.md](../NORMALIZED_AST.md), reachable without a checkout as the
`envelope-guide` resource over MCP or `zzop contract envelope-guide` on the CLI. zzop's own
framework-silence warnings already say this and name the missing channel.

## Before / after the fetch, at a glance

| | Without the other repo | With the other tree attached |
|---|---|---|
| The normalized key your code sends | yes | yes |
| Whether your own consume side is visible | yes | yes |
| Whether the route exists over there | **no** | yes, subject to Step 2 |
| Which file/line serves it | no | yes (`matches.edges[]`) |
| Near-miss / rename diagnosis | no | yes (`relatedFindings`) |
| Whether the other tree was visible at all | no | yes (`cross_repo` coverage + warnings) |

Everything in the left column comes from a single-tree run and is cheap. Everything in the right
column needs the tree on disk — but it needs the *tree*, not a reading of it. That is the whole point
of the workflow: a clone plus two calls, instead of reading the other repo's routing layer, for the
routes those two calls can actually see.

See also: [getting-started.md](../getting-started.md) (install and first run),
[demo/break-a-route.md](../demo/break-a-route.md) (the same join watched while a route is
deliberately broken), [modules/mcp.md](../modules/mcp.md) (tool surface),
[modules/facade.md](../modules/facade.md) (reply contract).
