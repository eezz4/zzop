// selftest-stub.rs — a fake "MCP server" that misbehaves on purpose, so every validation branch in
// snapshot.mjs can be PROVEN to abort instead of quietly recording a zero.
//
// This exists because a guard that has never been seen red is not known to work. The accident this
// whole harness is built around was a measurement script that called a nonexistent subcommand,
// discarded stderr, wrote 22 zero-byte files, and had them read as "460 findings -> 0, all fixed".
// Asserting in a comment that the harness now catches that is worth nothing; running it against a
// binary that does exactly that is worth something.
//
// NOT part of the cargo workspace — a standalone single file, compiled on demand:
//
//   rustc -O scripts/measure/selftest-stub.rs -o <tmp>/stub.exe
//   for m in empty initonly garbage rpcerror iserror wrongpayload fail; do
//     ZZOP_STUB=$m node scripts/measure/snapshot.mjs --label selftest-$m \
//       --bin <tmp>/stub.exe --config <any corpus>/zzop.config.jsonc --runs <tmp>/runs
//   done
//
// Every one of those must exit nonzero with a HARNESS ABORT naming the specific defect, and must
// leave no directory behind under --runs. A mode that produces a snapshot is a harness regression.
use std::io::Read;

fn main() {
    // Drain stdin the way a real server would, so the harness's write side never blocks.
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    let mode = std::env::var("ZZOP_STUB").unwrap_or_else(|_| "empty".into());
    let init = r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"stub","version":"0"}}}"#;
    match mode.as_str() {
        // Exit 0 with NOTHING on stdout — the exact shape that produced 22 zero-byte snapshots.
        "empty" => {}
        // The initialize reply arrives; the tools/call reply never does.
        "initonly" => println!("{init}"),
        // A crash message interleaved into the JSON-RPC stream.
        "garbage" => {
            println!("{init}");
            println!("Segmentation fault (core dumped)");
        }
        "rpcerror" => {
            println!("{init}");
            println!(
                r#"{{"jsonrpc":"2.0","id":2,"error":{{"code":-32602,"message":"unknown tool"}}}}"#
            );
        }
        // A well-formed reply whose tool result is an ERROR — success at the transport layer only.
        "iserror" => {
            println!("{init}");
            println!(
                r#"{{"jsonrpc":"2.0","id":2,"result":{{"isError":true,"content":[{{"type":"text","text":"config not found"}}]}}}}"#
            );
        }
        // Valid JSON-RPC, valid JSON payload, but not a cross_repo reply.
        "wrongpayload" => {
            println!("{init}");
            println!(
                r#"{{"jsonrpc":"2.0","id":2,"result":{{"content":[{{"type":"text","text":"{{\"hello\":1}}"}}]}}}}"#
            );
        }
        // Nonzero exit whose only explanation is on stderr — the stream the old harness discarded.
        "fail" => {
            eprintln!("stub: unknown subcommand `mcp` -- this binary does not speak MCP");
            std::process::exit(2);
        }
        other => {
            eprintln!("stub: unknown ZZOP_STUB mode {other}");
            std::process::exit(9);
        }
    }
}
