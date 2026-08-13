//! The starter `zzop.config.jsonc` document — the ONE canon behind three surfaces: the
//! `config-template` embedded contract resource, which `zzop contract config-template` prints and MCP
//! `resources/read` serves (both wired in `zzop_summary::contracts`), and `zzop init`, which is
//! nothing but argv parsing plus a file write of these exact bytes. It lives HERE, in the config front
//! end, for the same reason `config-surface.json` does: the crate that decides what a key MEANS owns
//! the text that teaches it, and the contract table references it rather than embedding a second copy.
//!
//! Two properties are machine-pinned in `template_tests.rs` instead of left to review, because this
//! repo has already paid for both defect classes:
//! 1. **Every key it names is real.** A starter file advertising a key no surface consumes is a new
//!    instance of exactly the defect `warnings::RETIRED_KEYS` had to clean up. The pins run the ACTIVE
//!    keys through this crate's own unknown-key walk, and check the keys named in the COMMENTS against
//!    the same `config-surface.json` vocabulary.
//! 2. **Its values are zzop's own suggestions, not a second set.** Every value below is read from the
//!    symbol its consumer already owns (`VocabularyConfig::built_in`), so the starter file DOCUMENTS the
//!    suggestions instead of quietly re-deciding them.
//! 3. **Its "TypeScript/JavaScript files only" sentences stay true.** Added 2026-08-13 for the third
//!    defect class this file has now paid for: a knob whose consumer is one-language-gated, written into
//!    a Java/Go/Python/C# user's own repository with no sign of that. The pin reads the consuming rules'
//!    OWN sightline declarations rather than a hand list, and it covers four of the eleven such keys —
//!    the rest have no declaration to bind to, and the pin's doc names them instead of implying cover.
//!
//! This property used to be spelled "writing it changes nothing — every value below is the one a run
//! already uses with no config file at all", and the prose in the template said the same to the user.
//! Both went false on 2026-07-27 and neither noticed: config became mandatory (there is no run without
//! a file), and an undeclared vocabulary key became a judgment NOT MADE (so "delete what you do not
//! need" told the reader to switch analyses off while sounding like tidying). D15's sweep of "zzop runs
//! without config" claims covered `docs/`, `site/`, the READMEs and the MCP tool descriptions — and
//! missed THIS file, the one the product writes into the user's own repository, i.e. the most-read
//! config document zzop has. The pins below could not catch it either: they check that every key named
//! is real, never that the surrounding sentences are true. A guard over vocabulary is not a guard over
//! claims.
//!
//! Comment style, and the reason for it: the prose says what a key MEANS and stops there — no counts,
//! no inventories, no "currently". A comment stating today's state rots, and this one rots inside a
//! file the user owns and zzop will never rewrite. Keys are named in backticks so a machine can check
//! them; everything else stays plain prose.

