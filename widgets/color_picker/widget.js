(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  if (!invoke) return;

  var iconEl = el.querySelector(".cp-icon");
  if (!iconEl) return;

  if (applyIcons) applyIcons(el);

  // Left-click: sample a screen pixel with the fullscreen eyedropper window.
  // The window copies the color to the clipboard (in the configured format)
  // on pick and closes itself.
  el.addEventListener("click", function (e) {
    if (document.body.classList.contains("is-arranging")) return;
    e.preventDefault();
    var rect = el.getBoundingClientRect();
    var barLeft = e.screenX - e.clientX;
    var cx = barLeft + rect.left + rect.width / 2;
    var barTop = e.screenY - e.clientY;
    var cy = barTop + rect.bottom + 4;
    invoke("open_eyedropper", { x: cx, y: cy }).catch(function () {});
  });

  // Right-click: open the full color-picker window (no native menu — we
  // render our own inside the window).
  el.addEventListener("contextmenu", function (e) {
    e.preventDefault();
    e.stopPropagation();
    if (document.body.classList.contains("is-arranging")) return;
    var rect = el.getBoundingClientRect();
    var barLeft = e.screenX - e.clientX;
    var cx = barLeft + rect.left + rect.width / 2;
    var barTop = e.screenY - e.clientY;
    var cy = barTop + rect.bottom + 4;
    invoke("open_color_picker", { x: cx, y: cy }).catch(function () {});
  });
})();
