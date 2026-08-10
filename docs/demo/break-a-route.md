# Break a route: the drift a single-repo tool can't see

zzop joins a frontend and a backend on the **HTTP/DB contract** they share — `METHOD /path`, `table:name` — even when the two are **separate repositories that never import each other**. This demo changes one backend route and shows zzop pinpoint the resulting drift on *both* sides, while the frontend keeps compiling and its tests keep passing.

This page is a **narrated walkthrough**: every command and the output it produced are written out below,
so it reads end to end without running anything.

**Reproducing it yourself needs two things this repository does not ship:**

- a **source checkout** with a working Rust toolchain — the run builds a `cargo` example
  (`zzop-engine`'s `xlayer_dump`), so a released binary is not enough; and
- the **two corpus trees**, at `corpus/oss/fe-vite` and `corpus/oss/be-express`. `corpus/oss/` is
  gitignored and no lane in this repo fetches it — bring your own pair, the way
  [CONTRIBUTING.md](../../CONTRIBUTING.md) describes. Any independently-authored frontend/backend pair
  demonstrates the same thing; those two paths are simply what the script hardcodes.

With both in place, the script replays the whole sequence (and restores the corpus on exit):

```bash
bash docs/demo/break-a-route.sh
```

## The setup

Two independently-authored [RealWorld](https://github.com/gothinkster/realworld) apps are vendored under `corpus/oss/`:

| tree | stack | role |
|---|---|---|
| `fe-vite` | React + valtio + react-query, MirageJS-mocked | frontend |
| `be-express` | Express + Prisma | backend |

Neither repo depends on the other. The contract between them lives in **runtime strings** on both sides:

- frontend — `corpus/oss/fe-vite/src/pages/Settings.jsx:19`
  ```js
  const { data } = await axios.put(`/user`, { user: values })
  ```
- backend — `corpus/oss/be-express/src/app/routes/auth/auth.controller.ts:61`
  ```ts
  router.put('/user', auth.required, async (req, res, next) => { … })
  ```

Baseline: zzop resolves **79 cross-layer EDGES** — 19 HTTP-route edges + 60 db-table edges — and `PUT /api/user` is one matched edge (that `axios.put` ↔ that `router.put`).

The metric is **edges**, one per provide×consume pair — not distinct keys. Those 60 db-table edges run
over only **4** distinct table keys, because several files on each side touch the same table, so
"60 shared DB tables" would be a different and much larger claim than the tool actually makes.
Recount both halves rather than trusting this paragraph:

```sh
cargo run --release -q -p zzop-engine --example xlayer_dump -- \
  corpus/oss/fe-vite corpus/oss/be-express \
  | sed -n '/^=== edges/,/^=== unprovided/p' | grep '^  ' | sed 's/^  //' | sort | uniq -c
```

Your own vendored pair may differ from ours; the shape of the story does not depend on the totals, only on the ONE edge that disappears.

## The break

An ordinary REST tidy-up on the backend — the kind that sails through review — renames the route:

```diff
- router.put('/user',    auth.required, …)
+ router.put('/users/me', auth.required, …)
```

The frontend is **not touched**.

## What zzop reports

```
=== edges (78) ===            ← was 79; the PUT /api/user edge is gone

=== unprovided consumes (2) ===
  Some("PUT /api/user")  @ fe-vite     src/pages/Settings.jsx:19          ← the FE call now hits nothing

=== unconsumed provides (6) ===
  "PUT /api/users/me"    @ be-express  src/app/routes/auth/auth.controller.ts:61   ← the BE route nobody calls
```

Both ends of the break, located to the file and line, across two repos that share no code.

## Why the frontend's own tooling stays green

- **`tsc` / `vite build`**: `axios.put(\`/user\`, …)` is a string literal. The type system has nothing to check it against — the backend route is not a type the frontend imports. The build is clean.
- **Frontend tests**: `fe-vite` mocks its own API with MirageJS (`src/server.js` still handles `PUT /user`), so every test passes against the *frontend's own idea* of the contract — which is now stale.

A linter, a type-checker, or a test suite scoped to one repository is structurally incapable of seeing this drift: the evidence is split across two repos and never crosses a compiler boundary. zzop sees it because its cross-layer join is an exact `(kind, key)` match over each tree's projected interface facts — it needs no shared types, no running services, and no test harness.

## The general shape

This is not specific to a renamed path. The same join surfaces:
- a changed HTTP **verb** (`POST` → `PUT`) — `unprovided` on one side, `unconsumed` on the other;
- a **removed** endpoint the frontend still calls;
- a request/response **body-field** the handler stopped returning (via the body-shape channel);
- a **DB column/table** two services disagree on.

Anywhere a frontend and backend are wired by convention rather than a shared, compiler-checked type, the contract can drift silently. zzop is the check that spans the gap.
