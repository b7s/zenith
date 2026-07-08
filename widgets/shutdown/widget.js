(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  if (applyIcons) applyIcons(el);

  el.addEventListener("click", function () {
    if (document.body.classList.contains("is-arranging")) return;
    var rect = el.getBoundingClientRect();
    var dpr = window.devicePixelRatio || 1;
    var barLeft = window.screenX;
    var widgetCenter = barLeft + rect.left + rect.width / 2;
    var barTop = window.screenY;
    var popupX = widgetCenter - 130;
    var popupY = barTop + rect.bottom + 4;
    invoke("open_shutdown_popup", {
      x: Math.round(popupX * dpr),
      y: Math.round(popupY * dpr),
    }).catch(function () {});
  });
})();
