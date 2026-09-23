use super::non_source::NON_SOURCE_EXTENSIONS;
use super::*;

fn cfg() -> DispatchConfig {
    DispatchConfig::default()
}

#[test]
fn dispatches_known_typescript_extensions() {
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
        let path = format!("src/x.{ext}");
        assert_eq!(
            dispatch(&path, &cfg()),
            Some(Language::TypeScript),
            "{path}"
        );
    }
}

#[test]
fn dispatches_prisma_extension() {
    assert_eq!(dispatch("db/schema.prisma", &cfg()), Some(Language::Prisma));
}

#[test]
fn unknown_extension_dispatches_to_none() {
    assert_eq!(dispatch("README", &cfg()), None);
    // .jsp/.jspx/.tag stay lexical-fallback; only .java gets the structural parser.
    assert_eq!(dispatch("src/Foo.jsp", &cfg()), None);
}

#[test]
fn dispatches_java_extension_to_the_structural_parser() {
    assert_eq!(dispatch("src/Foo.java", &cfg()), Some(Language::Java21));
}

#[test]
fn dispatches_python_extensions() {
    for ext in ["py", "pyi"] {
        let path = format!("src/x.{ext}");
        assert_eq!(dispatch(&path, &cfg()), Some(Language::Python), "{path}");
    }
}

#[test]
fn dispatches_rust_extension() {
    assert_eq!(dispatch("src/main.rs", &cfg()), Some(Language::Rust));
}

#[test]
fn dispatches_go_extension() {
    assert_eq!(dispatch("src/main.go", &cfg()), Some(Language::Go));
}

#[test]
fn dispatches_sql_extension() {
    assert_eq!(
        dispatch("db/migrations/001_init.sql", &cfg()),
        Some(Language::Sql)
    );
}

/// Pin for [`declaration_only_extension`], both directions: exactly the `Prisma`/`Sql` arms are
/// declaration-only, and every code-hosting arm (plus unknown/lexical extensions) is not. The
/// predicate is derived from the dispatch table, so what this pins is the CLASSIFICATION — a new
/// dispatch arm, or a Prisma/Sql arm growing symbols/imports, must re-justify its row here.
#[test]
fn declaration_only_extensions_are_exactly_the_prisma_and_sql_arms() {
    for ext in ["prisma", "sql", "PRISMA"] {
        assert!(declaration_only_extension(ext), "{ext}");
    }
    for ext in [
        "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "java", "py", "pyi", "rs", "go",
        "cs", "md", "html", "unknown",
    ] {
        assert!(!declaration_only_extension(ext), "{ext}");
    }
}

#[test]
fn dispatches_csharp_extension() {
    assert_eq!(
        dispatch("src/UsersController.cs", &cfg()),
        Some(Language::CSharp)
    );
}

#[test]
fn extension_match_is_case_insensitive() {
    assert_eq!(dispatch("src/Foo.TS", &cfg()), Some(Language::TypeScript));
}

#[test]
fn glob_override_wins_over_extension_map() {
    let config = DispatchConfig {
        glob_overrides: vec![("legacy/**".to_string(), Language::Prisma)],
        ..cfg()
    };
    // `.ts` would normally be TypeScript, but the override forces the whole `legacy/` subtree to Prisma.
    assert_eq!(
        dispatch("legacy/schema.ts", &config),
        Some(Language::Prisma)
    );
    assert_eq!(
        dispatch("fresh/schema.ts", &config),
        Some(Language::TypeScript)
    );
}

#[test]
fn glob_single_star_does_not_cross_slash() {
    let config = DispatchConfig {
        glob_overrides: vec![("src/*.ts".to_string(), Language::Prisma)],
        ..cfg()
    };
    assert_eq!(dispatch("src/Foo.ts", &config), Some(Language::Prisma));
    // `*` must not cross `/`, so the override doesn't apply here — falls through to the extension map.
    assert_eq!(
        dispatch("src/nested/Foo.ts", &config),
        Some(Language::TypeScript)
    );
}

