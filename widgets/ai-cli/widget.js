(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  var iconEl = el.querySelector(".ac-icon");
  if (!iconEl) return;

  if (applyIcons) applyIcons(el);

  var failDot = el.querySelector(".zen-status-dot--fail");
  var waitDot = el.querySelector(".zen-status-dot--wait");
  var runDot = el.querySelector(".zen-status-dot--run");
  var okDot = el.querySelector(".zen-status-dot--ok");
  var dotsWrap = el.querySelector(".zen-status-dots");

  // Read initial state
  var lastState = null;

  function paint(state) {
    lastState = state;
    var hasFail = !!(state && state.has_unseen_failure);
    var hasRun = !!(state && state.any_running);
    var hasWait = !!(state && state.per_cli && state.per_cli.some(function (s) { return s.is_waiting; }));
    var hasOk = !hasFail && !hasRun && !hasWait;

    // Decide which dots to show
    if (dotsWrap) {
      var any = hasFail || hasWait || hasRun || hasOk;
      dotsWrap.style.display = any ? "flex" : "none";
    }
    if (failDot) failDot.style.display = hasFail ? "inline-flex" : "none";
    if (waitDot) waitDot.style.display = hasWait ? "inline-flex" : "none";
    if (runDot) runDot.style.display = hasRun ? "inline-flex" : "none";
    if (okDot) okDot.style.display = hasOk ? "inline-flex" : "none";

    var parts = [];
    if (hasFail) parts.push("unseen failures");
    if (hasWait) parts.push("waiting confirmation");
    if (hasRun) parts.push("running");
    if (hasOk) parts.push("idle");
    iconEl.title = parts.length ? "AI CLI — " + parts.join(" · ") : "AI CLI";
  }

  iconEl.addEventListener("click", function (e) {
    if (document.body.classList.contains("is-arranging")) return;
    e.preventDefault();
    e.stopPropagation();
    // Acknowledge failures when opening manager (clears red dot)
    if (lastState && lastState.has_unseen_failure) {
      invoke("ack_ai_cli_failures", {}).catch(function () {});
    }
    var rect = iconEl.getBoundingClientRect();
    invoke("open_ai_cli_manager", { x: rect.left, y: rect.bottom });
  });

  invoke("get_ai_cli_state", {})
    .then(function (state) { paint(state); })
    .catch(function () {});

  if (listen) {
    listen("zenith:ai-cli-changed", function (e) {
      var state = e && e.payload;
      if (!state) return;
      paint(state);
    });
  }
})();
