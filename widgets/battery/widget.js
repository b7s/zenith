(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  var iconEl = el.querySelector(".bat-icon");
  if (!iconEl) return;

  if (applyIcons) applyIcons(el);

  var currentPercent = -1;
  var currentCharging = false;
  var pollTimer = null;

  function iconName(percent, charging) {
    if (percent < 0) return "battery";
    if (charging) return "battery-charging";
    if (percent <= 15) return "battery-warning";
    if (percent <= 30) return "battery-low";
    if (percent <= 60) return "battery-medium";
    return "battery-full";
  }

  function updateUI() {
    iconEl.dataset.icon = iconName(currentPercent, currentCharging);
    if (applyIcons) applyIcons(el);

    if (currentPercent >= 0) {
      var label = "Battery: " + currentPercent + "%";
      if (currentCharging) label += " (Charging)";
      el.title = label;
    } else {
      el.title = "";
    }
  }

  function refresh() {
    invoke("get_battery_status")
      .then(function (info) {
        currentPercent = info.percent;
        currentCharging = info.charging;
        updateUI();
      })
      .catch(function () {});
  }

  refresh();
  pollTimer = setInterval(refresh, 10000);

  var observer = new MutationObserver(function () {
    if (!document.contains(el)) {
      if (pollTimer) clearInterval(pollTimer);
      observer.disconnect();
    }
  });
  observer.observe(document.body, { childList: true, subtree: true });
})();
