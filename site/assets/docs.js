/*
 * zzop docs — vanilla JS, no dependencies.
 * (a) right-hand TOC scroll-spy via IntersectionObserver
 * (b) mobile toggle for the left docs nav sidebar
 * (c) click-to-sort on every docs table column
 * (d) a two-line clamp on the long prose cells inside those tables
 * (e) one row filter per page, spanning every table on it
 * Each no-ops entirely on a page that lacks its elements (e.g. the home page),
 * so every behaviour here is additive and none of them is load-bearing for
 * reading the page.
 */
(function () {
  "use strict";

  function setupScrollSpy() {
    var content = document.querySelector(".docs-content");
    var toc = document.querySelector(".docs-toc");
    if (!content || !toc) return;

    var sections = Array.prototype.slice.call(content.querySelectorAll("section[id]"));
    var links = Array.prototype.slice.call(toc.querySelectorAll(".docs-toc__link"));
    if (!sections.length || !links.length) return;

    var linkById = {};
    links.forEach(function (link) {
      var id = (link.getAttribute("href") || "").replace(/^#/, "");
      if (id) linkById[id] = link;
    });

    function setActive(id) {
      links.forEach(function (link) {
        link.classList.remove("is-active");
      });
      var active = linkById[id];
      if (active) active.classList.add("is-active");
    }

    if (!("IntersectionObserver" in window)) return;

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) setActive(entry.target.id);
        });
      },
      { rootMargin: "-96px 0px -70% 0px", threshold: 0 }
    );

    sections.forEach(function (section) {
      observer.observe(section);
    });
  }

  function setupMobileNavToggle() {
    var toggle = document.querySelector(".docs-nav-toggle");
    var nav = document.querySelector(".docs-nav");
    if (!toggle || !nav) return;

    toggle.addEventListener("click", function () {
      var open = nav.classList.toggle("is-open");
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
    });
  }

  /*
   * (c) click-to-sort on every docs table column. The Severity column sorts by
   * rank (critical < warning < info), not alphabetically; numeric columns sort
   * numerically; everything else case-insensitive text. Tables that have a
   * Severity column default to critical-first on load.
   */
  function setupSortableTables() {
    var SEV = { critical: 0, warning: 1, info: 2 };

    function cellText(row, i) {
      var c = row.cells[i];
      return c ? c.textContent.trim() : "";
    }

    Array.prototype.forEach.call(document.querySelectorAll(".docs-content table"), function (table) {
      var head = table.tHead;
      var body = table.tBodies[0];
      if (!head || !body || !head.rows.length || !body.rows.length) return;
      var headerCells = head.rows[0].cells;

      function isSeverityCol(i) {
        if (headerCells[i].textContent.trim().toLowerCase() === "severity") return true;
        var rows = body.rows, seen = 0, ok = 0;
        for (var r = 0; r < rows.length; r++) {
          var t = cellText(rows[r], i).toLowerCase();
          if (!t) continue;
          seen++;
          if (Object.prototype.hasOwnProperty.call(SEV, t)) ok++;
        }
        return seen > 0 && ok === seen;
      }

      function comparator(i) {
        var sev = isSeverityCol(i);
        var rows = body.rows, allNum = !sev;
        for (var r = 0; allNum && r < rows.length; r++) {
          var t = cellText(rows[r], i);
          if (t && isNaN(parseFloat(t))) allNum = false;
        }
        return function (a, b) {
          var ta = cellText(a, i), tb = cellText(b, i);
          if (sev) {
            var ra = Object.prototype.hasOwnProperty.call(SEV, ta.toLowerCase()) ? SEV[ta.toLowerCase()] : 99;
            var rb = Object.prototype.hasOwnProperty.call(SEV, tb.toLowerCase()) ? SEV[tb.toLowerCase()] : 99;
            return ra - rb;
          }
          if (allNum) return (parseFloat(ta) || 0) - (parseFloat(tb) || 0);
          if (!ta) return 1;
          if (!tb) return -1;
          return ta.toLowerCase().localeCompare(tb.toLowerCase());
        };
      }

      function sortBy(i, dir) {
        var cmp = comparator(i);
        var rows = Array.prototype.slice.call(body.rows);
        rows.sort(function (a, b) { return dir === "desc" ? -cmp(a, b) : cmp(a, b); });
        rows.forEach(function (r) { body.appendChild(r); });
        Array.prototype.forEach.call(headerCells, function (h) { h.removeAttribute("aria-sort"); });
        headerCells[i].setAttribute("aria-sort", dir === "desc" ? "descending" : "ascending");
      }

      var sevCol = -1;
      Array.prototype.forEach.call(headerCells, function (th, i) {
        if (sevCol === -1 && isSeverityCol(i)) sevCol = i;
        th.classList.add("is-sortable");
        th.setAttribute("role", "button");
        th.setAttribute("tabindex", "0");
        function onSort() {
          sortBy(i, th.getAttribute("aria-sort") === "ascending" ? "desc" : "asc");
        }
        th.addEventListener("click", onSort);
        th.addEventListener("keydown", function (e) {
          if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSort(); }
        });
      });

      if (sevCol !== -1) sortBy(sevCol, "asc");
    });
  }

  /*
   * (d) long prose cells clamp to a two-line lead.
   *
   * This is the complaint itself, not the table length: `Detects` holds
   * multi-sentence paragraphs (over 700 characters on some rules), so twenty
   * rows read as one undifferentiated block. Hiding the whole table behind a
   * fold made the page shorter without making a single row easier to read.
   *
   * WHICH cells clamp is MEASURED, never declared. Every cell is clamped, the
   * page is asked which ones actually overflow two lines, and the rest drop the
   * class in the same pass. A "long columns" list would be a fourth owner of
   * what three generators emit, and it would be wrong the first time a table
   * gained a column.
   */
  function setupCellClamps(content) {
    var cells = Array.prototype.slice.call(content.querySelectorAll("table tbody td"));
    if (!cells.length) return [];

    var candidates = cells.map(function (td) {
      /* The children move into a span so the clamp applies to the TEXT box: a
         `td` is a table-cell and cannot itself be the `-webkit-box` the clamp
         needs. Moving nodes rather than reassigning innerHTML keeps the <code>
         and <a> elements the generators emitted, identity and all. */
      var text = document.createElement("span");
      text.className = "cell-clamp__text";
      while (td.firstChild) text.appendChild(td.firstChild);
      td.appendChild(text);
      td.classList.add("cell-clamp");
      return { td: td, text: text, search: td.textContent.toLowerCase().replace(/\s+/g, " ") };
    });

    /* Every write above, then every read here: one forced layout for the whole
       page instead of one per cell. */
    var overflowing = candidates.filter(function (c) {
      return c.text.scrollHeight > c.text.clientHeight + 1;
    });

    candidates.forEach(function (c) {
      if (overflowing.indexOf(c) === -1) c.td.classList.remove("cell-clamp");
    });

    return overflowing.map(function (c) {
      var td = c.td;

      var more = document.createElement("button");
      more.type = "button";
      more.className = "cell-clamp__more";
      td.appendChild(more);

      function setExpanded(on) {
        td.classList.toggle("is-expanded", on);
        more.setAttribute("aria-expanded", on ? "true" : "false");
        /* The label is an attribute, not a child text node — see the CSS note:
           anything readable here would be matched by the row filter. */
        more.setAttribute("aria-label", on ? "Collapse this description" : "Show the full description");
      }
      setExpanded(false);

      more.addEventListener("click", function () {
        setExpanded(!td.classList.contains("is-expanded"));
      });

      /* The lead text toggles too, because a two-line paragraph that ends mid
         sentence is its own invitation to click. Guarded on the selection: a
         reader dragging across the text wants the text, not a state change. */
      c.text.addEventListener("click", function () {
        var sel = window.getSelection && window.getSelection();
        if (sel && !sel.isCollapsed) return;
        setExpanded(!td.classList.contains("is-expanded"));
      });

      return { search: c.search, setExpanded: setExpanded };
    });
  }

  /*
   * (e) one row filter per page, over every table on it.
   *
   * Built HERE rather than in the markup on purpose. The pages that carry tables
   * are GENERATED (gen-site-rules.mjs, site-graph-data.mjs), so emitting the
   * control from each generator would put the same widget in three places and
   * let them drift; and check-rules-catalog-sync.sh compares the generated rule
   * rows against the catalog, which a markup change would disturb for no gain.
   * One implementation, every page, no generator touched.
   */
  function setupTableSearch(content, clamps) {
    var tables = Array.prototype.slice
      .call(content.querySelectorAll(".table-scroll"))
      .map(function (scroll) {
        var table = scroll.querySelector("table");
        return { scroll: scroll, body: table && table.tBodies[0] };
      })
      .filter(function (t) { return t.body && t.body.rows.length; });
    if (!tables.length) return;

    var header = content.querySelector(".docs-content-header") || content;

    var totalRows = tables.reduce(function (n, t) { return n + t.body.rows.length; }, 0);

    var box = document.createElement("div");
    box.className = "table-search";

    var input = document.createElement("input");
    input.type = "search";
    input.className = "table-search__input";
    input.placeholder = "Filter " + totalRows + " rows on this page";
    input.setAttribute("aria-label", "Filter table rows on this page");

    var status = document.createElement("span");
    status.className = "table-search__status";
    /* Announced politely so a screen reader hears the new count without losing
       the caret — the reader is still typing. */
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");

    box.appendChild(input);
    box.appendChild(status);
    header.appendChild(box);

    /* Row text is cached on the first search rather than read per keystroke:
       332 rows x every character typed is a lot of layout-thrashing reads for a
       string that cannot change. */
    var cache = null;

    function build() {
      cache = tables.map(function (t) {
        return Array.prototype.map.call(t.body.rows, function (r) {
          return r.textContent.toLowerCase().replace(/\s+/g, " ");
        });
      });
    }

    function apply() {
      var q = input.value.trim().toLowerCase();
      if (!cache) build();

      /* A match can sit in the clamped half of a cell, where the reader would
         see a surviving row and no reason for it. So the query decides the
         clamp too: cells that hold it open, the rest close. */
      clamps.forEach(function (c) {
        c.setExpanded(q !== "" && c.search.indexOf(q) !== -1);
      });

      if (!q) {
        tables.forEach(function (t) {
          Array.prototype.forEach.call(t.body.rows, function (r) {
            r.classList.remove("is-filtered-out");
          });
          t.scroll.classList.remove("is-empty");
        });
        status.textContent = "";
        status.classList.remove("is-zero");
        return;
      }

      var shownTotal = 0;
      tables.forEach(function (t, i) {
        var texts = cache[i];
        var shown = 0;
        Array.prototype.forEach.call(t.body.rows, function (r, ri) {
          var hit = texts[ri].indexOf(q) !== -1;
          r.classList.toggle("is-filtered-out", !hit);
          if (hit) shown++;
        });
        shownTotal += shown;
        /* A table the query excludes entirely leaves the page rather than
           sitting there as a header row the reader has to rule out by hand.
           Its HEADING stays, so the section it belongs to is still legible as
           "nothing here". */
        t.scroll.classList.toggle("is-empty", shown === 0);
      });

      status.textContent = shownTotal === 0
        ? "no rows match"
        : shownTotal + " of " + totalRows + " rows";
      status.classList.toggle("is-zero", shownTotal === 0);
    }

    input.addEventListener("input", apply);
    input.addEventListener("search", apply);
    input.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && input.value) { input.value = ""; apply(); }
    });
  }

  function init() {
    setupScrollSpy();
    setupMobileNavToggle();
    setupSortableTables();

    var content = document.querySelector(".docs-content");
    if (!content) return;
    /* Clamps are measured FIRST, while every row is still on screen: a cell in a
       filtered-out row has no height, so the measurement would read zero. */
    var clamps = setupCellClamps(content);
    setupTableSearch(content, clamps);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