#[test]
fn default_skip_dirs_cover_common_build_and_vcs_output() {
    let config = cfg();
    for name in [
        "node_modules",
        "dist",
        "build",
        ".next",
        ".git",
        "target",
        ".yarn",
    ] {
        assert!(is_skip_dir(name, &config), "{name}");
    }
    assert!(!is_skip_dir("src", &config));
}

/// zzop's own output dirs must be self-scan-excluded by default — a run that writes its own
/// reports/cache inside the analyzed tree must not have the NEXT run walk that output as source
/// (regression pin for the self-scan-pollution fix, blind field test round 3).
///
/// `.zzop` is the LIVE one: the config front-end defaults `cacheDir` to `zzop_cache::DEFAULT_CACHE_DIR`,
/// so every run whose config omits `cacheDir` writes there. It is asserted through the shared constant rather than as a
/// literal, so moving the default cache directory can never leave the skip list behind.
/// `zzop-reports`/`.zzop-cache` are the removed JS CLI's names, kept as legacy defense.
#[test]
fn default_skip_dirs_exclude_zzops_own_report_and_cache_output_dirs() {
    let config = cfg();
    assert!(is_skip_dir(zzop_cache::TOOL_DIR, &config));
    assert!(is_skip_dir("zzop-reports", &config));
    assert!(is_skip_dir(".zzop-cache", &config));
}

/// The counterpart of the pin above, and the reason `zzop/` is NOT in the skip list: a user-authored
/// `zzop/rules/` pack (the no-dot half of the on-disk layout dichotomy) is source a human wrote and
/// wants analyzed. One character separates it from the tool-owned `.zzop/`; only one of them is skipped.
#[test]
fn the_user_authored_zzop_dir_is_not_skipped() {
    let config = cfg();
    assert!(!is_skip_dir("zzop", &config));
}

/// T2 policy pin: the exact `NON_SOURCE_EXTENSIONS` contents, KIND INCLUDED. Any edit to this list
/// changes which extensions the "bring an adapter" per-extension disclosure stays silent about, and any
/// edit to a kind changes which extensions the coverage-gap surfaces may report — pinned so a change to
/// either is a conscious, reviewed decision, not an accidental drop/add/reclassify.
#[test]
fn non_source_extensions_pin() {
    use NonSourceKind::{DataConfig as D, NoFactsToLose as N};
    const EXPECTED: &[(&str, NonSourceKind)] = &[
        // docs/text
        ("md", N),
        ("mdx", N),
        ("txt", N),
        ("rst", N),
        ("adoc", N),
        ("rtf", N),
        // data/config
        ("json", D),
        ("jsonc", D),
        ("json5", D),
        ("yaml", D),
        ("yml", D),
        ("toml", D),
        ("xml", D),
        ("csv", D),
        ("tsv", D),
        ("ini", D),
        ("properties", D),
        ("lock", D),
        ("po", D),
        // styles
        ("css", N),
        ("scss", N),
        ("sass", N),
        ("less", N),
        ("styl", N),
        // markup-as-asset
        ("html", D),
        ("htm", D),
        // images
        ("png", N),
        ("jpg", N),
        ("jpeg", N),
        ("gif", N),
        ("webp", N),
        ("svg", N),
        ("ico", N),
        ("bmp", N),
        ("avif", N),
        // fonts
        ("woff", N),
        ("woff2", N),
        ("ttf", N),
        ("otf", N),
        ("eot", N),
        // media
        ("mp3", N),
        ("mp4", N),
        ("webm", N),
        ("wav", N),
        ("ogg", N),
        ("mov", N),
        // binaries/archives
        ("zip", N),
        ("gz", N),
        ("tar", N),
        ("pdf", N),
        ("wasm", N),
        ("exe", N),
        ("dll", N),
        ("so", N),
        ("dylib", N),
        ("node", N),
        ("jar", N),
        ("map", N),
        // compiled/generated artifacts — a parser frontend for these is not a thing anyone writes
        ("pyc", N),
        ("mo", N),
        ("egg", N),
        ("arrow", N),
        ("snap", D),
        // packaging outputs
        ("deb", N),
        ("rpm", N),
        // archives (siblings of zip/gz/tar above)
        ("bz2", N),
        ("xz", N),
        ("lzma", N),
        ("tgz", N),
        // binary data/geo/db formats
        ("dbf", N),
        ("shp", N),
        ("shx", N),
        ("mmdb", N),
        ("tif", N),
        ("tiff", N),
        ("graffle", N),
        // logs and editor/backup residue
        ("log", N),
        ("backup", N),
        // go module manifests — the `lock` family, structured data rather than a language
        ("sum", D),
        ("mod", D),
        ("work", D),
        // misc
        ("pem", N),
        ("crt", N),
        ("cert", N),
    ];
    assert_eq!(
            NON_SOURCE_EXTENSIONS, EXPECTED,
            "NON_SOURCE_EXTENSIONS drifted — update EXPECTED deliberately if this is an intended policy \
             change"
        );
}

