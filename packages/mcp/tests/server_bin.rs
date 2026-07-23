//! Binary-level smoke tests for the `zzop-mcp` MCP-server binary (`src/bin/zzop-mcp.rs`) — its
//! non-serving surfaces. The serving path (bare / `mcp` → stdio JSON-RPC) blocks on stdin, so it is
//! covered by the protocol unit tests in `server.rs`, not spawned here; this pins the thin entry's
//! version/help/error lanes so the CLI (`zzop`) and the server can never disagree on the version string.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zzop-mcp"))
        .args(args)
        .output()
        .expect("zzop-mcp binary should spawn")
}

#[test]
fn version_prints_the_shared_server_version_and_exits_zero() {
    for arg in ["version", "--version"] {
        let out = run(&[arg]);
        assert!(out.status.success(), "`zzop-mcp {arg}` must exit 0");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("zzop-mcp {}", zzop_host::server::version()),
            "`zzop-mcp {arg}` prints the same server::version() the CLI's `zzop version` reports"
        );
        assert!(out.stderr.is_empty(), "no stderr on success");
    }
}

#[test]
fn help_prints_usage_to_stdout_and_points_at_the_cli_binary() {
    let out = run(&["help"]);
    assert!(out.status.success(), "help exits 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("usage: zzop-mcp"), "got: {text}");
    // The server binary points terminal users at the `zzop` CLI for subcommands.
    assert!(
        text.contains("zzop"),
        "help should point at the CLI: {text}"
    );
}

#[test]
fn an_unknown_subcommand_is_a_usage_error_exit_two() {
    // A CLI subcommand aimed at the server binary (`zzop-mcp analyze`) is a usage error — that lives on
    // the `zzop` binary. Exit 2, usage on stderr, nothing on stdout.
    let out = run(&["analyze"]);
    assert_eq!(out.status.code(), Some(2), "unknown subcommand exits 2");
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: zzop-mcp"));
    assert!(out.stdout.is_empty(), "nothing on stdout for a usage error");
}
