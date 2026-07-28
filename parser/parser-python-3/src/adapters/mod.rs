//! Framework-vocabulary producers emitting cross-layer IO facts, at the same grade as
//! `zzop_parser_typescript::adapters`: FastAPI route PROVIDES (`fastapi`, projected as framework-neutral
//! router-mount fragments) and `requests`/`httpx` literal HTTP egress CONSUMES (`http_clients`), plus
//! the two AUTH-GUARD evidence producers (`fastapi::guard`, `django_routes::guard`) that feed the
//! framework-neutral `auth-guarded` channel — both judging names through the ONE shared vocabulary in
//! `guard_vocab`.

pub mod django;
pub mod django_routes;
pub mod fastapi;
pub mod guard_vocab;
pub mod http_clients;
pub mod sqlalchemy;
