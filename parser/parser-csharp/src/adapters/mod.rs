//! Framework-vocabulary producers emitting cross-layer IO facts — see crate root doc's "Layout"
//! section. `provides` is the ASP.NET Core PROVIDE-side producer (attribute-routed controllers +
//! minimal-API route registrations); `http_clients` is the `HttpClient` CONSUME-side counterpart.

pub mod http_clients;
pub mod provides;
