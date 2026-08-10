# `sql-langs` tree — do the quote-anchored SQL rules hold outside TypeScript and Rust?

Five `sql` rules read only the INSIDE of a quoted string literal: a quote character, a SQL keyword, a
table name, a closing quote. Not one character of host-language syntax appears in any of their patterns.
Until 2026-08-10 all five were gated to `.ts/.tsx/.js/.mjs/.cjs/.rs`, so a Python or Go service shipping
`conn.execute("DELETE FROM users")` got silence while the identical TypeScript line got a CRITICAL. This
tree is the measurement that the widening to `.py`, `.java`, `.cs` and `.go` is real.

## Four PAIRS plus one migration twin

| production twin (expectations) | second twin | what the second twin proves |
|---|---|---|
| `services/queries.py` | `services/test_queries.py` (`benign`) | Python `test_*.py` — the path gate holds |
| `services/queries.go` | `services/queries_test.go` (`benign`) | Go `_test.go` — the path gate holds |
| `Api/Queries.cs` | `Api.Tests/QueriesTests.cs` (`benign`) | C# `*Tests.cs` **and** `*.Tests/` — the path gate holds |
| `src/main/java/.../Queries.java` | `src/test/java/.../QueriesTest.java` (`benign`) | Java `src/test/` + `*Test.java` — the path gate holds |
| `services/queries.py` | `alembic/versions/0001_backfill_ids.py` (**expectations**) | the migration handoff is real, not just an exclusion |

The first four follow `trees/test-conventions/README.md`: a negative control whose silence has two
possible causes measures nothing, so each test twin carries the SAME literals as its production twin and
the two can only diverge because of the PATH. The fifth is the same discipline with a stronger claim — the
three destructive rules must be SILENT under `alembic/versions/` **and** `sql/destructive-migration` must
FIRE there at info. A migration fixture with only the silent half would score green if the disclosure rule
had quietly stopped admitting `.py`, which is precisely what the three critical rules' messages promise it
does not.

## Why the literals are byte-identical across the four production twins

That is the claim under test. The same `DELETE FROM …` string in five languages either produces five
findings or the gate is not a gate. The files differ only in the language, plus a short per-language block
of QUOTE-FORM evidence — the spellings the line-scan can and cannot reach:

* **fires**: C# verbatim `@"…"`, Go raw `` `…` ``, a single-line Python `"""…"""`. In each of these the
  quote is still the character adjacent to the keyword.
* **silent, and disclosed in the rules' own messages**: every MULTI-LINE form — a Java text block, a Go
  raw-string block, a multi-line C# `@"…"`, a Python triple-quoted block. The statement then sits on a
  line carrying no quote, and a line-scan reads one line.
* **silent, by an exclusion added with the widening**: `LIKE '%s'` / `LIKE '%(term)s'`, where the `%` is a
  printf placeholder rather than a wildcard. `queries.go` also plants the OTHER side —
  `LIKE '%%%s%%'`, whose leading `%` is genuine — so the exclusion cannot decay into a blanket veto on `%`
  without the gate noticing.
* **silent for a different reason, recorded so the difference is not assumed**: C# composite formatting
  spells its placeholder `{0}`, which never reaches the `%` anchor at all.

## Constraints this tree keeps

* **Tree-unique table names** (`sqlx_<lang>_…`), even though this tree MEASURES join-inert
  (`ioConsumesKeyed: 0`). That zero is a fact about the four new languages, not about the fixtures — the
  SQL-literal-to-`db-table` extraction that makes `trees/rust-svc` join-ACTIVE does not run for
  `.py`/`.go`/`.cs`/`.java`. The prefix is insurance against the day it does, and the cost of the
  alternative is on record: `trees/rust-svc/src/queries.rs` measured a plain `users` making a cross-tree
  rule fire in that tree AND in `api-be`, i.e. adding a tree silently changed another tree's expected set.
* **No HTTP provides, no secrets, no weak-crypto, no console calls.** This tree measures the per-file SQL
  rule axis. `trees/test-conventions` states the same constraints for the same reasons — and states "no
  SQL", which is why these fixtures live here instead of being appended there.
* **No `"…" + ident` concatenation.** That shape belongs to `security/sql-string-concat`, whose Java
  fixture is `trees/java-svc/.../UnsafeController.java:19` — the line that also carries the one intended
  co-fire (`sql/select-star`, the over-fetch, alongside the injection). Duplicating it here would be a
  second owner of that ruling.
