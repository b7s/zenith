(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return; // openAicli is read lazily on click — it is wired via a
                       // dynamic import in bar/main.ts and may not be set when
                       // this IIFE first runs (it races layoutBar). Bailing
                       // here would permanently kill the dots + listener.

  var TAG = "[ai-cli-js]";
  function dlog() {
    try {
      var args = Array.prototype.slice.call(arguments);
      args.unshift(TAG);
      console.log.apply(console, args);
    } catch (e) {}
  }

  var iconEl = el.querySelector(".ai-icon");
  if (!iconEl) return;
  if (applyIcons) applyIcons(el);

  // Severity ranking for which single dot to show on the bar: failed > waiting > running.
  function severity(state) {
    if (!state) return "none";
    if (state.totals && state.totals.failed > 0) return "failed";
    if (state.totals && state.totals.waiting > 0) return "waiting";
    if (state.totals && state.totals.running > 0) return "running";
    return "none";
  }

  var failDot = el.querySelector(".zen-status-dot--fail");
  var waitDot = el.querySelector(".zen-status-dot--wait");
  var runDot = el.querySelector(".zen-status-dot--run");

  function paint(state) {
    if (failDot) failDot.style.display = "none";
    if (waitDot) waitDot.style.display = "none";
    if (runDot) runDot.style.display = "none";

    var sev = severity(state);
    if (sev === "failed" && failDot) failDot.style.display = "inline-flex";
    else if (sev === "waiting" && waitDot) waitDot.style.display = "inline-flex";
    else if (sev === "running" && runDot) runDot.style.display = "inline-flex";

    // Tooltip: summarize running agents.
    var parts = [];
    if (state && state.sessions) {
      for (var i = 0; i < state.sessions.length; i++) {
        var s = state.sessions[i];
        if (!s.installed || !s.running) continue;
        var st = s.status === "failed" ? "failed"
          : s.status === "waiting" ? "waiting for confirmation"
          : "running";
        parts.push(s.label + ": " + st + (s.title ? " — " + s.title : ""));
      }
    }
    el.title = parts.length > 0 ? parts.join("\n") : "AI Agents — idle";
  }

  iconEl.addEventListener("click", function () {
    if (document.body.classList.contains("is-arranging")) return;
    // `openAicli` is resolved lazily on every click — it's bound through a
    // dynamic `import()` in bar/main.ts that races the initial layoutBar.
    var openAicli = window.__zenith_openAicli;
    if (typeof openAicli === "function") {
      try { openAicli(iconEl); return; } catch (err) { dlog("openAicli threw", err); }
    }
    var invokeFn = window.__zenith_invoke || (typeof window.__TAURI_INTERNALS__ !== "undefined" && window.__TAURI_INTERNALS__.invoke);
    if (invokeFn) {
      invokeFn("open_aicli_window", { x: 0, y: 0 }).catch(function (err) {
        console.error(TAG, "invoke failed:", err);
      });
    } else {
      console.error(TAG, "no invoke available");
    }
  });

  invoke("get_aicli_state")
    .then(function (state) {
      paint(state);
    })
    .catch(function (e) { dlog("init err", e); });

  if (listen) {
    listen("zenith:aicli-changed", function (e) {
      var state = e && e.payload;
      if (!state) return;
      paint(state);
    });
  }
})();
