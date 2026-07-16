//! Whole-graph native rule runners + thin delegates to `zzop_rules_graph`/`zzop_rules_http`/
//! `zzop_rules_schema`: circular/unreachable/dead-candidate graph analyses, the call-graph-BFS HTTP
//! rules (re-parses TS off disk — see `run_callgraph_rules`'s own doc for why), the whole-corpus Java
//! Spring provides pass, and the schema x usage JOIN rules. Module root only re-exports; the substance
//! lives in the submodules.

mod callgraph;
mod delegates;
mod java_provides;
mod schema_join;

#[cfg(test)]
mod prisma_client_getter_consistency_tests;

pub(super) use callgraph::run_callgraph_rules;
pub(crate) use delegates::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};
pub(super) use java_provides::run_java_provides_project_pass;
pub(super) use schema_join::run_schema_join_rules;
