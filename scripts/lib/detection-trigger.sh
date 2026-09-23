#!/usr/bin/env bash
# detection-trigger.sh — the ONE spelling of "paths whose bytes can change what the analyzer finds".
#
# Two scripts need this set and they need the SAME set:
#   * scripts/detection-gate-if-touched.sh  — arms the gate when a COMMIT stages one of these paths.
#   * scripts/detection-gate-staleness.sh   — counts how many commits have touched one of these paths
#                                             SINCE the gate last recorded a verdict.
#
# Those two numbers only mean anything together: "the gate last scored N commits ago, and M of those
# N could have moved a finding". If each script carried its own copy of the pattern, the second number
# would silently stop being about the first the moment one copy was widened — and this repository has
# already paid for that class twice (check-guards-wired.sh's two `invoked_in`/`defense_invoked_anywhere`
# functions drifted apart on a word boundary; scripts/lib/tracked-grep.sh exists because four isolation
# guards had copy-pasted an enumeration whose exclusion sets had already diverged). The pattern moved
# here the day the second reader was written, before there were two copies rather than after.
#
# Sourced, never executed: it defines one variable and has no exit status of its own. It lives in
# scripts/lib/ so check-guards-wired.sh's `scripts/check-*.sh` glob and its `DEFENSES` enumeration
# (which excludes `scripts/lib/`) do not demand a pre-commit/ci.yml invocation site for it.
#
# WHAT THE SET IS AND IS NOT — the reasoning is owned by detection-gate-if-touched.sh's header, which
# states both the cost measurement that chose it and the blind spots it leaves (`crates/summary`,
# `crates/metrics`, `packages/`, anything reached only at runtime). Read that before widening this.
# Widening here widens BOTH readers at once, which is the point of the file.
DETECTION_TRIGGER_RE='^(rules|parser|cases)/|^crates/(engine|core)/src/'