#[test]
fn is_non_source_extension_matches_every_pinned_entry() {
    for (ext, _) in NON_SOURCE_EXTENSIONS {
        assert!(is_non_source_extension(ext), "{ext}");
    }
}

/// The whole point of the split, in one assertion: the two questions must NOT have the same answer for
/// every extension. Without this, a future edit that reclassifies every `DataConfig` entry back to
/// `NoFactsToLose` restores the measured mall defect (114 `.xml` files holding 906 SQL statements,
/// reported nowhere) while the pin above still passes with an internally consistent table.
#[test]
fn the_two_questions_disagree_on_the_data_config_group() {
    // The extension the defect was measured on. Question 1 says "do not nag for an XML parser";
    // question 2 says "an unread XML majority can absolutely have cost you something".
    assert!(
        is_non_source_extension("xml"),
        "question 1 must still say yes"
    );
    assert!(
        extraction_can_lose_facts("xml"),
        "question 2 must say a dispatch-None on .xml can cost facts — this is the mall MyBatis case"
    );
    // ...and they must still AGREE where agreement is correct, or the split would just be noise.
    for inert in ["png", "css", "woff2", "pem"] {
        assert!(is_non_source_extension(inert), "{inert}");
        assert!(
            !extraction_can_lose_facts(inert),
            "{inert} has no symbol/import/io fact to lose"
        );
    }
    // `.md` answers both questions the same way as the inert group, for a DIFFERENT reason, and the
    // reason is what a future reader needs: a VitePress/Nuxt `.md` page IS a Vue SFC and its
    // `<script setup>` imports are real edges — they are not lost because the import pre-scan reads
    // them (`zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`, consumed by
    // `assemble::helpers::is_prescan_ext`), not because they are absent. That gate is what makes this
    // pair true; see the roster's doc. `prescan_roster_is_orthogonal_to_the_non_source_table` below
    // nails those two facts together.
    assert!(
        is_non_source_extension("md"),
        "no Markdown language frontend is a plausible ask, and this predicate is ALSO the \
         dead-file-candidate exclusion — flipping it makes every prose page a deletion candidate"
    );
    assert!(
        !extraction_can_lose_facts("md"),
        "measured: flipping this adds a gap row on be-gin (6 of 40 files, zero script blocks) and \
         NONE on koel (47 of 2773, ten script blocks) — it would fire where the phenomenon is absent \
         and stay silent where it is present"
    );
    // Source extensions are outside the table entirely and answer both questions the other way.
    for source in ["ts", "java", "vue"] {
        assert!(!is_non_source_extension(source), "{source}");
        assert!(extraction_can_lose_facts(source), "{source}");
    }
}

