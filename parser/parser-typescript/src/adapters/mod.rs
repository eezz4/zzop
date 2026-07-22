//! Framework-vocabulary producers: each module here recognizes one framework's or convention's own
//! shape (NestJS/`@n8n/decorators` controller decorators, FE HTTP-call libraries, tRPC routers/proxy
//! clients, Next.js `pages/api` handlers, Hono-style code-registered routers) and emits `IoConsume`/
//! `IoProvide`/fragment IR from it. These are in-process siblings of external envelope producers: an
//! out-of-process adapter for another language injects the same shape of data through the
//! Normalized-AST envelope (`zzop_core::normalized`) instead of a direct Rust call, playing the
//! identical role once it reaches the engine's assembly pass. See `lang`'s module doc for the
//! sibling half of this crate's 2-layer layout.

pub mod class_shapes;
pub mod client_base;
pub mod client_base_generated;
pub mod controller_decorators;
pub mod db_table_consume;
pub mod egress;
pub mod entity_decorators;
pub mod global_prefix;
pub mod hono_client;
pub mod nest_middleware;
pub mod next_pages_api;
pub mod pathname_dispatch;
pub mod router_mounts;
pub mod trpc_consume;
pub mod trpc_router;
pub mod typeorm_repository;
pub mod wrapper_calls;
