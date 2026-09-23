//! MINING THE TEMPLATE — how the shared segments of a rule's messages are FOUND.
//!
//! Split out of [`super`] so the wire contract (what is stored, what each finding keeps, what the
//! bytes cost) can be read without the alignment underneath it, and because the two answer to
//! different standards: nothing in this file has to be RIGHT, only cheap and usually good. Its
//! output is a PROPOSAL for where the seams are, and `super::split` plus the byte-identity check
//! there are what make the result correct whatever this proposed. A worse proposal costs bytes; it
//! cannot cost a fact.

use super::super::cost;

/// Alignment is quadratic in tokens, so a message longer than this folds nothing rather than
/// spending a reply's whole budget on one `circular` finding carrying a ~300-hop cycle path. Chosen
/// against the shipped population: the longest message that actually folds on the dogfood corpus is
/// ~800 tokens, and the ones above this cap are the data-as-prose cases that want a cap of their own.
const MAX_TOKENS: usize = 1200;

/// Byte spans of the whitespace-separated runs of `s`. Whitespace is the seam because an interpolated
/// value is a word: aligning on characters would find "common" fragments inside two unrelated
/// identifiers and propose seams no reader would recognize.
fn token_spans(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if let Some(st) = start.take() {
                out.push((st, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(st) = start {
        out.push((st, s.len()));
    }
    out
}

/// Longest common subsequence of two token lists, as the index pairs that survive.
fn lcs_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in 1..=n {
        for j in 1..=m {
            dp[i * w + j] = if a[i - 1] == b[j - 1] {
                dp[(i - 1) * w + j - 1] + 1
            } else {
                dp[(i - 1) * w + j].max(dp[i * w + j - 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] && dp[(i - 1) * w + j - 1] + 1 == dp[i * w + j] {
            out.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * w + j] >= dp[i * w + j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out.reverse();
    out
}

/// Byte ranges of `a` that appear in `b` CONTIGUOUSLY and byte-identically, separators included.
///
/// The contiguity check is the correction that made this work: a token subsequence alone says the
/// words appear in order, not that they sit next to each other, so a run taken straight from the LCS
/// can be absent from `b` as a substring. Two rule groups on cal.com (95 and 54 findings) failed to
/// split for exactly that reason before this loop compared the separators too.
fn align_runs(
    a: &str,
    b: &str,
    ta: &[(usize, usize)],
    tb: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let at: Vec<&str> = ta.iter().map(|&(s, e)| &a[s..e]).collect();
    let bt: Vec<&str> = tb.iter().map(|&(s, e)| &b[s..e]).collect();
    let pairs = lcs_pairs(&at, &bt);
    let mut runs = Vec::new();
    let mut p = 0;
    while p < pairs.len() {
        let mut q = p;
        while q + 1 < pairs.len() {
            let (i, j) = pairs[q];
            let (i2, j2) = pairs[q + 1];
            if i2 != i + 1 || j2 != j + 1 || a[ta[i].1..ta[i2].0] != b[tb[j].1..tb[j2].0] {
                break;
            }
            q += 1;
        }
        runs.push((ta[pairs[p].0].0, ta[pairs[q].0].1));
        p = q + 1;
    }
    runs
}

/// The literal parts shared by every message in `msgs`, anchored on `msgs[0]`.
///
/// A byte of `msgs[0]` survives only if EVERY other message contains it inside an aligned run, and
/// run boundaries are recorded as cuts so a part can never span two runs that are adjacent here but
/// not there. An empty leading or trailing part is added exactly when the messages do not all share
/// that end, which is what lets a residue sit at either edge without paying for a slot when it does
/// not. Returns `None` when there is no hole — a message set with nothing varying is the EXACT
/// fold's population, priced by its own arithmetic.
pub(super) fn derive_parts(msgs: &[&str]) -> Option<Vec<String>> {
    let m0 = msgs[0];
    let t0 = token_spans(m0);
    if t0.is_empty() || t0.len() > MAX_TOKENS {
        return None;
    }
    let mut mask = vec![true; m0.len()];
    let mut cut = vec![false; m0.len() + 1];
    for m in &msgs[1..] {
        let tk = token_spans(m);
        if tk.len() > MAX_TOKENS {
            return None;
        }
        let mut keep = vec![false; m0.len()];
        for (s, e) in align_runs(m0, m, &t0, &tk) {
            keep[s..e].fill(true);
            cut[s] = true;
            cut[e] = true;
        }
        for (slot, k) in mask.iter_mut().zip(keep) {
            *slot &= k;
        }
    }
    let mut runs: Vec<String> = Vec::new();
    let mut i = 0;
    while i < m0.len() {
        if !mask[i] {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < m0.len() && mask[j] && !cut[j] {
            j += 1;
        }
        runs.push(m0.get(i..j)?.to_string());
        i = j;
    }
    let (first, last) = (runs.first()?.clone(), runs.last()?.clone());
    let lead = msgs.iter().all(|m| m.starts_with(first.as_str()));
    let tail = msgs.iter().all(|m| m.ends_with(last.as_str()));
    let mut parts: Vec<String> = Vec::new();
    if !lead {
        parts.push(String::new());
    }
    parts.extend(runs);
    if !tail {
        parts.push(String::new());
    }
    (parts.len() >= 2).then_some(parts)
}

/// Drops middle parts too short to be worth their own slot, folding them back into the residues that
/// surround them. A HEURISTIC on shape only — `super::fold`'s gate re-prices whatever this returns
/// exactly, so a wrong call here costs a few bytes and can never cost a fact.
pub(super) fn prune(mut parts: Vec<String>, n: usize) -> Option<Vec<String>> {
    loop {
        let dead = (1..parts.len().saturating_sub(1))
            .find(|&i| cost::template_part_is_dead_weight(parts[i].len() as i64, n as i64));
        match dead {
            Some(i) => {
                parts.remove(i);
                if parts.len() < 2 {
                    return None;
                }
            }
            None => return Some(parts),
        }
    }
}
