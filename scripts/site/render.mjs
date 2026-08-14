// Content -> HTML renderer.
//
// Why this exists: when a sentence is embedded in HTML twice, once per language,
// fixing one copy and not the other goes unnoticed forever. Sentences live in
// site-src/content/*.mjs as a PAIR, exactly once, and the output forks here:
//   en — English only  (site/)
//   ko — Korean only   (site/ko/)
// When a translation is missing it is not a guard that complains, it is the build
// that dies. That is the whole point of this structure.
//
// The third edition (bi), which hid and showed both languages inside one file, was
// removed on 2026-08-14: once the language switch became a LINK (the editions became
// separate pages) there was already a way to read them side by side, and three
// editions means three editions to maintain. Its traces (the two lang attributes,
// the [lang] hiding CSS, the toggle JS) went with it.
//
// ── the two shapes a sentence can take ───────────────────────────────────────
//   {ko: "…", en: "…"}   a pair. Both sides must be present.
//   "…"                  language-neutral (commands, ids, symbols). Hangul inside one
//                        is a build error — it means something that should have been a pair.
//
// Inline tags inside a string (<strong> <em> <code> <a> <br>) pass through as-is.
// Which word carries the emphasis differs per language, so it is part of the sentence —
// these are not escaped.

// Escapes, not literal characters. This file is OSS-facing source and CI's check-english-source.sh
// rejects non-Latin letters everywhere except the one exempt content path — a literal Hangul class
// here would make the file that ENFORCES the pair rule the first thing to violate the guard.
// U+AC00-U+D7A3 = Hangul syllables, U+3131-U+318E = compatibility jamo. Same set as before.
const HANGUL = /[\uAC00-\uD7A3\u3131-\u318E]/;

class Bad extends Error {}

/** Pick one sentence in this edition's language. If one side of the pair is empty, die here. */
function pick(t, mode, where) {
  if (t == null) throw new Bad(`${where}: empty sentence`);
  if (typeof t === "string") {
    if (HANGUL.test(t)) {
      throw new Bad(
        `${where}: Hangul in a language-neutral string — this has to be a {ko, en} pair\n    ${t.slice(0, 80)}`
      );
    }
    return t;
  }
  for (const k of ["ko", "en"]) {
    if (typeof t[k] !== "string" || !t[k].trim()) {
      throw new Bad(`${where}: '${k}' is empty — write only one side of the pair and the other edition loses that slot`);
    }
  }

  // Non-empty on both sides is not enough. Prose may diverge freely -- that is what "rewritten, not
  // translated" means -- but a <code> token is a NAME the reader retypes: a config key, a rule id, a
  // column. Fix such a name on one side only and the other edition ships a name that does not exist,
  // with nothing red: check-english-source only counts Hangul, and check-site-generated only proves
  // the pages match their source. So the code tokens, as a SET, must agree.
  // An angle-bracket placeholder is the one part of a token that IS prose -- `<reason>` and its Korean
  // twin are the same instruction, not two different names -- so its contents are blanked before the
  // comparison. Everything outside the brackets still has to match exactly.
  const tokens = (s) =>
    new Set(
      [...s.matchAll(/<code>([\s\S]*?)<\/code>/g)].map((m) => m[1].trim().replace(/&lt;[^&]*&gt;/g, "&lt;&gt;"))
    );
  const ko = tokens(t.ko);
  const en = tokens(t.en);
  const onlyKo = [...ko].filter((x) => !en.has(x));
  const onlyEn = [...en].filter((x) => !ko.has(x));
  if (onlyKo.length || onlyEn.length) {
    throw new Bad(
      `${where}: the two sides name different code tokens — one edition would ship a name the other does not\n` +
        (onlyKo.length ? `    ko only: ${onlyKo.join(" ")}\n` : "") +
        (onlyEn.length ? `    en only: ${onlyEn.join(" ")}\n` : "") +
        `    (prose may differ freely; a <code> token is a name the reader retypes)`
    );
  }

  return t[mode];
}

/** One block element. */
function block(tag, cls, t, mode, where, indent = "    ") {
  const c = cls ? ` class="${cls}"` : "";
  return `${indent}<${tag}${c}>${pick(t, mode, where)}</${tag}>`;
}

/** One inline sentence. */
const inline = (t, mode, where) => pick(t, mode, where);

