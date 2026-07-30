# zzop docs site

Plain static HTML — no build step, no dependencies, no external requests.

## Preview locally

Opening `index.html` directly in a browser works (the site is `file://`-safe).
For a URL-faithful preview (same relative paths GitHub Pages serves), run any
static file server from the repo root:

```
npx serve site
# or
python -m http.server 8080 --directory site
```

## Deploy

`.github/workflows/pages.yml` uploads this directory to GitHub Pages after the
`ci` workflow finishes **green** on `main` — not straight off the push. The gate
exists because the guards that validate this directory (catalog↔site sync, docs
link graph, overclaim prose, english-source, rule-id and io-key vocabulary) run
in `ci`, and until 2026-07-28 the site deployed beside them rather than behind
them. `workflow_run` takes no `paths` filter, so this fires after every green
main CI, not only site-touching ones; re-uploading identical bytes is idempotent,
so the price is a runner. That workflow's own header states the trade in full —
it is the source of truth for the trigger, not this paragraph.

One-time setup after the repo goes public: Settings → Pages → Source → "GitHub
Actions".

## Editing

- Pages: `ls site/*.html` is the list. It is not repeated here — the hand-written
  copy that used to live on this line named five pages and missed `graph.html`
  and `privacy.html`, which is the drift this repo keeps paying for.
- Shared styles/behavior: `assets/site.css`, `assets/docs.js`. Every page carries
  the same header markup by hand, so a nav change is a change to all of them.
- `graph.html` is the one page with generated content: the data block between the
  `zzop:dep-data` markers, and every published count that describes it, are
  written by `scripts/site-graph-data.mjs`. Edit the viewer freely, never the
  block. CI fails if the committed output differs from what a regeneration
  produces, so a change to the dep graph means re-running that script.
- The rule catalog page mirrors `docs/rules/catalog.md` (the machine-checked
  source of truth) — update it from there, not ad hoc.
