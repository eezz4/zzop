# Break a route: the drift a single-repo tool can't see

zzop joins a frontend and a backend on the **HTTP/DB contract** they share — `METHOD /path`, `table:name` — even when the two are **separate repositories that never import each other**. This demo changes one backend route and shows zzop pinpoint the resulting drift on *both* sides, while the frontend keeps compiling and its tests keep passing.

Reproduce it end to end (restores the corpus on exit):

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

Baseline: zzop resolves **49 cross-layer edges** — 19 HTTP routes + 30 shared DB tables — and `PUT /api/user` is one matched edge (that `axios.put` ↔ that `router.put`).

## The break

An ordinary REST tidy-up on the backend — the kind that sails through review — renames the route:

```diff
- router.put('/user',    auth.required, …)
+ router.put('/users/me', auth.required, …)
```

The frontend is **not touched**.

## What zzop reports

```
=== edges (48) ===            ← was 49; the PUT /api/user edge is gone

=== unprovided consumes (2) ===
  "PUT /api/user"        @ fe-vite     src/pages/Settings.jsx:19          ← the FE call now hits nothing

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
