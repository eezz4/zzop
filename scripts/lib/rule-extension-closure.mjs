// Same-syntax extension closure for DSL rule `file_pattern`s. See check-rule-extension-closure.sh
// for the defect this answers; this file owns the MECHANISM only.
//
// The rule: a pattern that admits one member of a same-syntax family must admit all of it. `.mts` and
// `.cts` are `.ts` with a module system bolted on; `.jsx` is `.js` with JSX syntax. A pattern reading
// one and not the others is not making a choice, it is missing a case.
import fs from "fs";
import path from "path";

/// Deliberately NOT closed over: `ts => js` (a TypeScript-only rule is a real choice, and two rules
/// make it), and `py => pyi` (a `.pyi` is a type stub with no runtime code, so a runtime rule cannot
/// fire in one — the dispatch table sends `.pyi` to Python, and that is still correct).
export const SAME_SYNTAX_FAMILIES = {
  ts: ["tsx", "mts", "cts"],
  js: ["jsx", "mjs", "cjs"],
};

/// Rule-level exceptions: a rule that deliberately does NOT read a family member.
///
/// 🔴 This exists because its absence caused a defect (2026-09-13, review ledger V177). The families
/// above are a LANGUAGE claim (`.mts` is `.ts` with a module system), but `.tsx`/`.jsx` also carry a
/// ROLE claim in practice: under `routes/`, `api/` or `server/`, a `.tsx` file is a UI route
/// component, not backend source. `console-in-be`'s pattern excluded BOTH `tsx` and `jsx` -- a
/// consistent choice, not a missing case -- and this guard had nowhere to say so, so it read the
/// choice as a gap. A repair batch then widened it, and a Remix-style `app/routes/index.tsx` React
/// component started being reported as "Console write in backend-path source".
///
/// ⚠ The list is a CONTRACT, not a convenience (design cheat-sheet §2). Each entry owes a `why` that
/// a later reader can judge, and `assertExceptionsAreLive` below makes an entry that stopped being
/// true RED -- an exemption that outlives its subject absolves whatever takes the same shape next,
/// which is the stale-exemption discipline `check-mcp-tools-listed.sh` already runs.
export const FAMILY_EXCEPTIONS = [
  {
    ruleId: "console-in-be",
    skip: ["tsx", "jsx"],
    why:
      "Path-scoped to backend-role directories, and its message promises 'backend-path source'. A " +
      ".tsx/.jsx file under routes/ or api/ is a UI route component (Remix, Next app router, Astro), " +
      "so admitting them reports React components as backend logging. The module variants .mts/.cts " +
      "carry no JSX and ARE in scope.",
  },
];

/// A rule's declared skips, or an empty array.
export function exceptionFor(ruleId) {
  const hit = FAMILY_EXCEPTIONS.find((e) => e.ruleId === ruleId);
  return hit ? hit.skip : [];
}

/// Throws when an exception no longer describes reality: the rule is gone (renamed/deleted), or its
/// pattern now admits the extension the entry claims it deliberately skips. Either way the entry is
/// no longer a decision, it is cover.
export function assertExceptionsAreLive(rules) {
  for (const e of FAMILY_EXCEPTIONS) {
    const matching = rules.filter((r) => r.id === e.ruleId);
    if (matching.length === 0) {
      throw new Error(
        `family exception names rule ${JSON.stringify(e.ruleId)}, which no pack defines. ` +
          `Renamed or deleted -- delete the exception rather than leaving it to absolve the next rule.`
      );
    }
    for (const r of matching) {
      const admitted = admittedExtensions(r.filePattern);
      const live = e.skip.filter((x) => admitted.has(x));
      if (live.length) {
        throw new Error(
          `family exception for ${e.ruleId} claims it deliberately skips ${live.join(", ")}, ` +
            `but its file_pattern admits them. The exception is stale -- delete it or fix the pattern.`
        );
      }
    }
  }
}

