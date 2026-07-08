(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  if (!invoke) return;

  var timeEl = el.querySelector(".dt-time");
  var dateEl = el.querySelector(".dt-date");
  if (!timeEl) return;

  var timezone = "";
  var format = "24h";
  var showDate = true;
  var showYear = false;

  // Open the calendar popup on left-click. We pipe through the shared
  // helper exposed at window.__zenith_openCalendar when present (the bar
  // sets it in main.ts). Falls back to the raw IPC command otherwise.
  el.addEventListener("click", function () {
    // Respect the right-click context menu — don't open on contextmenu.
    if (document.body.classList.contains("is-arranging")) return;
    var open = window.__zenith_openCalendar || function (e) {
      invoke("open_calendar", { x: 0, y: 0 });
    };
    try { open(el); } catch (err) { /* swallow — UI never blocks the bar */ }
  });

  function update() {
    var now = new Date();
    var timeOpts =
      format === "12h"
        ? { hour: "2-digit", minute: "2-digit", hour12: true }
        : { hour: "2-digit", minute: "2-digit", hour12: false };
    if (timezone) timeOpts.timeZone = timezone;

    try {
      timeEl.textContent = now.toLocaleTimeString("en-US", timeOpts);
    } catch (e) {
      timeOpts.timeZone = undefined;
      timeEl.textContent = now.toLocaleTimeString("en-US", timeOpts);
    }

    if (showDate && dateEl) {
      var dateOpts = { weekday: "short", month: "short", day: "numeric" };
      if (showYear) dateOpts.year = "numeric";
      if (timezone) dateOpts.timeZone = timezone;
      try {
        dateEl.querySelector('span').textContent = now.toLocaleDateString("en-US", dateOpts);
        dateEl.style.display = "";
      } catch (e) {
        dateOpts.timeZone = undefined;
        dateEl.querySelector('span').textContent = now.toLocaleDateString("en-US", dateOpts);
        dateEl.style.display = "";
      }
    } else if (dateEl) {
      dateEl.style.display = "none";
    }
  }

  function loadConfig() {
    invoke("get_config").then(function (cfg) {
      var widgetCfg =
        (cfg.widgets && cfg.widgets.config && cfg.widgets.config["datetime"]) || {};
      timezone = widgetCfg.timezone || "";
      format = widgetCfg.format || "24h";
      showDate = widgetCfg.show_date !== undefined ? widgetCfg.show_date : true;
      showYear = widgetCfg.show_year !== undefined ? widgetCfg.show_year : false;
      update();
    }).catch(function () {
      update();
    });
  }

  loadConfig();
  update();
  setInterval(update, 1000);
})();
