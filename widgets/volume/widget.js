(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  var iconEl = el.querySelector(".vol-icon");
  if (!iconEl) return;

  // applyIcons already ran on startup before widget HTML existed,
  // so render the icon now
  if (applyIcons) applyIcons(el);

  var currentVolume = 0.5;
  var currentMuted = false;
  var pollTimer = null;

  function iconName(level, muted) {
    if (muted || level < 0.01) return "volume-x";
    if (level < 0.50) return "volume-1";
    return "volume-2";
  }

  function updateIcon() {
    iconEl.dataset.icon = iconName(currentVolume, currentMuted);
    if (applyIcons) applyIcons(el);
  }

  function refresh() {
    invoke("get_volume")
      .then(function (info) {
        currentVolume = info.level;
        currentMuted = info.muted;
        updateIcon();
      })
      .catch(function () {});
  }

  refresh();
  pollTimer = setInterval(refresh, 2000);

  var observer = new MutationObserver(function () {
    if (!document.contains(el)) {
      if (pollTimer) clearInterval(pollTimer);
      observer.disconnect();
    }
  });
  observer.observe(document.body, { childList: true, subtree: true });

  // Scroll to change volume
  el.addEventListener("wheel", function (e) {
    e.preventDefault();
    var delta = e.deltaY > 0 ? -0.05 : 0.05;
    var newLevel = Math.max(0, Math.min(1, currentVolume + delta));
    currentVolume = newLevel;
    if (currentMuted) {
      currentMuted = false;
      invoke("set_muted", { muted: false }).catch(function () {});
    }
    invoke("set_volume", { level: newLevel }).catch(function () {});
    updateIcon();
  });

  // Click opens volume popup near the widget, clamped to screen edges
  el.addEventListener("click", function (e) {
    var rect = el.getBoundingClientRect();
    var dpr = window.devicePixelRatio || 1;

    // Widget center on screen
    var barLeft = e.screenX - e.clientX;
    var widgetCenter = barLeft + rect.left + rect.width / 2;

    var popupW = 260;
    var popupH = 60;
    var gap = 4;
    var margin = 8;

    var screenW = screen.width;
    var screenH = screen.height;

    var popupX = Math.max(
      margin,
      Math.min(widgetCenter - popupW / 2, screenW - popupW - margin)
    );
    var barTop = e.screenY - e.clientY;
    var popupY = barTop + rect.bottom + gap;
    popupY = Math.max(0, Math.min(popupY, screenH - popupH - margin));

    invoke("open_volume_popup", {
      x: popupX * dpr,
      y: popupY * dpr,
    }).catch(function () {});
  });

  // Right-click toggles mute/unmute directly
  el.addEventListener("contextmenu", function (e) {
    e.preventDefault();
    e.stopPropagation();
    currentMuted = !currentMuted;
    invoke("set_muted", { muted: currentMuted }).catch(function () {});
    updateIcon();
  });
})();
