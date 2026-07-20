(function () {
  var slot = document.currentScript && document.currentScript.parentElement;
  if (!slot) return;

  var root = slot.querySelector(".lk-root");
  if (!root) {
    var allRoots = document.querySelectorAll(".lk-root");
    for (var i = 0; i < allRoots.length; i++) {
      if (allRoots[i].closest("[data-widget]") === slot) {
        root = allRoots[i];
        break;
      }
    }
    if (!root) return;
  }

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
  if (!invoke) return;

  var icons = {};
  // Per-layout icon cache: link.id → data-URL-string | null. Avoids re-IPC'ing
  // the same PNG bytes on every render. The cache is recreated on every
  // layoutBar (IIFE re-runs), so config changes get a fresh fetch.
  var iconCache = {};

  function getIconForLink(id, cb) {
    if (iconCache[id] !== undefined) {
      cb(iconCache[id]);
      return;
    }
    invoke("get_link_icon_data", { id: id }).then(function (dataUrl) {
      iconCache[id] = dataUrl || null;
      cb(iconCache[id]);
    }).catch(function () {
      iconCache[id] = null;
      cb(null);
    });
  }

  function getDomain(url) {
    try {
      var u = new URL(url);
      return u.hostname;
    } catch (e) {
      return null;
    }
  }

  function linkTitle(link) {
    return link.label || getDomain(link.url) || "Link";
  }

  function getLinks() {
    return invoke("get_config").then(function (cfg) {
      var wc = (cfg.widgets && cfg.widgets.config && cfg.widgets.config["links"]) || {};
      var list = wc.links || [];
      return list.filter(function (l) { return l.enabled !== false; });
    });
  }

  function renderIcon(link) {
    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "lk-ic";
    btn.dataset.linkId = link.id;
    btn.title = linkTitle(link);
    btn.setAttribute("aria-label", btn.title);

    var glyph = document.createElement("span");
    glyph.className = "lk-ic-glyph";
    btn.append(glyph);

    function showGlobe() {
      if (window.__zenith_setIcon) {
        window.__zenith_setIcon(glyph, "globe", { size: 16 });
      }
    }

    // Icons live on disk (one PNG per link); fetch via IPC. globe is the
    // fallback when no icon is configured or the image fails to load.
    getIconForLink(link.id, function (dataUrl) {
      if (!dataUrl) {
        showGlobe();
        return;
      }
      var img = document.createElement("img");
      img.className = "lk-ic-img";
      img.src = dataUrl;
      img.alt = "";
      img.addEventListener("error", function () {
        if (img.parentNode) img.parentNode.removeChild(img);
        showGlobe();
      });
      glyph.append(img);
    });

    var dot = document.createElement("span");
    dot.className = "lk-dot";
    dot.setAttribute("aria-hidden", "true");
    btn.append(dot);

    btn.addEventListener("click", function () {
      if (document.body.classList.contains("is-arranging")) return;
      var rect = btn.getBoundingClientRect();
      invoke("open_link", { id: link.id, x: rect.left, y: rect.bottom }).catch(function () {});
    });
    btn.addEventListener("contextmenu", function (e) {
      e.preventDefault();
      e.stopPropagation();
      invoke("show_link_menu", { id: link.id }).catch(function () {});
    });

    icons[link.id] = btn;
    return btn;
  }

  function render(list) {
    root.innerHTML = "";
    icons = {};
    if (!list || list.length === 0) {
      root.style.display = "none";
      return;
    }
    root.style.display = "";
    for (var i = 0; i < list.length; i++) {
      root.append(renderIcon(list[i]));
    }
  }

  function update() {
    getLinks().then(render).catch(function () { render([]); });
  }

  if (listen) {
    listen("zenith:link-notification", function (e) {
      var p = e && e.payload;
      if (!p) return;
      var btn = icons[p.id];
      if (!btn) return;
      var dot = btn.querySelector(".lk-dot");
      if (dot) dot.classList.toggle("is-active", !!p.has);
    });
  }

  update();
})();
