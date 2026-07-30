---
description: Report whether a newer zzop-mcp release exists, and how to take it. Never installs anything.
argument-hint: (no arguments)
---

# Check for a newer zzop-mcp, and report only

Tell the user which `zzop-mcp` they have, whether a newer release exists, and — if it does — exactly
what to do about it. **Do not act on the answer.** Not with this command, not as a follow-up, not
even if the user seems to want it: taking the update is a decision about *when*, and the reasons are
below.

## Why this never installs

A zzop version change is not a patch to a tool that produces the same answer faster. It changes what
the analyzer *finds*, and it invalidates the analysis cache — every cache fingerprint is derived from
the source that produced the cached bytes, so a new binary means a full cold run. An update that
happened without being asked for would change findings between two sessions with no change in the
user's code, which is the exact opposite of the determinism this tool sells. The timing belongs to
whoever is going to read the diff in the findings.

The plugin's `SessionStart` hook follows the same rule: it installs when nothing is installed, and
after that it only ever reports. This command is the on-demand half of the same contract.

## Steps

1. **Find the installed binary.** It lives at `$CLAUDE_PLUGIN_DATA/zzop-mcp` (`zzop-mcp.exe` on
   Windows). That location is decided by `.claude-plugin/hooks/bootstrap.sh` — read it rather than
   assuming, if the path does not resolve.
2. **Read the installed version** with `zzop-mcp version`. If the binary is missing or cannot report
   a version, say so and stop: the fix is a new session (the hook installs on `SessionStart`), not
   anything this command should do.
3. **Read the latest published version**:
   `curl -fsSL --max-time 10 https://api.github.com/repos/eezz4/zzop/releases/latest` and take
   `tag_name`. If the request fails, say the check could not run — offline, proxied, or rate-limited
   — and do not guess from any other source.
4. **Report the comparison.** Same version: one line saying it is current. Newer available: name both
   versions, then give the two ways to take it:
   - delete the binary at `$CLAUDE_PLUGIN_DATA/zzop-mcp` and start a new session — the hook installs
     the current release, or
   - download it from `https://github.com/eezz4/zzop/releases` and put it at that path yourself.

   Add that the first run after an update is a cold one, because the cache generation is retired by
   the version change.

## What not to do

- Do not download, delete, move, or replace the binary. Reporting is the entire job.
- Do not read the version from a manifest in a checked-out copy of the zzop repo. That is the version
  of the *source tree*, not of the binary the user's sessions actually run — and confusing the two is
  the failure that made the install hook necessary in the first place.
- Do not report a version comparison you could not complete. "Could not check" is an honest answer;
  a guess is the kind of confident-and-wrong output this project treats as a defect.
