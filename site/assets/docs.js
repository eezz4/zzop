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

  function init() {
    setupScrollSpy();
    setupMobileNavToggle();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
