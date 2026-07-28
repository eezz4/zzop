# `decoy` tree — the corpus's whole-file precision axis

Every `.ts` file in this tree is a **negative control**, listed in `EXPECTED.jsonc`'s `benign` array, where
**any** finding scores as a false positive. Nothing here is meant to fire, ever.

Before this tree existed the corpus measured precision almost entirely through the per-module `good`
exports, i.e. "does the rule stay quiet on the one line right next to the planted defect". That misses the
question a real repo asks: does the rule stay quiet across a file of ordinary code that *resembles* what it
looks for?

## The rule that makes a decoy worth anything

**A decoy must be IN SCOPE for the rule it baits.** Rules are gated by `file_pattern`, `file_exclude_pattern`
and `require_file`; a decoy that fails any of those was never evaluated, and counting its silence inflates
precision for free. So each file states, in its header, *why the rule looked at it* — which gate it
satisfies and how — and then which arm of the matcher keeps it silent. Concretely:

* `api/sql.nplus1.decoy.ts` sits under `api/` because `sql/nplus1`'s `file_pattern` is anchored `^api/`.
* `web/security.secret-env-in-fe.decoy.ts` sits under `web/` for the same reason.
* `api/reliability.console-in-be.decoy.ts` needs one of the backend path segments.
* `security.eval-dynamic-code`, `jwt-none-algorithm`, `shell-exec-interpolation`, `parseint-no-radix`,
  `select-star`, `delete-no-where`, `no-document-write`, `no-system-dialogs`, `interval-no-clear` all have a
  `require_file`, and each decoy satisfies it deliberately so the rule really runs.

The strongest decoys are the ones whose `line_pattern` DOES match and whose `exclude_pattern` /
`require_file_absent` is what produces the silence (`interval-no-clear`, `body-limit-missing`,
`no-system-dialogs`, `timing-unsafe-compare`, `sql.no-where`, `db.unawaited-write`): those measure the
rule's veto arm, which is the part most likely to rot unnoticed.

## Constraints this tree keeps

* **Synthetic values only** — no real secrets, no real company names. Hosts are `*.example.net`.
* **No http provides.** A route here would immediately draw `cross-layer/unconsumed-endpoint` and turn a
  control into a false positive. `http/protected-path-no-auth-evidence`, `dev-path-no-guard-hint` and
  `get-route-no-cache-marker` therefore keep
  their existing in-module `good` controls instead of getting decoys here.
* **Exactly one outbound consume** (`services/reliability.fetch-no-timeout.decoy.ts`) on a host used
  nowhere else, so the tree perturbs no cross-layer aggregate.
* **Everything is imported by `index.ts`.** An unreferenced control file makes `dead-candidates` /
  `dead-exports` fire *legitimately*, and the scaffolding would score as a false positive. The two
  `dead.*` files are the deliberate exception-shaped cases: reachable only via a re-export hop and only via
  a dynamic import.
* `reliability/env-outside-config` is structurally silent in this tree — it has a
  `require_attr_declared: env-config-module` gate and no overlay declares that key here — so `process.env`
  reads in these files prove nothing about that rule. Its exemption arm is covered by the `config/env.ts`
  controls in the `api` and `web` trees instead.
