// DECOY for security/conn-string-credentials. In scope: `.ts`, no require_file. Every line matches the
// rule's `scheme://user:pass@host` line_pattern shape and is then vetoed by a DIFFERENT arm of its
// exclude_pattern: interpolated credential, well-known placeholder credential, loopback host.
export const DB_URL = `postgres://appuser:${process.env.DB_PASSWORD}@db.example.net:5432/app`;
export const LOCAL_DB_URL = 'mysql://root:root@localhost:3306/app';
export const CACHE_URL = 'redis://cache.example.net:6379/0';