/** One attribute value (placeholder, aria-label). Tags cannot go in here. */
function attrOf(t, mode, where) {
  const s = pick(t, mode, where);
  if (/[<>]/.test(s)) throw new Bad(`${where}: an attribute value cannot contain a tag`);
  return s.replace(/"/g, "&quot;");
}

function renderBlocks(blocks, mode, where, indent) {
  return blocks.map((b, i) => renderBlock(b, mode, `${where}[${i}]`, indent)).join("\n\n");
}

function renderBlock(b, mode, where, indent = "    ") {
  if (!Array.isArray(b)) throw new Bad(`${where}: a block has to be a [kind, …] array`);
  const [kind, a, opt] = b;
  const W = `${where} ${kind}`;

  switch (kind) {
    case "eyebrow":
      return block("span", "eyebrow", a, mode, W, indent);
    case "h1":
    case "h2":
    case "h3":
      return block(kind, null, a, mode, W, indent);
    case "p":
      return block("p", null, a, mode, W, indent);
    case "lede":
      return block("p", "lede", a, mode, W, indent);
    case "muted":
      return block("p", opt?.inner ? "muted inner" : "muted", a, mode, W, indent);
    case "note":
      return block("p", opt?.inner ? "note inner" : "note", a, mode, W, indent);

    case "group": {
      const cls = ["inner", "stack", a?.tight && "stack--tight", a?.loose && "stack--loose"]
        .filter(Boolean)
        .join(" ");
      return (
        `${indent}<div class="${cls}">\n` +
        renderBlocks(opt, mode, W, indent + "  ") +
        `\n${indent}</div>`
      );
    }

    case "lenses": {
      if (a.length !== 3) throw new Bad(`${W}: there have to be exactly three lenses (got ${a.length}) — the CSS is a 3-column grid`);
      const cells = a
        .map((l, i) =>
          `${indent}  <div class="lens">\n` +
          block("h3", null, l.h, mode, `${W}[${i}].h`, indent + "    ") + "\n" +
          block("p", null, l.p, mode, `${W}[${i}].p`, indent + "    ") + "\n" +
          `${indent}  </div>`
        )
        .join("\n");
      return `${indent}<div class="lenses">\n${cells}\n${indent}</div>`;
    }

    case "vs": {
      const rows = a
        .map((r, i) => {
          const k = inline(r.k, mode, `${W}[${i}].k`);
          const v = inline(r.v, mode, `${W}[${i}].v`);
          return (
            `${indent}  <div class="vs__row">\n` +
            `${indent}    <div class="vs__k">${k}</div>\n` +
            `${indent}    <div class="vs__v">${v}</div>\n` +
            `${indent}  </div>`
          );
        })
        .join("\n");
      return `${indent}<div class="vs${opt?.wide ? " vs--wide" : ""}">\n${rows}\n${indent}</div>`;
    }

    case "run": {
      const rows = a
        .map((r, i) => {
          const n = String(i + 1).padStart(2, "0");
          return (
            `${indent}  <div class="run__row">\n` +
            `${indent}    <span class="run__n">${n}</span>\n` +
            `${indent}    <code class="run__cmd">${r.cmd}</code>\n` +
            `${indent}    <span class="run__what">${inline(r.what, mode, `${W}[${i}].what`)}</span>\n` +
            `${indent}  </div>`
          );
        })
        .join("\n");
      return `${indent}<div class="run">\n${rows}\n${indent}</div>`;
    }

    case "panel": {
      const tab = block("div", "panel__tab", a.tab, mode, `${W}.tab`, indent + "  ");
      // Inside <pre>, whitespace IS the rendering — no indentation is added here.
      const lines = a.lines
        .map((l, i) => {
          if (typeof l === "string") {
            if (HANGUL.test(l)) throw new Bad(`${W}.lines[${i}]: Hangul in a code line — move it out into {comment}`);
            return l;
          }
          const code = l.code ?? "";
          if (HANGUL.test(code)) throw new Bad(`${W}.lines[${i}].code: Hangul on the code side`);
          // `mid` — a language-neutral fragment that sits BETWEEN the two languages (e.g. the
          // field-name run that both sentences point at). Push it into one language's string
          // and that whole line disappears from the other edition.
          let mid = "";
          if (l.mid !== undefined) {
            if (typeof l.mid !== "string") throw new Bad(`${W}.lines[${i}].mid: has to be a string`);
            if (HANGUL.test(l.mid)) throw new Bad(`${W}.lines[${i}].mid: Hangul — this has to be a language-neutral fragment`);
            mid = `<span class="c">${l.mid}</span>`;
          }

          // Where `mid` goes differs per edition: a Korean comment ENDS with the line break and an
          // English one BEGINS with it, so the line only lines up if mid comes after the comment in
          // the ko edition and before it in the en edition.
          const own = `<span class="c">${pick(l.comment, mode, `${W}.lines[${i}].comment`)}</span>`;
          return mode === "ko" ? `${code}${own}${mid}` : `${code}${mid}${own}`;
        })
        .join("\n");
      return `${indent}<div class="panel">\n${tab}\n<pre>${lines}</pre>\n${indent}</div>`;
    }

    // Dependency-graph stage — this emits the SLOT only. The pixels are drawn by the viewer that
    // lives in the original site/graph.html, and the generator (scripts/gen-site.mjs) slices that
    // viewer and its data out of the original at build time and appends them at the end of the page.
    //
    // The ids and classes below are a CONTRACT WITH THE VIEWER. One of them being off gives you a
    // silently blank stage, with no exception:
    //   #depgraph(canvas) · #gdetail · #gcensus · #glegend · #gq
    //   .graph-controls(the root the change events are delegated from) · [data-zoom](zoom buttons)
    //   .is-dragging(toggled by the viewer)
    // The radios are read by `name="col"` / `name="siz"` and by `value` — the labels are
    // translatable, the values are not.
    case "graph": {
      const I = (k) => inline(a[k], mode, `${W}.${k}`);
      const A = (k) => attrOf(a[k], mode, `${W}.${k}`);
      const seg = (name, opts) =>
        opts
          .map(
            (o) =>
              `${indent}        <label><input type="radio" name="${name}" value="${o.v}"` +
              `${o.on ? " checked" : ""}><span>${I(o.k)}</span></label>`
          )
          .join("\n");

      return (
        `${indent}<div class="graph-stage">\n` +
        `${indent}  <canvas id="depgraph" aria-label="${A("canvas")}"></canvas>\n` +
        `\n` +
        `${indent}  <div class="graph-controls">\n` +
        `${indent}    <fieldset>\n` +
        `${indent}      <legend>${I("colourBy")}</legend>\n` +
        `${indent}      <div class="seg">\n` +
        seg("col", [
          { v: "area", k: "byArea", on: true },
          { v: "degree", k: "byDegree" },
        ]) +
        `\n${indent}      </div>\n` +
        `${indent}    </fieldset>\n` +
        `${indent}    <fieldset>\n` +
        `${indent}      <legend>${I("sizeBy")}</legend>\n` +
        `${indent}      <div class="seg">\n` +
        seg("siz", [
          { v: "degree", k: "byDegree", on: true },
          { v: "fanIn", k: "byFanIn" },
          { v: "fanOut", k: "byFanOut" },
        ]) +
        `\n${indent}      </div>\n` +
        `${indent}    </fieldset>\n` +
        `${indent}    <label class="graph-find">\n` +
        `${indent}      <span>${I("find")}</span>\n` +
        `${indent}      <input id="gq" type="search" placeholder="${A("findHint")}"` +
        ` autocomplete="off" spellcheck="false">\n` +
        `${indent}    </label>\n` +
        // The viewer fills this — being empty here is correct.
        `${indent}    <div class="graph-detail" id="gdetail"></div>\n` +
        `${indent}  </div>\n` +
        `\n` +
        `${indent}  <div class="graph-zoom">\n` +
        `${indent}    <button type="button" data-zoom="in" aria-label="${A("zoomIn")}">+</button>\n` +
        `${indent}    <button type="button" data-zoom="out" aria-label="${A("zoomOut")}">−</button>\n` +
        `${indent}    <button type="button" data-zoom="reset" aria-label="${A("zoomReset")}">↻</button>\n` +
        `${indent}  </div>\n` +
        `\n` +
        `${indent}  <ol class="graph-legend" id="glegend"></ol>\n` +
        `${indent}</div>\n` +
        `\n` +
        `${indent}<p class="graph-cap"><span id="gcensus"></span>` +
        `<span class="graph-src">${I("caption")}</span></p>`
      );
    }

    // Escape hatch for LANGUAGE-NEUTRAL structure the schema cannot express. Hangul is rejected —
    // if translated sentences leak in here, the reason this structure exists is gone.
    case "raw": {
      if (typeof a !== "string") throw new Bad(`${W}: raw takes a single string`);
      if (HANGUL.test(a)) throw new Bad(`${W}: Hangul in raw — translated sentences have to be written as pairs`);
      return indent + a;
    }

    default:
      throw new Bad(`${W}: unknown block kind '${kind}'`);
  }
}

/** One page (its bands) as an HTML fragment. */
export function renderPage(page, mode, pageId) {
  if (!Array.isArray(page?.bands)) throw new Bad(`${pageId}: no bands array`);
  return page.bands
    .map((b, i) => {
      const where = `${pageId}.bands[${i}]`;
      const tag = b.header ? "header" : "section";
      const cls = ["band", b.tint && "band--tint"].filter(Boolean).join(" ");
      const innerCls = ["inner", b.wide && "inner--wide", "stack", b.loose && "stack--loose"]
        .filter(Boolean)
        .join(" ");
      return (
        `<${tag} class="${cls}">\n` +
        `  <div class="${innerCls}">\n` +
        renderBlocks(b.blocks, mode, where, "    ") +
        `\n  </div>\n` +
        `</${tag}>`
      );
    })
    .join("\n\n");
}

// HANGUL is exported so the generator can run the same test on its finished output. One owner for
// the character class: a second copy over there would be a second thing to keep in sync.
export { Bad, HANGUL };
