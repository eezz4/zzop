//! Framework-vocabulary producers: each module here recognizes one framework's or convention's own
//! shape (NestJS/`@n8n/decorators` controller decorators, FE HTTP-call libraries, tRPC routers/proxy
//! clients, Next.js `pages/api` handlers, Hono-style code-registered routers) and emits `IoConsume`/
//! `IoProvide`/fragment IR from it. These are in-process siblings of external envelope producers: an
//! out-of-process adapter for another language injects the same shape of data through the
//! Normalized-AST envelope (`zzop_core::normalized`) instead of a direct Rust call, playing the
//! identical role once it reaches the engine's assembly pass. See `lang`'s module doc for the
//! sibling half of this crate's 2-layer layout.

pub mod controller_decorators;
pub mod db_table_consume;
pub mod egress;
pub mod hono_client;
pub mod next_pages_api;
pub mod router_mounts;
pub mod store_binding;
pub mod trpc_consume;
pub mod trpc_router;
pub mod wrapper_calls;
