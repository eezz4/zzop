# zzop docs site

Plain static HTML — no dependencies, no external requests, nothing fetched at
runtime. What is served is exactly what is committed here, so GitHub Pages does
no work beyond copying the directory.

It is not, however, hand-written end to end: four of these pages carry generated
content, and one of the generators has to run before a `site-src/` edit reaches a
page. See [Generated pages](#generated-pages) — that is the section to read
before editing anything.

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

- Pages:

  ```
  find site -name '*.html' | sort
  ```

  is the list, and the command is written out rather than its answer — the
  hand-written copy that used to live on this line named five pages and missed
  `graph.html` and `privacy.html`, and its replacement (`ls site/*.html`) then
  missed `site/ko/index.html`, because the shell glob does not descend. Counting
  by hand is the drift this repo keeps paying for; `find` is what descends.
- Two of the pages are **redirect stubs**, not content: `architecture.html` and
  `usage.html` are now tabs of `index.html`, and the files stay only to absorb
  links that already point at them. Each says so in a comment at the top.
- Shared styles/behavior: `assets/site.css`, `assets/docs.js` — for the
  hand-written pages. `index.html` and `ko/index.html` do **not** use them: they
  inline their own stylesheet from `site-src/site-v2.css`, so that the same bytes
  render as a standalone artifact preview.

## Generated pages

Four pages carry content no one should edit in place. Each names the command that
rewrites it; each is committed, and CI compares the committed bytes against a
fresh run, so an edit here without a re-run is a red guard rather than a stale
site.

| Page | Generated from | Command |
| --- | --- | --- |
| `index.html`, `ko/index.html` | `site-src/content/*.mjs` (`{ko, en}` sentence pairs) — the whole page, both editions | `node scripts/gen-site.mjs` |
| `rules.html` | `docs/rules/catalog.md` — the table `<tbody>` rows and the TOC; the prose around them is hand-written | `node scripts/gen-site-rules.mjs` |
| `graph.html` | this repo's own import graph — the data block between the `zzop:dep-data` markers, plus every published count that describes it; the viewer is hand-written | `node scripts/site-graph-data.mjs <nodes.ndjson> <links.ndjson>` (the script's header gives the two `zzop graph` commands that produce its input) |

`index.html`/`ko/index.html` are the pair to be careful with: **no sentence on
them lives in this directory.** Editing the HTML directly is thrown away by the
next generator run, and `scripts/check-site-generated.sh` fails on the commit that
does it. Edit `site-src/content/*.mjs`, then re-run `gen-site.mjs` and commit both
sides together. `graph.html` supplies those two pages with their graph data — it
is sliced out at build time, never copied — so regenerating the graph means
re-running `gen-site.mjs` afterwards as well.
