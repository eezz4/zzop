# Linter overlap — what the tools you already run can and cannot see

This is the per-rule inventory behind zzop's bundling bar. For each **bundled** DSL rule it names the
nearest equivalent in a standard, widely-run linter, or states that there is none.

## The bar, and why the column is spelled as a tool name

The decision this table serves lives in the internal decision record, not here; the one line that
matters is: **can you name the tool that already sees this rule? If you cannot, the rule stays.**

Naming a tool is *not* by itself a reason to unbundle. Five structural proxies for "a linter already
does this" were tried and all five were wrong, because the real question — *what is this user already
running?* — is answered differently per language. TypeScript has de-facto defaults
(`typescript-eslint`, `biome`, `oxlint`); Java, Go, C#, Rust and Python do not. So a rule leaves only
when **both** hold:

1. a standard tool for that language sees the same defect, **and**
2. the rule reads **one language** — no cross-file fact, no git history, no framework or project
   vocabulary, and no multi-language reach that one zzop config gives and a single-language linter
   cannot.

`security/hardcoded-secret` is the worked counter-example: a plain line scan with an obvious equivalent
(`gitleaks`), kept because one config points it at JS/TS, Java and Rust together. Condition 1 is met and
condition 2 is not.

The `typescript-lint` pack is what leaving looks like — see `examples/packs/README.md`, which owns
that pack's rule count and the measurement that justified moving it.

**Recount the rule set** (this document lists rules; it does not own their count — which is why no
section heading below carries one. A heading count restates the table three lines under it, and the
restatement is what rots: one of them was wrong in the commit that created this file and stayed wrong,
because the pin guarding this document asserts membership and deliberately not arithmetic):

```
node -e 'const fs=require("fs"),p=require("path");let n=0;
for(const d of fs.readdirSync("rules/dsl")){const f=p.join("rules/dsl",d,d+".json");
if(fs.existsSync(f))n+=(JSON.parse(fs.readFileSync(f,"utf8")).rules||[]).length}
console.log(n)'
```

## How to read the verdict column

- **stays — no tool** — condition 1 fails. Nothing standard sees it.
- **stays — multi-language** — condition 1 holds, condition 2 fails on reach. One config covers a
  language set no single-language linter does.
- **stays — needs a fact linters lack** — condition 1 holds only partially; the equivalent tool sees a
  *broader or narrower* defect, and zzop's version depends on framework/ORM vocabulary, a whole-tree
  fact, or a cross-layer join.
- **candidate** — both conditions hold. These are the rules a future export wave should consider, one
  at a time. Listing a rule here is not a decision to move it.
- **measured — stays** — both conditions held on paper, the rule was carried through the two
  measurements the Summary describes, and it did not move. This verdict is strictly stronger than
  "candidate": it means someone ran the corpus and read the configs, and the answer was no. See
  § *The seven candidates were measured* below for the numbers and their recount commands.

---

## `browser` — all single-language TS/JS

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `browser/unsafe-html-sink` | `eslint-plugin-no-unsanitized` (`no-unsanitized/property`) | **measured — stays** |
| `browser/jquery-html-sink` | `eslint-plugin-no-unsanitized` (`no-unsanitized/method`), but see below | **measured — stays** |
| `browser/no-document-write` | `eslint-plugin-no-unsanitized` (`no-unsanitized/method`), for a different defect | **measured — stays** |
| `browser/javascript-url` | core ESLint `no-script-url` | **measured — stays** |
| `browser/vue-v-html` | `eslint-plugin-vue` (`vue/no-v-html`) | **measured — stays** |
| `browser/postmessage-wildcard` | no ESLint rule; a semgrep registry rule exists | stays — no tool |
| `browser/location-assign-dynamic` | none | stays — no tool |
| `browser/markdown-and-html-sink-unsanitized` | `eslint-plugin-no-unsanitized`, partially | stays — needs a fact linters lack |

This was the strongest cluster in the bundle on paper: sink-based XSS in one language, with a
maintained plugin that most projects touching the DOM have a reason to run. The measurement found that
**no repository in the reference corpus has that plugin** — not in a config, not in a lockfile — and
that the five rules together produce **one finding across 30 checkouts**. The `markdown-and-html-sink`
row is deliberately not among them either way: it pairs a markdown renderer with a sink across a method
span, which `no-unsanitized` does not model.

Two of the equivalence claims above are weaker than they read, and both were found by re-deriving the
column rather than by re-reading it:

