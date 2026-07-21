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
  var runDot = el.querySelector(".zen-status-dot--run");
  var okDot = el.querySelector(".zen-status-dot--ok");
  var dotsWrap = el.querySelector(".zen-status-dots");

  function paint(state) {
    if (!state || (!state.has_unseen_failure && !state.any_running && !state.any_finished)) {
      if (dotsWrap) dotsWrap.style.display = "none";
      iconEl.title = "AI CLI — idle";
      return;
    }
    if (dotsWrap) dotsWrap.style.display = "";
    if (failDot) failDot.style.display = state.has_unseen_failure ? "inline-flex" : "none";
    if (runDot) runDot.style.display = state.any_running ? "inline-flex" : "none";
    if (okDot) okDot.style.display = state.any_finished ? "inline-flex" : "none";

    var parts = [];
    if (state.has_unseen_failure) parts.push("unseen failures");
    if (state.any_running) parts.push("running");
    if (state.any_finished) parts.push("idle");
    iconEl.title = "AI CLI — " + parts.join(" · ");
  }

  iconEl.addEventListener("click", function (e) {
    if (document.body.classList.contains("is-arranging")) return;
    e.preventDefault();
    e.stopPropagation();
    var rect = iconEl.getBoundingClientRect();
    invoke("open_ai_cli_manager", { x: rect.left, y: rect.bottom });
  });

  invoke("get_ai_cli_state", {})
    .then(function (state) {
      paint(state);
    })
    .catch(function () {});

  if (listen) {
    listen("zenith:ai-cli-changed", function (e) {
      var state = e && e.payload;
      if (!state) return;
      paint(state);
    });
  }
})();
