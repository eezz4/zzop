# zpz docs site

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

`.github/workflows/pages.yml` uploads this directory to GitHub Pages on every
push to `main` that touches `site/`. One-time setup after the repo goes public:
Settings → Pages → Source → "GitHub Actions".

## Editing

- Pages: `index.html`, `usage.html`, `architecture.html`, `rules.html`
- Shared styles/behavior: `assets/site.css`, `assets/docs.js`
- The rule catalog page mirrors `docs/rules/catalog.md` (the machine-checked
  source of truth) — update it from there, not ad hoc.
