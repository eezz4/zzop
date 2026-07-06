/*
 * zzop docs — vanilla JS, no dependencies.
 * (a) right-hand TOC scroll-spy via IntersectionObserver
 * (b) mobile toggle for the left docs nav sidebar
 * No-ops entirely on pages that lack these elements (e.g. the home page).
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

  function init() {
    setupScrollSpy();
    setupMobileNavToggle();
    setupSortableTables();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