* **`jquery-html-sink`** — `no-unsanitized/method` checks the native insertion methods
  (`insertAdjacentHTML`, `document.write`/`writeln`, `Range.createContextualFragment`). jQuery's
  `.html()` / `.append()` are not in its default method set, so the plugin most likely does not see
  this rule's subject at all. Nothing standard does: `eslint-plugin-no-jquery` exists but is a
  deprecation-migration plugin, not a sanitization one.
* **`no-document-write`** — the tool is named correctly but it is aimed elsewhere.
  `no-unsanitized/method` fires on an *unsanitized dynamic* argument; this rule fires on **any**
  `document.write(`, because the defect it reports is parser-blocking and CSP/PWA breakage, not XSS.
  Overlapping subject, different claim.

## `db`

Almost the entire pack encodes ORM vocabulary (Prisma, TypeORM, Sequelize, Knex) plus transaction and
loop reasoning. No general-purpose linter models a transaction boundary.

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `db/unawaited-write` | `@typescript-eslint/no-floating-promises` (with types) | stays — needs a fact linters lack |
| `db/unawaited-transaction` | `@typescript-eslint/no-floating-promises` (with types) | stays — needs a fact linters lack |
| `db/update-delete-no-where` · `db/pagination-no-orderby` · `db/client-new-in-handler` · `db/external-call-and-tx` · `db/unbounded-user-limit` · `db/find-then-create-no-unique` · `db/float-money-compare` · `db/empty-catch-and-write` · `db/multi-write-no-tx` · `db/non-atomic-counter-update` · `db/connection-no-release` · `db/client-new-in-loop` · `db/write-in-loop-no-tx` · `db/check-then-act-in-loop` · `db/idempotency-key-regenerated-in-loop` · `db/manual-tx-no-rollback` · `db/tx-and-empty-catch` · `db/money-tx-no-isolation-level` · `db/tx-and-db-call-in-loop` | none | stays — no tool |

The two floating-promise rows are the interesting judgment. `no-floating-promises` sees a *superset*
and sees it better, because it has type information zzop does not. But it is not a substitute here:
zzop's versions fire only on an ORM write, which is what makes them quiet enough to be worth reading,
and they need no `tsconfig`-aware type-checked lint setup. Kept, and worth revisiting if a project's
own type-checked ESLint config is ever a signal zzop can consume.

## `egress`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `egress/http-url-literal` | none | stays — no tool |
| `egress/get-and-body` | none | stays — no tool |
| `egress/ws-no-auth` | none | stays — no tool |

## `go`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `go/goroutine-in-loop` | `go vet` (`loopclosure`) covers a *different* loop defect — variable capture, not unbounded spawn | stays — no tool |

## `http`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `http/protected-path-no-auth-evidence` | none | stays — needs a fact linters lack |
| `http/dev-path-no-guard-hint` | none | stays — needs a fact linters lack |

Both read the projected io channel — route declarations across a tree — which is the cross-layer fact
no file-scoped linter has.

## `perf` · `react`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `perf/api-in-loop` | none | stays — no tool |
| `react/setstate-after-async-unguarded` | none — `eslint-plugin-react-hooks` does not model this | stays — no tool |

## `redis`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `redis/flushall-in-code` · `redis/keys-command-in-code` · `redis/client-no-error-listener` · `redis/lock-get-then-set` · `redis/lock-no-ttl` · `redis/counter-get-set` | none | stays — no tool |

Client-library vocabulary end to end. `lock-get-then-set` and `counter-get-set` additionally judge a
sequence across a method span.

## `reliability`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `reliability/sync-fs-in-handler` | `eslint-plugin-n` (`n/no-sync`) | stays — needs a fact linters lack |
| `reliability/fs-in-loop-serial` | core ESLint `no-await-in-loop`, partially | stays — needs a fact linters lack |
| `reliability/await-inside-promise-all-array` | core ESLint `no-await-in-loop` sees a different shape | stays — no tool |
| `reliability/map-async-no-promise-all` | `@typescript-eslint/no-misused-promises`, partially | stays — needs a fact linters lack |
| `reliability/async-route-no-catch` · `reliability/debug-true-committed` · `reliability/promise-all-and-writes` · `reliability/json-parse-no-try` · `reliability/fetch-no-timeout` · `reliability/reqwest-no-timeout` · `reliability/body-limit-missing` · `reliability/interval-no-clear` · `reliability/stream-open-no-close-in-loop` · `reliability/listener-subscribe-in-loop` · `reliability/emitter-async-listener` · `reliability/fs-check-then-use` | none | stays — no tool |

