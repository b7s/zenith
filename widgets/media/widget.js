(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
  var setIcon = window.__zenith_setIcon;
  if (!invoke) return;

  var TAG = "[media-js]";
  function dlog() {
    try {
      var args = Array.prototype.slice.call(arguments);
      args.unshift(TAG);
      console.log.apply(console, args);
    } catch (e) {}
  }
  function dwarn() {
    try {
      var args = Array.prototype.slice.call(arguments);
      args.unshift(TAG);
      console.warn.apply(console, args);
    } catch (e) {}
  }

  var root = el.querySelector(".md-root");
  var bgEl = root && root.querySelector(".md-bg");
  var chip = root && root.querySelector(".md-chip");
  var chipIc = root && root.querySelector(".md-chip-ic");
  var thumbEl = root && root.querySelector(".md-thumb");
  var info = root && root.querySelector(".md-info");
  var titleWrap = info && info.querySelector(".md-title-wrap");
  var artistWrap = info && info.querySelector(".md-artist-wrap");

  var scrollLabel = false;
  var compact = false;
  var thumbStyle = "background";

  var lastTitle = "";
  var lastArtist = "";
  var lastStatus = "";
  var current = null;
  var pollTimer = null;

  var scrollAnimDur = 10;

  function chipIcon(status) {
    if (status === "playing") return "pause";
    return "play";
  }

  function chipClass(status) {
    if (status === "playing") return "md-chip is-playing";
    return "md-chip";
  }

  function rootClass(info) {
    if (!info) return "md-root is-none";
    var cls = "md-root";
    if (info.status === "paused") cls += " is-paused";
    if (info.status === "stopped" || info.status === "closed") cls += " is-none";
    if (compact) cls += " is-compact";
    return cls;
  }

  function setupText(wrap, text, canScroll) {
    if (!wrap) return;
    wrap.innerHTML = "";

    if (!text) {
      wrap.classList.remove("is-scroll");
      return;
    }

    // Measure overflow with a hidden-span technique
    var meas = document.createElement("span");
    meas.style.cssText =
      "position:absolute;visibility:hidden;white-space:nowrap;font-size:inherit;font-weight:inherit;pointer-events:none";
    meas.textContent = text;
    wrap.appendChild(meas);
    var textW = meas.offsetWidth;
    wrap.removeChild(meas);

    var overflows = canScroll && text.length > 0 && textW > wrap.clientWidth;

    if (overflows) {
      wrap.classList.add("is-scroll");
      var scroll = document.createElement("span");
      scroll.className = "md-text-scroll";
      // Duration proportional to text length (longer = slower to keep speed consistent)
      var dur = Math.max(6, Math.min(16, Math.round(textW / 30)));
      scroll.style.animationDuration = dur + "s";
      for (var i = 0; i < 2; i++) {
        var t = document.createElement("span");
        t.className = "md-text-copy";
        t.textContent = text;
        scroll.appendChild(t);
      }
      wrap.appendChild(scroll);
    } else {
      wrap.classList.remove("is-scroll");
      var plain = document.createElement("span");
      plain.className = "md-text-plain";
      plain.textContent = text;
      wrap.appendChild(plain);
    }
  }

  function triggerEnterAnimation() {
    if (!titleWrap || !artistWrap) return;
    titleWrap.classList.remove("md-title--enter");
    artistWrap.classList.remove("md-artist--enter");
    void titleWrap.offsetWidth;
    titleWrap.classList.add("md-title--enter");
    artistWrap.classList.add("md-artist--enter");
  }

  function render(info) {
    current = info;

    var status = info ? info.status : "none";
    var title = info && info.title ? info.title : (info ? "" : "No media");
    var artist = info ? info.artist : "";

    if (chip) {
      chip.className = chipClass(status);
      if (chipIc && setIcon) {
        setIcon(chipIc, chipIcon(status), { size: 12 });
      }
    }
    if (root) root.className = rootClass(info);

    setupText(titleWrap, title, scrollLabel);
    setupText(artistWrap, artist, scrollLabel);

    if (bgEl && thumbEl) {
      if (info && info.thumbnail) {
        if (thumbStyle === "background") {
          bgEl.style.backgroundImage = "url(\"" + info.thumbnail + "\")";
          root.classList.add("is-thumb-bg");
          thumbEl.style.display = "none";
        } else {
          root.classList.remove("is-thumb-bg");
          bgEl.style.backgroundImage = "";
          thumbEl.style.backgroundImage = "url(\"" + info.thumbnail + "\")";
          thumbEl.style.display = "";
        }
      } else {
        root.classList.remove("is-thumb-bg");
        bgEl.style.backgroundImage = "";
        thumbEl.style.display = "none";
      }
    }

    if (info) {
      var tip = title;
      if (artist) tip += " \u00b7 " + artist;
      if (status === "playing") tip += " (playing)";
      else if (status === "paused") tip += " (paused)";
      el.title = tip;
    } else {
      el.title = "No media playing";
    }

    if (info && status !== "none") {
      var enter = title !== lastTitle || artist !== lastArtist;
      if (enter) triggerEnterAnimation();
    }
    lastTitle = title;
    lastArtist = artist;
    lastStatus = status;
  }

  function applySnapshot(snap, source) {
    if (!snap || !snap.available || !snap.info) {
      render(null);
      return;
    }
    render(snap.info);
  }

  function refresh() {
    invoke("get_media").then(function (snap) {
      applySnapshot(snap, "ipc");
    }).catch(function (e) {
      dwarn("get_media FAILED:", String(e));
      render(null);
    });
  }

  function togglePlay() {
    invoke("media_toggle_play_pause").catch(function (e) {
      dwarn("media_toggle_play_pause failed:", String(e));
    });
    if (current) {
      var next = (current.status === "playing") ? "paused" : "playing";
      current.status = next;
      if (chip) {
        chip.className = chipClass(next);
        if (chipIc && setIcon) setIcon(chipIc, chipIcon(next), { size: 12 });
      }
      if (root) root.className = rootClass(current);
    }
  }

  function loadConfig() {
    invoke("get_config").then(function (cfg) {
      var wc = (cfg.widgets && cfg.widgets.config && cfg.widgets.config["media"]) || {};
      scrollLabel = wc.scroll_label !== false;
      compact = wc.compact === true;
      thumbStyle = wc.thumb_style === "cover" ? "cover" : "background";
      dlog("config ok:", "scroll_label=" + scrollLabel, "compact=" + compact, "thumb_style=" + thumbStyle);
      if (root) root.className = rootClass(current);
      refresh();
    }).catch(function (e) {
      dwarn("get_config failed:", String(e));
      refresh();
    });
  }

  if (chipIc && setIcon) {
    setIcon(chipIc, "play", { size: 12 });
  }

  if (chip) {
    chip.addEventListener("click", function (e) {
      if (document.body.classList.contains("is-arranging")) return;
      e.preventDefault();
      e.stopPropagation();
      togglePlay();
    });
  }

  el.addEventListener("wheel", function (e) {
    if (!current) return;
    e.preventDefault();
    var step = e.deltaY > 0 ? 5000 : -5000;
    var next = Math.max(0, Math.min(current.duration_ms, current.position_ms + step));
    current.position_ms = next;
    invoke("media_seek", { position_ms: next }).catch(function () {});
    render(current);
  });

  if (listen) {
    listen("zenith:media-changed", function (ev) {
      if (ev && ev.payload) {
        applySnapshot(ev.payload, "event");
      } else {
        refresh();
      }
    });
  }

  pollTimer = setInterval(refresh, 10000);

  loadConfig();
})()
