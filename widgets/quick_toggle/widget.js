(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  var root = el.querySelector(".qt-root");
  if (!root) return;

  var TOGGLES = [
    { id: "wifi", icon: "wifi", label: "WiFi", showKey: "show_wifi", cmd: "toggle_wifi", stateKey: "wifi" },
    { id: "bluetooth", icon: "bluetooth", label: "BT", showKey: "show_bluetooth", cmd: "toggle_bluetooth", stateKey: "bluetooth" },
    { id: "dark_mode", icon: "moon", label: "Dark", showKey: "show_dark_mode", cmd: "toggle_dark_mode", stateKey: "dark_mode" },
    { id: "focus_assist", icon: "focus", label: "Focus", showKey: "show_focus_assist", cmd: "toggle_focus_assist", stateKey: "focus_assist" },
    { id: "airplane", icon: "plane", label: "Airplane", showKey: "show_airplane", cmd: "toggle_airplane", stateKey: "airplane" },
    { id: "night_light", icon: "sun-moon", label: "Night", showKey: "show_night_light", cmd: "toggle_night_light", stateKey: "night_light" }
  ];

  var cfg = {
    compact: true,
    show_wifi: true, show_bluetooth: true, show_dark_mode: true,
    show_focus_assist: true, show_airplane: false, show_night_light: false
  };

  var states = {};
  var chipMap = {};
  var pollTimer = null;
  var availableSet = null;

  function loadConfig(cb) {
    invoke("get_config").then(function (config) {
      var wc = config.widgets && config.widgets.config && config.widgets.config.quick_toggle;
      if (wc) {
        if (typeof wc.compact === "boolean") cfg.compact = wc.compact;
        for (var i = 0; i < TOGGLES.length; i++) {
          var k = TOGGLES[i].showKey;
          if (typeof wc[k] === "boolean") cfg[k] = wc[k];
        }
      }
      if (cb) cb();
    }).catch(function () { if (cb) cb(); });
  }

  function buildUI() {
    root.innerHTML = "";
    chipMap = {};
    root.dataset.compact = cfg.compact ? "true" : "false";

    var visible = TOGGLES.filter(function (t) { return cfg[t.showKey]; });
    for (var i = 0; i < visible.length; i++) {
      var t = visible[i];
      var wrap = document.createElement("div");
      wrap.className = "qt-chip-wrap";

      var chip = document.createElement("button");
      chip.type = "button";
      chip.className = "qt-chip";
      chip.dataset.toggle = t.id;
      chip.title = t.label;

      var ic = document.createElement("span");
      ic.className = "qt-chip-ic";
      ic.dataset.icon = t.icon;
      ic.dataset.size = "12";
      chip.append(ic);

      if (!cfg.compact) {
        var lab = document.createElement("span");
        lab.className = "qt-chip-label";
        lab.textContent = t.label;
        chip.append(lab);
      }

      (function (toggle) {
        chip.addEventListener("click", function (e) {
          e.preventDefault();
          e.stopPropagation();
          if (document.body.classList.contains("is-arranging")) return;
          invoke(toggle.cmd).then(function (newState) {
            if (newState && typeof newState.on === "boolean") {
              states[toggle.stateKey] = newState.on;
              applyChip(toggle);
            }
          }).catch(function () {});
        });
      })(t);

      var dot = document.createElement("span");
      dot.className = "qt-chip-dot";
      wrap.append(chip);
      wrap.append(dot);
      root.append(wrap);
      chipMap[t.id] = chip;
    }

    if (applyIcons) applyIcons(root);
  }

  function applyChip(t) {
    var chip = chipMap[t.id];
    if (!chip) return;
    var wrap = chip.parentElement;
    if (!wrap) return;
    var on = !!states[t.stateKey];
    chip.classList.toggle("is-on", on);
    wrap.classList.toggle("is-on", on);
    if (availableSet && availableSet[t.id] === false) {
      chip.classList.add("is-unavailable");
      wrap.classList.add("is-unavailable");
    } else {
      chip.classList.remove("is-unavailable");
      wrap.classList.remove("is-unavailable");
    }
  }

  function applyAll() {
    for (var i = 0; i < TOGGLES.length; i++) applyChip(TOGGLES[i]);
  }

  function refreshAvailable() {
    invoke("get_quick_toggle_status").then(function (status) {
      states = status.states || states;
      availableSet = status.available || {};
      applyAll();
    }).catch(function () {});
  }

  loadConfig(function () {
    buildUI();
    refreshAvailable();
    pollTimer = setInterval(function () {
      if (!document.hidden) refreshAvailable();
    }, 30000);

    if (window.__zenith_listen) {
      window.__zenith_listen("zenith:quick-toggle-updated", function () {
        refreshAvailable();
      });
    }

    var observer = new MutationObserver(function () {
      if (!document.contains(el)) {
        if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
        observer.disconnect();
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
})();