`n/no-sync` is the closest call in this pack. It flags *every* sync call; zzop's rule flags one inside
a request handler, which is the difference between a style preference and a latency bug. The handler
evidence is what `n/no-sync` cannot express.

## `security`

The pack where condition 1 is met most often and condition 2 almost never. Two facts from the corpus
measurement govern this section: **15 of 17 reference repositories run no security linter at all**, and
the Java/Rust/Go/C# rules here have no de-facto default tool the way TypeScript does.

### Secret scanning — equivalent named, kept on reach

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `security/hardcoded-secret` | `gitleaks`, `trufflehog` | stays — multi-language |
| `security/high-entropy-secret` | `gitleaks`, `trufflehog` | stays — multi-language |
| `security/private-key-committed` | `gitleaks`, `trufflehog` | stays — multi-language |
| `security/vendor-token-committed` | `gitleaks`, `trufflehog` | stays — multi-language |
| `security/conn-string-credentials` | `gitleaks` | stays — multi-language |
| `security/api-key-in-url` | `gitleaks`, partially | stays — multi-language |
| `security/config-file-secret` | `gitleaks` | stays — needs a fact linters lack |
| `security/secret-env-in-fe` | none — needs framework vocabulary (`NEXT_PUBLIC_`, `VITE_`) | stays — no tool |
| `security/hardcoded-password` | `find-sec-bugs`, SonarQube | stays — no tool |

Every row here names a tool, and none leaves. `gitleaks` is a *scanner*, not a linter, and the
distinction matters for condition 2: it is a separate pipeline step most repositories do not run,
whereas these rules ride a scan the user is already performing.

### Java lane — equivalent named, no de-facto default

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `security/xxe-no-guard` | `find-sec-bugs` (`XXE_*`) | stays — no tool |
| `security/unsafe-deserialization` | `find-sec-bugs` (`OBJECT_DESERIALIZATION`) | stays — no tool |
| `security/cmd-injection` | `find-sec-bugs` (`COMMAND_INJECTION`) | stays — no tool |
| `security/java-path-traversal` | `find-sec-bugs` (`PATH_TRAVERSAL_*`) | stays — no tool |
| `security/trust-all-tls` | `find-sec-bugs` (`WEAK_TRUST_MANAGER`) | stays — no tool |
| `security/weak-random` | SpotBugs / `find-sec-bugs` (`PREDICTABLE_RANDOM`) | stays — no tool |
| `security/weak-cipher` | `find-sec-bugs` (`CIPHER_INTEGRITY`, `ECB_MODE`) | stays — no tool |
| `security/stacktrace-to-response` | `find-sec-bugs` (`INFORMATION_EXPOSURE`) | stays — no tool |
| `security/annotation-sql-concat` | `find-sec-bugs` (`SQL_INJECTION_*`) | stays — no tool |
| `security/sql-string-concat` | `find-sec-bugs`, SpotBugs | stays — no tool |

