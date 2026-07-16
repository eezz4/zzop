//! `zzop_parser_typescript::PRISMA_CLIENT_GETTER` and `zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN`
//! are twin recognizers of the same "Prisma client getter function name" convention, kept in two
//! separate parser crates on purpose: `zzop_core` is vocabulary-free (no Prisma-specific concept
//! belongs there), and a parser-typescript -> parser-prisma dependency edge for one string would be
//! architecturally backwards. This guard — living here since `zzop_engine` already depends on both
//! parsers — catches the two twins silently drifting apart without forcing either coupling.
#[test]
fn prisma_client_getter_twins_stay_in_sync() {
    assert_eq!(
        zzop_parser_typescript::PRISMA_CLIENT_GETTER,
        zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN
    );
}