/// The wire vocabulary is a CLOSED set with one owner, and every table entry maps into it. A surface
/// spelling its own token is the drift this function exists to prevent.
#[test]
fn extension_content_kind_covers_the_table_and_the_source_case() {
    for (ext, _) in NON_SOURCE_EXTENSIONS {
        let kind = extension_content_kind(ext);
        assert!(
            kind == "data-config" || kind == "no-facts-to-lose",
            "{ext} rendered as {kind:?}, which is not a non-source token"
        );
    }
    assert_eq!(extension_content_kind("ts"), "source");
    assert_eq!(extension_content_kind("XML"), "data-config");
    assert_eq!(extension_content_kind("PNG"), "no-facts-to-lose");
}

/// 🔴 The residual must be the WEAKEST token, not the strongest (2026-09-06, review ledger V23).
///
/// `"source"` names a remedy the consuming surface publishes as "bring a parser adapter". While the
/// residual was spelled `"source"`, a coverage reply sent readers to write one for `.gitignore`,
/// `.env`, `.npmignore` — and for `local`, which is not a filetype at all, only the tail of
/// `.env.local`. A vocabulary whose DEFAULT is its loudest claim fails toward alarm every time its
/// table has a gap, and a hand-kept table always has one.
#[test]
fn an_unclassified_extension_is_not_called_source() {
    for ext in [
        "gitignore",
        "env",
        "npmignore",
        "local",
        "editorconfig",
        "gitattributes",
    ] {
        assert_eq!(
            extension_content_kind(ext),
            "unclassified",
            "{ext} is not a language this build knows anything about"
        );
    }
}

/// The other direction, and the reason the residual could not simply be renamed: a dialect this build
/// recognizes but cannot parse IS source, and "bring an adapter" is the right thing to tell someone
/// about it. That contract is pinned on the reply surface too
/// (`query_coverage::tests::unread_entries_are_ext_ordered_...`), which is what caught the first
/// attempt at this fix.
#[test]
fn a_recognized_dialect_with_no_parser_is_still_source() {
    for ext in ["vue", "svelte", "php", "rb", "kt", "sh"] {
        assert_eq!(
            extension_content_kind(ext),
            "source",
            "{ext} is a language; an adapter is the honest remedy"
        );
    }
}

/// The source signal comes from two places and neither may quietly become a copy of the other: the
/// parser table answers for what this build reads, [`SOURCE_DIALECT_EXTENSIONS`] for what it only
/// recognizes. An extension in BOTH would mean the hand-kept list had grown a shadow of the table.
#[test]
fn the_dialect_list_never_shadows_the_parser_table() {
    for ext in super::non_source::SOURCE_DIALECT_EXTENSIONS {
        assert!(
            crate::dispatch::language_for_extension(ext).is_none(),
            "{ext} is already dispatched to a parser -- the dialect list must not restate the table"
        );
    }
}

#[test]
fn is_non_source_extension_is_case_insensitive() {
    assert!(is_non_source_extension("MD"));
    assert!(is_non_source_extension("Png"));
}

#[test]
fn is_non_source_extension_rejects_real_source_and_template_dialects() {
    for ext in ["ts", "py", "sql", "rb", "go"] {
        assert!(!is_non_source_extension(ext), "{ext}");
    }
    // Template dialects that embed source in markup — deliberately NOT in the non-source list, since
    // an adapter for these is a real, plausible gap worth naming.
    for ext in ["jsp", "erb", "vue", "svelte"] {
        assert!(!is_non_source_extension(ext), "{ext}");
    }
}

