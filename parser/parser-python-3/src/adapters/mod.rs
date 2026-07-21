//! Framework-vocabulary producers emitting cross-layer IO facts, at the same grade as
//! `zzop_parser_typescript::adapters`: FastAPI route PROVIDES (`fastapi`, projected as framework-neutral
//! router-mount fragments) and `requests`/`httpx` literal HTTP egress CONSUMES (`http_clients`).

pub mod django;
pub mod fastapi;
pub mod http_clients;
pub mod sqlalchemy;