/// The starter `zzop.config.jsonc` bytes, verbatim.
pub const CONFIG_TEMPLATE_JSONC: &str = r#"// zzop configuration. JSONC: comments and trailing commas are allowed.
//
// Every analysis needs this file — zzop refuses to run without one rather than analyze your code under
// assumptions you never saw. What is written here IS the analysis: the `vocabulary` block below names
// what your project calls its own guards, routes and generated files, and a name you delete is a
// judgment zzop stops making. So edit values freely, but delete a key only when you mean "stop asking
// this question". The machine-checked vocabulary of every key zzop accepts is the config-surface
// contract document, which every zzop surface can print.
{
  // The trees to analyze, resolved against this file's own directory. Swap in `trees` when a tree
  // needs its own id, adapter overlays or deployment topology, and "trees": "auto" to derive one tree
  // per workspace package of a monorepo.
  "roots": ["."],

  // Rule packs — the DOMAIN-level on/off switch, and the one to reach for first: a pack is a whole
  // subject area, so turning one off is one line where the per-rule map below would be dozens.
  // The bundled pack ids are:
  //     browser  db  egress  go  http  perf  react  redis  reliability  security  sql
  // `packs.disabled` drops a pack whole, before any rule inside it is evaluated ("everything except").
  // `packs.only` is its opt-IN twin: name the packs you want and every OTHER bundled pack stays off,
  // so a repo that only cares about one subject writes one line instead of eleven. The two compose —
  // `only` selects, `disabled` still subtracts. Both are about PACKS: the native analyses
  // (dead-candidates, unimported-export, scores, and friends) are not packs and keep running; switch
  // those off by id in `rules` below.
  // Packs you wrote (or recovered) load from `zzop/rules/` beside this file, with no key at all — that
  // is the default authored-pack location, used whenever it exists on disk. `packs.extraDirs` names
  // other directories instead, and REPLACES that default outright rather than adding to it — the two
  // are never merged, so a run's packs have one origin. An empty array is therefore the explicit
  // opt-out. Recovering a rule that left a release? Save the pack under `zzop/rules/` and it loads —
  // and its rules are then documented by the pack file you saved, not by the rule-catalog contract
  // document, which covers the bundled packs.
  "packs": {
    "disabled": []
  },

  // Per-rule overrides, keyed by rule id (the rule-catalog contract document lists every id). A value is either a
  // severity string, "off" to disable that rule, or an object taking `severity` and `exclude` (paths
  // this one rule stops reporting on).
  "rules": {},

  // Git history collection, which is what the architecture/ownership signals are computed from.
  // `git.recentDays` sets the recency window in days, `git.since` pins an absolute start instead, and
  // `git.commitTypePatterns` replaces the built-in commit-TYPE pattern table whole.
  // `git.commitSubjectPatterns` is a separate, purely OPT-IN axis: a [{ "pattern", "label" }] table
  // matched against the raw commit subject, with no built-in vocabulary at all — declare nothing and
  // nothing is labelled, because a revert/ticket/hotfix convention is yours to state, not ours to guess.
  "git": {},

  // Findings dropped by path, whatever rule reported them — the per-rule `exclude` above is the same
  // idea scoped to one id. Entries are path substrings, or globs when they carry wildcards. A path
  // plays one of two roles and gets the treatment that role deserves: where it is the finding's own
  // subject the finding is dropped, and where it is only named as evidence in a finding about somewhere
  // else, the finding stays and your path is replaced by <excluded>.
  "exclude": [],

  // Where the analysis cache is written. Setting it to null turns caching off; the directory is
  // derived state, safe to delete at any time, and belongs in .gitignore as **/.zzop/
  "cacheDir": ".zzop/cache",

  // Which parser reads which file, when the extension alone does not say. `parsers.globOverrides` is
  // checked BEFORE the extension map and the first match wins, so a house extension or a generated
  // suffix can be routed to the parser that actually understands it. Languages: typescript, python,
  // java, rust, go, csharp, sql, prisma. Globs match tree-relative paths.
  "parsers": {
    "globOverrides": []
  },

  // The names YOUR project picks, which zzop would otherwise have to guess. Framework-fixed names
  // (@GetMapping, router.post) are not here: nobody can rename those. These are yours — what you call
  // your auth guards, which URL segments mark your API, where your sources live, which directories hold
  // build output. Every value below is zzop's own suggestion for that key, written out so you can see
  // and judge it. It is not a default the engine would apply behind you: an undeclared key makes NO
  // judgment at all, so this block is what turns these questions on, and deleting a key turns one off.
  // Editing one key replaces that whole list or pattern; the keys you leave alone keep theirs.
  // Patterns are regular expressions matched case-insensitively via their own (?i).
  // NOT EVERY KEY HERE ASKS ITS QUESTION OF EVERY LANGUAGE. Several are read only where a recognizer
  // exists to read them — TypeScript/JavaScript, today — and in a tree of another language they are
  // inert no matter what you write in them. That is a different thing from deleting a key, which stops
  // a question that WAS being asked. Each comment below says which of the two it is, so a Java, Go,
  // Python or C# reader can tell a knob that is theirs from one that only looks like it.
  "vocabulary": {
    // Names that prove a call is an auth check, matched against every symbol the mutating-route-no-auth
    // call graph walks. Narrow it and routes guarded by an unlisted name start reporting.
    "authGuardPattern": "(?i)(auth|guard|verify|session|token|permission|acl|owner|admin|role|(?:has|can|check|require)access)",
    // The same question for the CLASS a guard method hangs off (AuthorizationService.check).
    "authGuardQualifierTokens": ["auth", "authz", "authn", "authorization", "authentication", "authenticator", "security", "permission", "permissions", "guard", "guards", "acl", "rbac", "jwt"],
    // Routes that ARE how a caller gets credentials cannot require credentials to reach. The standalone
    // list exempts on its own; the conditional list exempts only when a family segment is also present,
    // so /auth/register is exempt and /devices/register is not.
    "authAcquisitionStandalonePattern": "(?i)/(auth|login|logout|signin|signup)(/|$)",
    "authAcquisitionConditionalPattern": "(?i)/(register|token|refresh|password|otp)(/|$)",
    "authFamilyPathPattern": "(?i)/(auth|login|signin|signup|session|oauth)(/|$)",
    // Which URL segments mark an API surface — what keeps unprovided-consume off ordinary asset fetches.
    "apiSegmentPattern": "(?i)/(api|graphql|rpc|v[0-9]+)(/|$)",
    // Where Java sources live. A Spring security config only governs routes under its own copy of this
    // segment, so one module's posture never clears a sibling module's routes.
    "javaSourceRoot": "src/main/java/",
    // Where Python absolute imports resolve from, beyond the tree root and src/ (always tried). An
    // entry is an extra root directory ("backend") or a package-name mapping ("tml=" maps import name
    // tml to the tree root — the editable-install symlink idiom). A wrong entry cannot invent an edge;
    // it is one more candidate that matches no real file. Empty because only you know your layout.
    "pythonPackageRoots": [],
    // Directories never analyzed at all, and (separately) never walked when "trees": "auto" looks for
    // workspace packages. Build output and tool state are named by you, not by any tool.
    "skipDirs": ["node_modules", "dist", "build", ".next", ".git", "target", ".yarn", ".zzop", "zzop-reports", ".zzop-cache"],
    "workspaceSkipDirs": ["node_modules", ".git"],
    // Extra paths that hold TEST code, ADDED to the language conventions zzop already knows: the Go
    // "_test.go" suffix, Python "test_x.py"/"x_test.py", C# "XTests.cs" and "My.Tests/", Java
    // "FooTest.java"/"TestFoo.java", the JS/TS ".test."/".spec." infix, and the directories
    // "tests/" "spec/" "__tests__/" "e2e/" "fixtures/" "cypress/" ".storybook/" plus the test-runner
    // config files. THE ONE ADDITIVE KEY IN THIS BLOCK — every other one REPLACES its built-in list,
    // this one can only widen, so declaring "(^|/)it/" for your integration tests cannot cost you the
    // Go suffix. Empty is therefore NOT "no test paths": it is "the built-ins, and nothing else".
    // It has a built-in default at all — unlike every other key here — because a test convention is
    // fixed by the LANGUAGE, not chosen by you. With no default, zzop would judge a "_test.go" file as
    // production code and every finding on it would be a wrong CLAIM, rather than the quiet
    // under-report the rest of this block degrades to when you leave a key out. Each entry is a regex
    // over the tree-relative, forward-slash path; one that does not compile is dropped with a warning
    // naming it. To be judged on a path after all, turn the rule off under `rules` — narrowing this
    // list is deliberately not expressible.
    "extraTestPathPatterns": [],

    // How you reach your ORM client, and which methods on a data-access receiver are writes. These
    // decide which lines zzop calls a write site. READ FOR TypeScript/JavaScript FILES ONLY: the
    // write-site and query-call-site channels have one producer between them, so in a Java, Go, Python,
    // C# or Rust tree these three keys change nothing and no renaming makes them apply. That is not the
    // quiet under-report an undeclared key gives you — it is a blind channel, and the rules that read it
    // declare that blindness themselves, so a coverage report names it per tree.
    "prismaClientGetter": "getPrisma",
    "ormReceiverPattern": "Repository$|Store$|^prisma$|^db$|^orm$|^tx$|^trx$",
    "ormWriteMethods": ["create","createMany","update","updateMany","delete","deleteMany","upsert","insert","save","remove"],
    // The helpers you wrap a call in to retry it — a write inside one is reported differently.
    // TypeScript/JavaScript files only, same single-producer reason as the three keys above.
    "retryWrappers": ["pRetry","backOff","retryAsync","asyncRetry"],

    // Express/Hono route registration: which callees ARE a gate, which identifier tails mean
    // "sub-router, not a gate", which rejection verbs prefix a guard wrapper, and which words mean the
    // check is about the ENVIRONMENT rather than the caller. Widen the veto lists and fewer routes are
    // cleared; widen the guard lists and more are. TypeScript/JavaScript files only — the frameworks
    // named are the scope. Spring, ASP.NET, Django/FastAPI, gin and axum routes are recognized by their
    // own syntax and answer to none of the five keys below.
    "middlewareGuardCallees": ["passport.authenticate","expressjwt","requiresAuth","clerkMiddleware","ensureLoggedIn","checkJwt"],
    "routerNameVetoSuffixes": ["router","routes","route","controller","service","module","client","store","config","api"],
    "wrapperGuardPrefixes": ["require","ensure","protect","restrict"],
    "envAxisVetoSubstrings": ["env","prod","staging","local","dev","debug"],
    // The header your API accepts as an idempotency key. Read on the same TypeScript/JavaScript route
    // registrations as the four keys above, not on every route zzop finds.
    "idempotencyHeaderNames": ["idempotency-key","x-idempotency-key"],

    // The Python side of the same guard question: what makes a dependency callable a gate, and the
    // three shapes that mean it reads or renders rather than rejects.
    "pythonGuardSubstrings": ["authoriz","authentic","currentuser","activeuser","superuser","staffuser","permission","loginrequired","isauthenticated","isadminuser","requirelogin","requireauth","verifytoken","checktoken","jwtrequired","apikeyrequired"],
    "pythonGuardAnonymousVetoSubstrings": ["optional","ornone","maybe","anonymous"],
    "pythonGuardReportVetoPrefixes": ["list","count","serialize","render","format"],
    "pythonGuardReportVetoSuffixes": ["header","headers","serializer","serializers","handler","stats","metrics","report","summary"],

    // The Rust side of the same guard question. A Rust guard is a TYPE in the handler's signature
    // (`async fn create(user: AuthUser, ..)`), not a call its body makes, so the type name is matched
    // against authGuardPattern above. This list names the extractors that ADMIT an anonymous caller —
    // MaybeAuthUser holds an Option, so it proves nothing. Delete an entry and that spelling starts
    // clearing mutating routes.
    "rustOptionalExtractorPrefixes": ["maybe","optional"],

    // Self-audit for incremental caches (`cache-lane-file-read`). Name the function that produces one
    // CACHED per-file unit; zzop then walks the call graph from it and reports any filesystem read it can
    // reach, because such a read is an input your cache key almost certainly does not cover. There is no
    // default and there cannot be one — only you know which of your functions carries that promise — so
    // the rule stays silent until you fill this in. The sink list IS defaulted (these are stdlib names,
    // not names you pick); narrow it if one of them means something else in your tree.
    // The value below is deliberately empty rather than a placeholder pattern: there is no zzop-chosen
    // default here, so the template says so in the value itself instead of writing something that looks
    // like an answer.
    "cacheLaneAnchorPattern": null,
    "fileReadCallees": ["read","read_to_string","read_to_end","read_dir","read_link","open","metadata","canonicalize","readFile","readFileSync","readdir","readdirSync","existsSync","statSync"],

    // Which schema field names mean money (so a float there is a bug), and which are boilerplate the
    // unreferenced-field check skips.
    "moneyTokens": ["price","amount","cost","total","subtotal","balance","salary","wage","payment","payout","payable","receivable","refund","rebate","fee","fare","tariff","surcharge","deposit","revenue","income","expense","budget","profit","tax","discount","charge","credit","debit","commission","currency","money","cash","invoice","billing","premium","allowance","bonus"],
    "schemaUsageSkipFields": ["id","createdAt","updatedAt"],

    // The names your own fetch wrapper exports, and the banners your code generators stamp. Both decide
    // what zzop treats as machine-owned rather than hand-written, and both stop at the same border: a
    // Java, Go, Python or C# file is reached by neither. The fetch-wrapper names are read for
    // TypeScript/JavaScript files only. The banners are matched as TEXT, with no language syntax
    // involved, so they LOOK language-neutral — but matching text is not reach: both analyses that
    // consult a banner are extension-gated before any banner is opened (the unused-export check reads
    // TypeScript/JavaScript sources only; the dead-file check drops .py/.pyi, .rs, .go, .java and .cs
    // from candidacy outright, so the widest file it can ask about is a .vue/.svelte component).
    "fetchWrapperExportNames": ["get","post","put","del","delete_","patch","request","send","api","http","client"],
    "generatedFileMarkers": ["@generated","auto-generated","autogenerated","automatically generated","code generated by","this file is generated","this file was generated","openapi-generator"],

    // Cross-repo joins: which query parameters carry secrets, how you spell an API version segment, and
    // which routes are fetched from outside every analyzed tree (so "no consumer here" proves nothing).
    "secretParamNames": ["token","access_token","accesstoken","apikey","api_key","api-key","api_token","apitoken","key","secret","client_secret","password","auth","signature","jwt"],

    // Declared response-DTO field names that count as sensitive (cross-layer/sensitive-response-field),
    // in three matching axes: substrings of the normalized name, whole-name exacts, and suffixes.
    // NORMALIZATION CONTRACT — different from secretParamNames above, where "api_key"/"api-key" are
    // distinct legitimate entries: a field name is lowercased and stripped of "_"/"-" BEFORE these three
    // lists are consulted, so every entry here must be lowercase with no separators ("apikey", never
    // "api_key" — a separator-carrying entry can never match anything).
    "sensitiveResponseFieldSubstrings": ["password","passwd","secret","apikey","privatekey","credential","passphrase","salt"],
    "sensitiveResponseFieldExactNames": ["token","jwt","hash","pwd","ssn","otp"],
    "sensitiveResponseFieldSuffixes": ["token"],
    "apiVersionSegmentPattern": "(?i)^v[0-9]+(?:\\.[0-9]+)*$",
    "externallyFetchedPaths": ["/","/health","/healthz","/healthcheck","/livez","/readyz","/robots.txt","/sitemap.xml","/sitemap_index.xml","/rss.xml","/feed.xml","/atom.xml","/favicon.ico"],

    // The identifiers you give your own router values. A call on one of these is read as a route.
    // TypeScript/JavaScript files only.
    "routerNames": ["apiRoutes"],

    // Directories that are shared infrastructure rather than a layer, so importing them upward is not a
    // layering violation. Separate from `vocabulary.featureSlicedDesign.shared` below on purpose: this one exempts, that
    // one names an FSD layer.
    // UNLIKE THE KEYS ABOVE, THIS ONE JUDGES EVERY LANGUAGE — it filters the dependency graph, which is
    // built for TypeScript, Python, Java, C#, Go and Rust alike, and it feeds the hierarchy,
    // sibling-cross and layer-co-churn signals. The value written here is JS-shaped, because that is
    // where the list came from: hooks, types, display and __test__ are React and Jest words, and a Java,
    // Go, C# or Python tree spells its shared infrastructure differently (common, internal, pkg, util,
    // Shared). Left as-is in such a tree it exempts nothing you have, so those signals judge your shared
    // directories as ordinary layers. Replace the list with your own names — this is one of the few keys
    // here whose suggested value is one ecosystem's convention rather than a language-neutral one.
    "hierarchySharedDirs": ["utils","types","helpers","hooks","constants","lib","display","__test__"],

    // Your Feature-Sliced Design layout, if you use one. The four keys answer one question together, so
    // they are grouped — but each is replaced on its own, exactly like `packs.disabled` leaves
    // `packs.extraDirs` alone. Retarget `vocabulary.featureSlicedDesign.sliceContainers` if your slices live elsewhere.
    "featureSlicedDesign": {
      "sliceContainers": ["features","domains"],
      "entry": ["pages","routes","api"],
      "shared": ["core","hooks","render","ui","shared","lib","utils","__test__"],
      "baseDirs": ["base"]
    }
  }

  // Left out above because their defaults are almost always right: `sizeCap` (a file bigger than this
  // many bytes is skipped) and `overlays` (Normalized-AST adapter envelope files merged on top of
  // native parsing).
}
"#;