/// The import PRE-SCAN roster (`zzop_parser_typescript`, which owns it) is a THIRD extension question
/// that cuts ACROSS this table, and this is the assertion that says so in both directions. It lives in
/// THIS file because `NonSourceKind::NoFactsToLose`'s claim about `.md`/`.mdx` is only true while the
/// roster carries them: a future edit that drops one back out (restoring the koel defect, or the MDX
/// one) or that "tidies up" by pulling `.md` out of the non-source table instead (which would mint a
/// dead-file candidate for every prose page in every tree) fails right here rather than in a corpus run
/// nobody repeats.
#[test]
fn prescan_roster_is_orthogonal_to_the_non_source_table() {
    use zzop_parser_typescript::{is_sfc_script_host, prescan_mode, PrescanMode};
    use zzop_parser_typescript::{PRESCAN_IMPORT_HOSTS, SFC_SCRIPT_HOST_EXTENSIONS};
    // Members on BOTH sides: `.md` and `.mdx` are non-source AND pre-scan hosts. These two lines are
    // the defect the third axis exists for.
    assert!(prescan_mode("md").is_some() && is_non_source_extension("md"));
    assert!(prescan_mode("mdx").is_some() && is_non_source_extension("mdx"));
    // ...and their answer to question 2 stays `false` BECAUSE the roster carries them — the imports are
    // read, not absent. Drop either from the roster and its pair becomes a lie again.
    assert!(!extraction_can_lose_facts("md") && !extraction_can_lose_facts("mdx"));
    // Members on the other side only, so the two lists provably are not the same list. `astro` is here
    // rather than in the table above on purpose: it keeps earning its coverage-gap row, because the
    // frontmatter arm reads its IMPORTS and nothing else.
    for host in ["vue", "svelte", "astro"] {
        assert!(prescan_mode(host).is_some(), "{host}");
        assert!(!is_non_source_extension(host), "{host}");
    }
    for non_host in ["ts", "png", "json", "txt", "html"] {
        assert!(prescan_mode(non_host).is_none(), "{non_host}");
    }
    // `mdx`/`astro` are absent from the `<script>`-BLOCK roster and present on the wider one — their
    // imports are not inside a `<script>` tag, so that lexical extract would find nothing, and the
    // bare-ESM and frontmatter arms are what reads them instead. A `<script>`-roster row would be a
    // name claiming reach the body lacks; the pair table is what tells the two apart.
    for absent in ["mdx", "astro"] {
        assert!(!is_sfc_script_host(absent), "{absent}");
        assert!(prescan_mode(absent).is_some(), "{absent}");
        assert_ne!(
            prescan_mode(absent),
            Some(PrescanMode::ScriptBlocks),
            "{absent}"
        );
    }
    assert!(
        prescan_mode("MD").is_some() && prescan_mode("Astro").is_some(),
        "case-insensitive, like non_source_kind"
    );
    assert_eq!(
        SFC_SCRIPT_HOST_EXTENSIONS,
        ["vue", "svelte", "md"],
        "roster drifted — update deliberately"
    );
    // The wider table is pinned too, MODE INCLUDED — a new host arriving without an arm, or with the
    // wrong one, is the drift a set-shaped pin could not see.
    assert_eq!(
        PRESCAN_IMPORT_HOSTS,
        [
            ("vue", PrescanMode::ScriptBlocks),
            ("svelte", PrescanMode::ScriptBlocks),
            ("md", PrescanMode::ScriptBlocks),
            ("mdx", PrescanMode::BareEsm),
            ("astro", PrescanMode::AstroFence),
        ],
        "pre-scan roster drifted — update deliberately"
    );
}

/// Seals the config-facing language vocabulary in both directions.
///
/// `as_wire`'s match is exhaustive with no wildcard, so a NEW `Language` variant fails to compile until
/// it is given a spelling — that half needs no test. What a test must hold is the other half:
/// `WIRE_NAMES` is a hand-kept list, and a variant missing from it would be unnameable in a config while
/// every compile stayed green. The length assertion is what fails then, and it fails one screen away
/// from the match the author has just edited.
#[test]
fn every_language_round_trips_through_its_config_spelling() {
    assert_eq!(
        Language::WIRE_NAMES.len(),
        8,
        "a Language variant was added or removed — give it a config spelling in WIRE_NAMES too, \
         otherwise parsers.globOverrides cannot name it"
    );
    for lang in Language::WIRE_NAMES {
        assert_eq!(
            Language::from_wire(lang.as_wire()),
            Some(*lang),
            "{} must parse back to itself",
            lang.as_wire()
        );
    }
    assert_eq!(
        Language::from_wire("java21"),
        None,
        "the grammar version is not the config spelling"
    );
    assert_eq!(
        Language::from_wire("Java"),
        None,
        "spellings are exact, not case-folded"
    );
}

