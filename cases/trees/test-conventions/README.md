# `test-conventions` tree — does the rule layer know what a test file is called?

Three PAIRS. Each pair is the same defects twice: once on a production path, once on the path that
language's toolchain uses for tests.

| production twin (expectations) | test twin (`benign`) | convention under test |
|---|---|---|
| `services/handler.go` | `services/handler_test.go` | Go `_test.go` suffix |
| `services/login.py` | `services/test_login.py` | Python `test_*.py` prefix |
| `Api/UserService.cs` | `Api.Tests/UserTests.cs` | C# `*Tests.cs` suffix **and** `*.Tests/` project dir |

## Why pairs, and not just the silent halves

A negative control whose silence has two possible causes measures nothing. If only the test twins were
committed, this tree would score green both when the path gate works and when the content simply stopped
triggering any rule — a rule rename, a narrowed `file_pattern`, a matcher regression would all read as
success. The production twin is what removes that reading: it holds the same lines, so the two halves
can only diverge because of the PATH.

That is the same discipline `decoy/`'s README states from the other direction. A decoy must be IN SCOPE
for the rule it baits, or its silence is free. These files are the case a decoy cannot express — the
whole point is that the path gate declines them BEFORE the matcher runs — so the scope evidence has to
come from a twin instead of from the file itself.

## Why this tree exists at all

`cases/trees/` held zero files in any of these three conventions, so the detection gate could not have
noticed that the DSL's shared `${test-paths}` vocabulary knew only TypeScript's spellings. Measured
2026-08-10, before the fix, on a tree of exactly this shape: **14 findings, all false positives**, versus
1 for the identical bytes moved under `tests/`. A benchmark with no fixture for a convention cannot
report on it, and that blindness is what let three shipped documents claim the fragment covered five
languages while it covered one.

## Constraints this tree keeps

* **No HTTP provides and no SQL.** A route would draw `cross-layer/unconsumed-endpoint`, and a table name
  would enter the `db-table` channel and perturb the cross-tree rules other trees are scored on — the
  effect `trees/rust-svc` measured when a plain `users` there moved `api-be`'s expected set. This tree is
  join-inert on purpose: it measures the per-file rule axis and nothing else.
* **No high-entropy or secret-shaped literals.** Several security rules deliberately carry NO test-path
  exclusion (`scan_test_regions: true` — a credential at rest is committed either way), so a secret in a
  test twin would be a legitimate finding and would turn a control into a false positive against itself.
* **`services/` is load-bearing** for the Go and Python pairs: `code-hygiene/console-in-be` is gated on a
  backend path segment, so those pairs would lose that rule's coverage anywhere else. The C# pair uses
  rules gated on the extension alone, which is what lets it sit at the idiomatic `Api.Tests/` location.