/// Every extension a `file_pattern` can end a path with.
///
/// 🔴 This replaced a probe that asked `re.test("x." + ext)` (review ledger V138). That probe could
/// not reach a PATH-SCOPED pattern: `(^|/)(migrations?|...)/.*\.(sql|ts|...)$` never matches a bare
/// `x.ts`, so the rule looked like it admitted NOTHING, no family check ran for it, and the guard
/// printed `clean`. Four shipped patterns are path-scoped, and one of them was the gap this very
/// guard was written to catch -- missed in the commit that created the guard.
///
/// ⚠ The hole was in the INPUT, not in an exception handler. The script did not fail; it reported
/// what it could not read as `admits nothing`, which is spelled the same as `has no gap`. So the
/// mechanism no longer synthesises a subject: it reads the pattern's own extension vocabulary, and
/// THROWS when a pattern yields none. A checker that cannot read a pattern must not pass it.
///
/// Two spellings appear in the shipped packs and both are handled:
///   - a group  -- `\.(ts|tsx|mts|cts)$`, including alternatives with an optional char (`ya?ml`)
///   - a literal -- `\.tsx?$`, `\.go$`
/// A pattern with several such tails (`\.(tsx|jsx)$|fe/.*\.(ts|js)$`) contributes the UNION. That
/// is a widening approximation: it can hide a family gap that exists in one arm and not the other.
/// It is still strictly better than the synthetic probe, which saw neither arm of that same pattern.
export function admittedExtensions(filePattern) {
  if (!filePattern) return new Set();
  const out = new Set();
  for (const m of filePattern.matchAll(/\\\.\(([^()]*)\)/g)) {
    for (const alt of m[1].split("|")) expandOptionals(alt.trim()).forEach((e) => out.add(e));
  }
  for (const m of filePattern.matchAll(/\\\.([A-Za-z0-9?]+)/g)) {
    expandOptionals(m[1]).forEach((e) => out.add(e));
  }
  if (out.size === 0) {
    throw new Error(
      `no extension is readable from file_pattern ${JSON.stringify(filePattern)}. ` +
        "Reporting it as `admits nothing` is how V138 shipped; add the spelling to admittedExtensions."
    );
  }
  return out;
}

/// `ya?ml` -> `yml`, `yaml`. Anything left holding a non-alphanumeric is dropped, not guessed.
function expandOptionals(token) {
  const i = token.indexOf("?");
  if (i < 0) return /^[A-Za-z0-9]+$/.test(token) ? [token.toLowerCase()] : [];
  if (i === 0) return [];
  const head = token.slice(0, i - 1);
  const opt = token[i - 1];
  const rest = token.slice(i + 1);
  return [...expandOptionals(head + rest), ...expandOptionals(head + opt + rest)];
}

export function admits(filePattern, ext) {
  return admittedExtensions(filePattern).has(ext);
}

/// Every root whose packs this guard judges.
///
/// `examples/packs` was missing until 2026-09-12 (review ledger V166), and its absence was not a
/// shrug: `VERSIONING.md`'s surface list names those four files, so their rule ids are a frozen
/// surface and a user who copies `typescript-lint.json` inherits whatever extension gap it carries.
/// Eight other guards already read that directory; this one -- the only guard whose whole subject is
/// `file_pattern` -- did not.
///
/// The two roots have different SHAPES (`rules/dsl/<pack>/<pack>.json` is nested, `examples/packs/*.json`
/// is flat), which is why this walks for `.json` instead of assuming one directory depth. A shape
/// assumption is how the narrower version stayed narrow without anyone deciding it should be.
export const RULE_PACK_ROOTS = ["rules/dsl", "examples/packs"];

function collectJsonFiles(dir, out) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) collectJsonFiles(p, out);
    else if (e.name.endsWith(".json")) out.push(p);
  }
}

export function collectRules(roots = RULE_PACK_ROOTS) {
  const out = [];
  for (const root of typeof roots === "string" ? [roots] : roots) {
    const files = [];
    collectJsonFiles(root, files);
    for (const file of files.sort()) {
      let parsed;
      try {
        parsed = JSON.parse(fs.readFileSync(file, "utf8"));
      } catch (e) {
        throw new Error(`unparsable rule pack ${file}: ${e.message}`);
      }
      for (const r of Array.isArray(parsed) ? parsed : parsed.rules || [parsed]) {
        if (!r || !r.id) continue;
        const fp = (r.matcher && r.matcher.file_pattern) || r.file_pattern || null;
        out.push({ file, id: r.id, filePattern: fp });
      }
    }
  }
  return out;
}

export function findGaps(rules) {
  const gaps = [];
  for (const r of rules) {
    const skip = exceptionFor(r.id);
    for (const [base, variants] of Object.entries(SAME_SYNTAX_FAMILIES)) {
      if (!admits(r.filePattern, base)) continue;
      const missing = variants.filter((v) => !skip.includes(v) && !admits(r.filePattern, v));
      if (missing.length) gaps.push({ ...r, base, missing });
    }
  }
  return gaps;
}