"stays — no tool" is doing precise work in this block: the tool *exists and is named*, but condition 1
asks what the user is **running**, and `find-sec-bugs` is opt-in SpotBugs plugin configuration that 15
of 17 corpus repositories do not have. This is the block that killed the "single-language stdlib"
proxy: cutting on rule shape would have exported `xxe-no-guard` (critical, nobody's tool runs) while
keeping a rule core ESLint sees for free.

### TypeScript lane — where condition 2 can actually be met

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `security/eval-dynamic-code` | core ESLint `no-eval` / `no-new-func`; `biome` (`security/noGlobalEval`) | **measured — stays** |
| `security/shell-exec-interpolation` | `eslint-plugin-security` (`detect-child-process`) | **measured — stays** |
| `security/path-traversal` | `eslint-plugin-security` (`detect-non-literal-fs-filename`) | stays — needs a fact linters lack |
| `security/weak-token-random` | `eslint-plugin-security` (`detect-pseudoRandomBytes`), partially | stays — needs a fact linters lack |
| `security/taint-flow` | semgrep, CodeQL | stays — no tool |
| `security/jwt-none-algorithm` · `security/jwt-verify-bypass` · `security/jwt-sign-literal-secret` | semgrep registry rules | stays — no tool |
| `security/raw-query-unsafe-api` · `security/dangerous-html-concat` · `security/html-response-from-request` · `security/ssrf-user-url` · `security/open-redirect` · `security/sendfile-from-request` · `security/mass-assignment` · `security/error-leak-to-client` · `security/localstorage-jwt` · `security/timing-unsafe-compare` · `security/bcrypt-cost-too-low` · `security/jwt-no-expiry` | none | stays — no tool |
| `security/cors-wildcard` · `security/cors-credentials-wildcard` · `security/cors-reflected-origin-credentials` · `security/csp-weak-or-disabled` · `security/insecure-cookie` | none | stays — no tool |
| `security/template-unescaped-output` | none (`.ejs`/`.hbs` templates) | stays — no tool |

`eval-dynamic-code` and `shell-exec-interpolation` are the only two security rules meeting both
conditions. An earlier revision of this table paired `eval-dynamic-code` with `no-implied-eval`, which
is wrong: `no-implied-eval` is about string arguments to `setTimeout`/`setInterval`, a shape this rule
does not read. The rule's two branches are `eval(<non-literal>)` and `new Function(...)`, whose core
equivalents are `no-eval` and `no-new-func`. Also note that neither is in `eslint:recommended` — they
arrive only through an opinionated preset, which is what the measurement below had to check.

`path-traversal` is the near miss worth explaining: `detect-non-literal-fs-filename` fires on any
non-literal path argument and is famously noisy, while zzop's requires request-derived evidence in the
same span. Different precision, same neighbourhood — kept.

### Cross-language crypto — kept on reach

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `security/weak-crypto` | per-language: `eslint-plugin-security`, `bandit`, `gosec`, `find-sec-bugs` | stays — multi-language |
| `security/weak-password-hash` | per-language, as above | stays — multi-language |
| `security/sql-format-interpolation` | none for Rust — clippy has no such lint | stays — no tool |
| `security/command-and-interpolation` | none for Rust | stays — no tool |

The first two are the call-kind rules: one rule, one config, six to eight languages. Replacing them
means running four different tools. This is condition 2 at its strongest.

## `sql`

| Rule | Nearest standard-linter equivalent | Verdict |
|---|---|---|
| `sql/delete-no-where` · `sql/update-no-where` · `sql/truncate-in-app-code` | none — SQL linters (`sqlfluff`) read `.sql` files, not SQL embedded in nine host languages | stays — multi-language |
| `sql/destructive-migration` | none | stays — multi-language |
| `sql/nplus1` · `sql/count-in-loop` · `sql/race-condition-toctou` · `sql/raw-sql-check-then-write` | none | stays — no tool |

## The seven candidates were measured. None of them moved.

Seven rules met both conditions on paper — one language each, with a nameable tool:

- `browser/unsafe-html-sink`, `browser/jquery-html-sink`, `browser/no-document-write`,
  `browser/javascript-url`, `browser/vue-v-html`
- `security/eval-dynamic-code`, `security/shell-exec-interpolation`

Listing a rule was never a decision to move it. Two things had to be measured per rule first, both
learned from the `typescript-lint` export:

1. **What share of real reports does it produce?** That export was justified by measurement —
   `no-explicit-any` was 56% of one repository's findings and drowned two unauthenticated mutating
   routes. **A rule that fires rarely costs nothing by staying.**
2. **Does the named tool actually run in the repositories we measure?** The Java block above is the
   standing warning against assuming a nameable tool is a running one.

Both were measured on a **30-checkout** reference corpus — 17 joined RealWorld-family repositories plus
13 unrelated public ones (29 distinct repositories; one is checked out in both sets) — cold cache, one
`analyze` per tree under the corpus vocabulary. 321 findings in total.

### Measurement 1 — share of report

| Rule | Findings / 321 | Trees it fires in | Files it is allowed to read | Files whose `require_file` matches |
|---|---|---|---|---|
| `browser/unsafe-html-sink` | 0 | 0 of 30 | 399 | 1 |
| `browser/jquery-html-sink` | 0 | 0 of 30 | 399 | 0 |
| `browser/no-document-write` | 0 | 0 of 30 | 399 | 0 |
| `browser/javascript-url` | 0 | 0 of 30 | 399 | 0 |
| `browser/vue-v-html` | 1 | 1 of 30 | 421 | 1 |
| `security/eval-dynamic-code` | 0 | 0 of 30 | 414 | 0 |
| `security/shell-exec-interpolation` | 0 | 0 of 30 | 388 | 3 |

The last two columns are what makes the zero readable rather than merely small: the rules are pointed
at hundreds of files they are entitled to judge, and the construct they look for is simply not written
there. `shell-exec-interpolation` is the sharpest case — three files really do use `child_process` and
the rule really did read them and stay silent. That is measured precision, not blindness.

The one finding is `browser/vue-v-html`: **12.5% of the one tree it fires in, 0.31% of the corpus.**
This is the opposite of the `no-explicit-any` result that justified the `typescript-lint` export; it
drowns nothing, and by criterion 1's own wording it costs nothing by staying.

Recount (from the repository root, with a corpus checked out per the internal corpus recipe — the
`--limit`/`--severity` pair makes the reply list every finding, and the per-rule counts ride in
`findings.byRule` whether or not they are listed):

```
zzop analyze --config <one-tree-config> --limit 1000 --severity info
```

The admitted-file columns are the rules' own `matcher.file_pattern` minus `matcher.file_exclude_pattern`
run over the real paths, and `require_file` run over the real bytes. Read those three expressions out of
`rules/dsl/browser/browser.json` and `rules/dsl/security/security.json`; they are the source of truth,
not this table.

**Invalidation drill.** A byte-identical zero is usually a broken instrument, so the same binary and the
same vocabulary were pointed at a three-file tree with one planted violation per rule. All seven fired:
`unsafe-html-sink` 2, `jquery-html-sink` 2, `no-document-write` 1, `javascript-url` 2, `vue-v-html` 1,
`eval-dynamic-code` 2, `shell-exec-interpolation` 2.

### Measurement 2 — does anyone run the tool?

Read off `.eslintrc*`, `eslint.config.*`, `package.json` (`devDependencies` + `eslintConfig`),
`biome.json`, and — for the transitive question a preset hides — the **lockfiles**, which name the
resolved dependency set whether or not the tree names it.

| Named tool | Trees that have it |
|---|---|
| `eslint-plugin-no-unsanitized` | **0 of 30** — absent from every config *and* every lockfile |
| `eslint-plugin-security` | **0 of 30** — same |
| `eslint-plugin-vue` | **1 of 30** (one Vue tree, via a bundled preset; its lockfile resolves the plugin) |
| ESLint at all | 4 of the 12 trees that contain any TS/JS file |
| Biome | 1 (one front end, `linter.enabled: true`, `recommended: true`, no security-group override) |

**Seven of the twelve trees holding TypeScript or JavaScript run no linter whatsoever** — two of them
carry only `prettier`, which is a formatter, and five carry nothing. This is the browser cluster's
version of the Java block above: the tool is nameable and it is not running.

**The exception, and it is worth reading.** The corpus's single `vue-v-html` finding sits between an
`<!-- eslint-disable vue/no-v-html -->` and an `<!-- eslint-enable vue/no-v-html -->` in the template.
That project runs the named tool, has the named rule switched on, has already seen that exact line, and
has already judged it. zzop reports it a second time — and zzop's own `// zzop-vue-v-html-ok` marker is
structurally unwritable there, because the finding lands inside markup (the rule's message says so). So
the honest reading of the one datum is not "the rule is wrong" but **"the rule cannot read a suppression
the user already wrote"**, which is a repair, not an exile — and one tree is not an evidence base for
moving anything.

### Verdicts

All seven stay.

* `unsafe-html-sink` · `jquery-html-sink` · `no-document-write` · `shell-exec-interpolation` — the
  named plugin runs in **zero** repositories. Condition 1 fails on the question it actually asks.
* `javascript-url` · `eval-dynamic-code` — the named rules are core ESLint, but neither is in
  `eslint:recommended`; they arrive only via an opinionated preset, and the constructs they judge
  appear in **0 of ~400** admitted files. Nothing to weigh against keeping them.
* `vue-v-html` — the only rule where the tool demonstrably runs, and the only one that ever fired. Held
  on n=1 and on the fold below.

Two of the four browser rules above carry a second reason. A proposed IR **`html-sink` call kind**
would fold the HTML-insertion rules into one cross-language judgment; `unsafe-html-sink`,
`jquery-html-sink` and `vue-v-html` are all inside that family, and a rule cannot be both exiled and
folded. Exporting them would also delete the inventory that says which kind is worth building.
`shell-exec-interpolation` is the same argument already finished: it declares
`"line_call_kind": "process-exec"`, so it is a *consumer* of a kind this engine already has.

One finding came out of the corpus in the rules' favour rather than against them. A real
`dangerouslySetInnerHTML={markup}` sink sits in a React tree that runs no linter at all, and
`unsafe-html-sink` misses it, because its pattern requires the inline `{{ __html: ... }}` object and
this one is a variable. The rule that would have exiled cleanly is the rule that needs widening.

The remaining rows are the answer to "what does zzop add over the linters I already have".
