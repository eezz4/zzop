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

/// `zzop-reports` (the JS CLI's default report output dir) and `.zzop-cache` (its default `cacheDir`
/// template value) must be self-scan-excluded by default — a run that writes its own reports/cache inside
/// the analyzed tree must not have the NEXT run walk that output as source (regression pin for the
/// self-scan-pollution fix, blind field test round 3).
#[test]
fn default_skip_dirs_exclude_zzops_own_report_and_cache_output_dirs() {
    let config = cfg();
    assert!(is_skip_dir("zzop-reports", &config));
    assert!(is_skip_dir(".zzop-cache", &config));
}

/// T2 policy pin: the exact `NON_SOURCE_EXTENSIONS` contents. Any edit to this list changes which
/// extensions the "bring an adapter" per-extension disclosure stays silent about — pinned so a change
/// is a conscious, reviewed decision, not an accidental drop/add.
#[test]
fn non_source_extensions_pin() {
    const EXPECTED: &[&str] = &[
        // docs/text
        "md",
        "mdx",
        "txt",
        "rst",
        "adoc", // data/config
        "json",
        "jsonc",
        "json5",
        "yaml",
        "yml",
        "toml",
        "xml",
        "csv",
        "tsv",
        "ini",
        "properties",
        "lock", // styles
        "css",
        "scss",
        "sass",
        "less",
        "styl", // markup-as-asset
        "html",
        "htm", // images
        "png",
        "jpg",
        "jpeg",
        "gif",
        "webp",
        "svg",
        "ico",
        "bmp",
        "avif", // fonts
        "woff",
        "woff2",
        "ttf",
        "otf",
        "eot", // media
        "mp3",
        "mp4",
        "webm",
        "wav",
        "ogg",
        "mov", // binaries/archives
        "zip",
        "gz",
        "tar",
        "pdf",
        "wasm",
        "exe",
        "dll",
        "so",
        "dylib",
        "node",
        "jar",
        "map",
        // misc
        "pem",
        "crt",
    ];
    assert_eq!(
            NON_SOURCE_EXTENSIONS, EXPECTED,
            "NON_SOURCE_EXTENSIONS drifted — update EXPECTED deliberately if this is an intended policy \
             change"
        );
}

#[test]
fn is_non_source_extension_matches_every_pinned_entry() {
    for ext in NON_SOURCE_EXTENSIONS {
        assert!(is_non_source_extension(ext), "{ext}");
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
