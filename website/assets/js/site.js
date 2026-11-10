/* ==========================================================================
   Megi website — behaviour
   Theme + language preferences, navigation, scroll effects, reveals,
   animated counters, copy buttons, gallery lightbox and FAQ accordions.
   No dependencies. Progressive enhancement: the page works without JS.
   ========================================================================== */
(function () {
  "use strict";

  var root = document.documentElement;
  var prefersReducedMotion =
    window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  var META = {
    es: {
      title: "Megi — Un navegador web independiente, escrito desde cero en Rust",
      description:
        "Megi es un navegador independiente escrito desde cero en Rust. Open source, verificado contra los estándares de la web y en desarrollo activo. Descubre el proyecto, su estado y cómo ayudar."
    },
    en: {
      title: "Megi — An independent web browser built from scratch in Rust",
      description:
        "Megi is an independent web browser written from scratch in Rust. Open source, verified against web standards and under active development. Discover the project, its status and how to help."
    }
  };

  var LABELS = {
    es: {
      themeLight: "Cambiar a tema oscuro",
      themeDark: "Cambiar a tema claro",
      navOpen: "Abrir menú",
      navClose: "Cerrar menú",
      copy: "Copiar comando",
      copied: "¡Copiado!",
      closeLightbox: "Cerrar imagen ampliada"
    },
    en: {
      themeLight: "Switch to dark theme",
      themeDark: "Switch to light theme",
      navOpen: "Open menu",
      navClose: "Close menu",
      copy: "Copy command",
      copied: "Copied!",
      closeLightbox: "Close enlarged image"
    }
  };

  function currentLang() {
    return root.getAttribute("data-lang") === "en" ? "en" : "es";
  }

  /* ------------------------------------------------------------- THEME --- */
  function themeColor() {
    return root.getAttribute("data-theme") === "dark" ? "#070a12" : "#f6f7fb";
  }

  function syncThemeMeta() {
    var metas = document.querySelectorAll('meta[name="theme-color"]');
    for (var i = 0; i < metas.length; i++) {
      metas[i].setAttribute("content", themeColor());
      metas[i].removeAttribute("media");
    }
  }

  function applyTheme(theme, persist) {
    root.setAttribute("data-theme", theme);
    var toggle = document.getElementById("theme-toggle");
    if (toggle) {
      toggle.setAttribute("aria-pressed", theme === "dark" ? "true" : "false");
      toggle.setAttribute("aria-label", LABELS[currentLang()][theme === "dark" ? "themeDark" : "themeLight"]);
    }
    syncThemeMeta();
    if (persist) {
      try {
        localStorage.setItem("megi-theme", theme);
      } catch (e) {
        /* storage unavailable: theme still applies for this page view */
      }
    }
  }

  function initTheme() {
    var toggle = document.getElementById("theme-toggle");
    var stored = null;
    try {
      stored = localStorage.getItem("megi-theme");
    } catch (e) {}

    var systemQuery = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;
    var theme = root.getAttribute("data-theme") || (systemQuery && systemQuery.matches ? "dark" : "light");
    applyTheme(theme, false);

    if (toggle) {
      toggle.addEventListener("click", function () {
        var next = root.getAttribute("data-theme") === "dark" ? "light" : "dark";
        applyTheme(next, true);
      });
    }

    if (systemQuery) {
      var onSystemChange = function (event) {
        var explicit = null;
        try {
          explicit = localStorage.getItem("megi-theme");
        } catch (e) {}
        if (explicit !== "light" && explicit !== "dark") {
          applyTheme(event.matches ? "dark" : "light", false);
        }
      };
      if (systemQuery.addEventListener) {
        systemQuery.addEventListener("change", onSystemChange);
      } else if (systemQuery.addListener) {
        systemQuery.addListener(onSystemChange);
      }
    }
  }

  /* ---------------------------------------------------------- LANGUAGE --- */
  function syncMeta(lang) {
    var meta = META[lang];
    document.title = meta.title;
    var description = document.querySelector('meta[name="description"]');
    if (description) description.setAttribute("content", meta.description);

    var pairs = [
      ['meta[property="og:title"]', meta.title],
      ['meta[property="og:description"]', meta.description],
      ['meta[name="twitter:title"]', meta.title],
      ['meta[name="twitter:description"]', meta.description]
    ];
    for (var i = 0; i < pairs.length; i++) {
      var el = document.querySelector(pairs[i][0]);
      if (el) el.setAttribute("content", pairs[i][1]);
    }
  }

  function syncLabels(lang) {
    var labels = LABELS[lang];
    var theme = root.getAttribute("data-theme") === "dark" ? "dark" : "light";
    var themeToggle = document.getElementById("theme-toggle");
    if (themeToggle) {
      themeToggle.setAttribute("aria-label", labels[theme === "dark" ? "themeDark" : "themeLight"]);
    }
    var navToggle = document.getElementById("nav-toggle");
    if (navToggle) {
      navToggle.setAttribute("aria-label", navToggle.getAttribute("aria-expanded") === "true" ? labels.navClose : labels.navOpen);
    }
    var copyButtons = document.querySelectorAll("[data-copy]");
    for (var i = 0; i < copyButtons.length; i++) {
      if (!copyButtons[i].classList.contains("is-copied")) {
        copyButtons[i].setAttribute("aria-label", labels.copy);
      }
    }
    var closeLightbox = document.querySelector("[data-close-lightbox]");
    if (closeLightbox) closeLightbox.setAttribute("aria-label", labels.closeLightbox);
    var i18nAria = document.querySelectorAll("[data-aria-es][data-aria-en]");
    for (var j = 0; j < i18nAria.length; j++) {
      var value = i18nAria[j].getAttribute("data-aria-" + lang);
      if (value) i18nAria[j].setAttribute("aria-label", value);
    }
    var i18nAlt = document.querySelectorAll("[data-alt-es][data-alt-en]");
    for (var k = 0; k < i18nAlt.length; k++) {
      var altText = i18nAlt[k].getAttribute("data-alt-" + lang);
      if (altText) i18nAlt[k].setAttribute("alt", altText);
    }
  }

  function applyLang(lang, persist) {
    root.setAttribute("data-lang", lang);
    root.setAttribute("lang", lang);
    var buttons = document.querySelectorAll("[data-set-lang]");
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].setAttribute("aria-pressed", buttons[i].getAttribute("data-set-lang") === lang ? "true" : "false");
    }
    syncMeta(lang);
    syncLabels(lang);
    if (persist) {
      try {
        localStorage.setItem("megi-lang", lang);
      } catch (e) {}
    }
  }

  function initLanguage() {
    var buttons = document.querySelectorAll("[data-set-lang]");
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].addEventListener("click", function () {
        applyLang(this.getAttribute("data-set-lang"), true);
      });
    }
    /* Default to the Spanish copy, but respect stored and URL choices. */
    var lang = currentLang();
    var params = null;
    try {
      params = new URLSearchParams(window.location.search);
    } catch (e) {}
    var fromUrl = params ? params.get("lang") : null;
    if (fromUrl === "es" || fromUrl === "en") {
      lang = fromUrl;
    } else {
      try {
        var stored = localStorage.getItem("megi-lang");
        if (stored === "es" || stored === "en") lang = stored;
      } catch (e) {}
    }
    applyLang(lang, false);
  }

  /* ------------------------------------------------------------ HEADER --- */
  function initScrollEffects() {
    var header = document.querySelector(".site-header");
    var bar = document.getElementById("scroll-progress-bar");
    var ticking = false;

    function update() {
      ticking = false;
      var y = window.scrollY || window.pageYOffset || 0;
      if (header) header.classList.toggle("is-scrolled", y > 8);
      if (bar) {
        var doc = document.documentElement;
        var max = (doc.scrollHeight || 0) - window.innerHeight;
        var ratio = max > 0 ? Math.min(1, Math.max(0, y / max)) : 0;
        bar.style.width = (ratio * 100).toFixed(2) + "%";
      }
    }

    function onScroll() {
      if (!ticking) {
        ticking = true;
        window.requestAnimationFrame(update);
      }
    }

    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll, { passive: true });
    update();
  }

  /* -------------------------------------------------------------- NAV --- */
  function initNav() {
    var toggle = document.getElementById("nav-toggle");
    var menu = document.getElementById("nav-menu");
    if (!toggle || !menu) return;

    function setOpen(open) {
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
      toggle.setAttribute("aria-label", LABELS[currentLang()][open ? "navClose" : "navOpen"]);
      menu.classList.toggle("is-open", open);
      document.body.classList.toggle("nav-open", open);
    }

    toggle.addEventListener("click", function () {
      setOpen(toggle.getAttribute("aria-expanded") !== "true");
    });

    menu.addEventListener("click", function (event) {
      var link = event.target.closest ? event.target.closest("a") : null;
      if (link && menu.contains(link)) setOpen(false);
    });

    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && menu.classList.contains("is-open")) {
        setOpen(false);
        toggle.focus();
      }
    });

    window.addEventListener("resize", function () {
      if (window.innerWidth > 1120 && menu.classList.contains("is-open")) setOpen(false);
    });
  }

  /* --------------------------------------------------------- ACTIVE NAV --- */
  function initActiveSection() {
    var links = document.querySelectorAll(".nav-links a[href^='#']");
    if (!links.length || !("IntersectionObserver" in window)) return;

    var byId = {};
    for (var i = 0; i < links.length; i++) {
      var href = links[i].getAttribute("href");
      byId[href.slice(1)] = links[i];
    }

    var sections = [];
    Object.keys(byId).forEach(function (id) {
      var section = document.getElementById(id);
      if (section) sections.push(section);
    });

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return;
          for (var i = 0; i < links.length; i++) {
            links[i].classList.remove("is-active");
            links[i].removeAttribute("aria-current");
          }
          var link = byId[entry.target.id];
          if (link) {
            link.classList.add("is-active");
            link.setAttribute("aria-current", "location");
          }
        });
      },
      { rootMargin: "-38% 0px -55% 0px", threshold: 0 }
    );

    sections.forEach(function (section) {
      observer.observe(section);
    });
  }

  /* ------------------------------------------------------------ REVEAL --- */
  function initReveal() {
    var items = document.querySelectorAll(".reveal");
    if (!items.length) return;

    if (!("IntersectionObserver" in window) || prefersReducedMotion) {
      for (var i = 0; i < items.length; i++) items[i].classList.add("is-visible");
      return;
    }

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return;
          var el = entry.target;
          if (!el.style.getPropertyValue("--reveal-delay")) {
            var siblings = Array.prototype.filter.call(
              el.parentNode.children,
              function (child) {
                return child.classList && child.classList.contains("reveal");
              }
            );
            var index = siblings.indexOf(el);
            el.style.setProperty("--reveal-delay", Math.min(index, 5) * 65 + "ms");
          }
          el.classList.add("is-visible");
          observer.unobserve(el);
        });
      },
      { threshold: 0.1, rootMargin: "0px 0px -40px 0px" }
    );

    for (var j = 0; j < items.length; j++) observer.observe(items[j]);
  }

  /* ----------------------------------------------------------- COUNTERS --- */
  function animateCount(el) {
    var target = parseInt(el.getAttribute("data-count"), 10);
    var suffix = el.getAttribute("data-suffix") || "";
    if (isNaN(target)) return;
    if (prefersReducedMotion || target === 0) {
      el.textContent = target + suffix;
      return;
    }
    var duration = 1200;
    var start = null;
    function tick(timestamp) {
      if (start === null) start = timestamp;
      var progress = Math.min(1, (timestamp - start) / duration);
      var eased = 1 - Math.pow(1 - progress, 3);
      el.textContent = Math.round(target * eased) + suffix;
      if (progress < 1) window.requestAnimationFrame(tick);
    }
    window.requestAnimationFrame(tick);
  }

  function initCounters() {
    var counters = document.querySelectorAll("[data-count]");
    if (!counters.length) return;

    if (!("IntersectionObserver" in window)) {
      for (var i = 0; i < counters.length; i++) animateCount(counters[i]);
      return;
    }

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return;
          animateCount(entry.target);
          observer.unobserve(entry.target);
        });
      },
      { threshold: 0.6 }
    );

    for (var j = 0; j < counters.length; j++) observer.observe(counters[j]);
  }

  /* -------------------------------------------------------------- COPY --- */
  function copyText(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text);
    }
    return new Promise(function (resolve, reject) {
      try {
        var area = document.createElement("textarea");
        area.value = text;
        area.setAttribute("readonly", "");
        area.style.position = "fixed";
        area.style.opacity = "0";
        document.body.appendChild(area);
        area.select();
        document.execCommand("copy");
        document.body.removeChild(area);
        resolve();
      } catch (error) {
        reject(error);
      }
    });
  }

  function initCopyButtons() {
    var buttons = document.querySelectorAll("[data-copy]");
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].addEventListener("click", function () {
        var button = this;
        var block = button.closest(".code-block");
        var code = block ? block.querySelector("pre code") : null;
        if (!code) return;
        copyText(code.textContent.replace(/\s+$/, "")).then(
          function () {
            var lang = currentLang();
            button.classList.add("is-copied");
            button.setAttribute("aria-label", LABELS[lang].copied);
            window.setTimeout(function () {
              button.classList.remove("is-copied");
              button.setAttribute("aria-label", LABELS[currentLang()].copy);
            }, 2000);
          },
          function () {
            /* Clipboard blocked: leave the button unchanged. */
          }
        );
      });
    }
  }

  /* ---------------------------------------------------------- LIGHTBOX --- */
  function initLightbox() {
    var dialog = document.getElementById("lightbox");
    var image = document.getElementById("lightbox-img");
    var caption = document.getElementById("lightbox-caption");
    var triggers = document.querySelectorAll("[data-lightbox]");
    if (!dialog || !image || !caption || !triggers.length || !dialog.showModal) return;

    var lastTrigger = null;

    function open(trigger) {
      lastTrigger = trigger;
      var lang = currentLang();
      var full = trigger.getAttribute("data-lightbox");
      var source = trigger.querySelector("img");
      image.src = full;
      image.alt = source ? source.alt : "";
      caption.textContent =
        trigger.getAttribute("data-caption-" + lang) ||
        trigger.getAttribute("data-caption-es") ||
        "";
      document.body.classList.add("nav-open");
      dialog.showModal();
    }

    function close() {
      if (dialog.open) dialog.close();
    }

    for (var i = 0; i < triggers.length; i++) {
      triggers[i].addEventListener("click", function () {
        open(this);
      });
    }

    var closeButton = dialog.querySelector("[data-close-lightbox]");
    if (closeButton) closeButton.addEventListener("click", close);

    dialog.addEventListener("click", function (event) {
      if (event.target === dialog) close();
    });

    dialog.addEventListener("close", function () {
      document.body.classList.remove("nav-open");
      image.setAttribute("src", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=");
      if (lastTrigger && lastTrigger.focus) lastTrigger.focus();
    });
  }

  /* --------------------------------------------------------------- FAQ --- */
  function initFaq() {
    var items = document.querySelectorAll(".faq");
    for (var i = 0; i < items.length; i++) {
      items[i].addEventListener("toggle", function () {
        if (!this.open) return;
        for (var j = 0; j < items.length; j++) {
          if (items[j] !== this) items[j].open = false;
        }
      });
    }
  }

  /* -------------------------------------------------------------- BOOT --- */
  function boot() {
    try {
      initTheme();
    } catch (e) {
      /* keep the page usable even if a feature fails */
    }
    try {
      initLanguage();
    } catch (e) {}
    try {
      initScrollEffects();
    } catch (e) {}
    try {
      initNav();
    } catch (e) {}
    try {
      initActiveSection();
    } catch (e) {}
    try {
      initReveal();
    } catch (e) {}
    try {
      initCounters();
    } catch (e) {}
    try {
      initCopyButtons();
    } catch (e) {}
    try {
      initLightbox();
    } catch (e) {}
    try {
      initFaq();
    } catch (e) {}

    var year = document.getElementById("year");
    if (year) year.textContent = String(new Date().getFullYear());

    /* Safety net: if anything went wrong with the observer, reveal content. */
    window.setTimeout(function () {
      if (document.querySelectorAll(".js .reveal:not(.is-visible)").length > 0) {
        var hidden = document.querySelectorAll(".reveal:not(.is-visible)");
        for (var i = 0; i < hidden.length; i++) hidden[i].classList.add("is-visible");
      }
    }, 3000);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
