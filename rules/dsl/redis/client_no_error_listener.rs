use super::{hits, scan, TempDir};

// --- client-no-error-listener ---

#[test]
fn node_redis_create_client_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\nexport const client = createClient({ url: process.env.REDIS_URL });\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_new_redis_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisClient.ts",
        "import Redis from \"ioredis\";\nexport const redis = new Redis(process.env.REDIS_URL);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_cluster_client_with_no_error_listener_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisCluster.ts",
        "import Redis from \"ioredis\";\nexport const cluster = new Redis.Cluster([{ host: \"127.0.0.1\", port: 7000 }]);\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "client-no-error-listener");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn ioredis_import_with_no_client_construction_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ping.ts",
        "import type { Redis as RedisClient } from \"ioredis\";\nexport function ping(client: RedisClient) {\n  return client.ping();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn ioredis_client_with_error_listener_attached_is_not_flagged() {
    // FP guard: the client is created AND `.on('error', ...)` is attached elsewhere in the same file —
    // `require_file_absent` skips the whole file once the veto pattern is found anywhere in it.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/ioredisClient.ts",
        "import Redis from \"ioredis\";\nexport const redis = new Redis(process.env.REDIS_URL);\nredis.on(\"error\", (err) => {\n  console.error(\"Redis connection error\", err);\n});\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn supabase_create_client_without_a_redis_import_is_not_flagged() {
    // FP guard: `createClient(...)` is also the Supabase JS SDK's factory function name — without an
    // ioredis/redis/@redis/client import specifier in the file, the `require_file` gate never opts this
    // file in, so the same-named-but-unrelated call is not mistaken for a redis client.
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/supabaseClient.ts",
        "import { createClient } from \"@supabase/supabase-js\";\nexport const supabase = createClient(process.env.SUPABASE_URL!, process.env.SUPABASE_KEY!);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_error_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\n// redis-error-ok: error listener attached in bootstrap.ts\nexport const client = createClient({ url: process.env.REDIS_URL });\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn create_client_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/redisClient.ts",
        "import { createClient } from \"redis\";\n// const client = createClient({ url: process.env.REDIS_URL }); -- old setup\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "client-no-error-listener").is_empty(),
        "{:?}",
        out.findings
    );
}