// ---------------------------------------------------------------------------------------------
// ONE glob dialect (review ledger V98). `glob_overrides` used to run a second, narrower translator
// than the `exclude`/`suppressions` key in the same config — two user-writable glob keys that meant
// different things. Measured before the fold: 7 of 12 cases disagreed. These pin the three shapes that
// changed, so the dialects cannot drift apart again without a red test.
// ---------------------------------------------------------------------------------------------

/// The sharpest one, and a defect rather than a dialect choice: the old translator never escaped `?`,
/// so `file?.ts` compiled to `^file?\.ts$` and the `?` acted as a regex QUANTIFIER — it matched
/// `fil.ts` and missed `file1.ts`, precisely inverting what a glob `?` means.
#[test]
fn a_question_mark_is_one_character_not_a_quantifier() {
    let config = DispatchConfig {
        glob_overrides: vec![("src/file?.ts".to_string(), Language::Prisma)],
        ..cfg()
    };
    assert_eq!(dispatch("src/file1.ts", &config), Some(Language::Prisma));
    assert_eq!(
        dispatch("src/fil.ts", &config),
        Some(Language::TypeScript),
        "the old translator matched THIS one and not file1.ts"
    );
}

/// `{a,b}` alternates. The old translator escaped the braces, so the pattern only matched a path with
/// literal brace characters in it — something no real repository has.
#[test]
fn a_brace_group_alternates() {
    let config = DispatchConfig {
        glob_overrides: vec![(
            "src/{legacy,vendor}/schema.ts".to_string(),
            Language::Prisma,
        )],
        ..cfg()
    };
    assert_eq!(
        dispatch("src/legacy/schema.ts", &config),
        Some(Language::Prisma)
    );
    assert_eq!(
        dispatch("src/vendor/schema.ts", &config),
        Some(Language::Prisma)
    );
    assert_eq!(
        dispatch("src/fresh/schema.ts", &config),
        Some(Language::TypeScript)
    );
}

/// `**/` matches zero directories, so a root-level file is covered. The old translator required at
/// least one directory, which meant `**/schema.prisma` silently skipped the one at the tree root.
#[test]
fn a_leading_double_star_matches_zero_directories() {
    let config = DispatchConfig {
        glob_overrides: vec![("**/schema.ts".to_string(), Language::Prisma)],
        ..cfg()
    };
    assert_eq!(dispatch("schema.ts", &config), Some(Language::Prisma));
    assert_eq!(dispatch("db/schema.ts", &config), Some(Language::Prisma));
}

/// The two keys a user can write globs into must answer the same question the same way. This is the
/// invariant the whole fold exists for — asserted directly against core's matcher, not inferred.
#[test]
fn the_dispatch_key_and_the_exclude_key_share_one_dialect() {
    for (glob, path) in [
        ("src/file?.ts", "src/file1.ts"),
        ("src/{a,b}/x.ts", "src/a/x.ts"),
        ("**/x.ts", "x.ts"),
        ("legacy/**", "legacy/a/b.ts"),
        ("src/*.ts", "src/nested/a.ts"),
    ] {
        let config = DispatchConfig {
            glob_overrides: vec![(glob.to_string(), Language::Prisma)],
            ..cfg()
        };
        assert_eq!(
            dispatch(path, &config) == Some(Language::Prisma),
            zzop_core::glob_matches(glob, path),
            "dispatch and zzop_core::glob_matches disagree on ({glob}, {path})"
        );
    }
}
