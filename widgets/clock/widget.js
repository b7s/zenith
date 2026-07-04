(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var span = el.querySelector(".clock-time");
  if (!span) return;

  function update() {
    var d = new Date();
    var h = String(d.getHours()).padStart(2, "0");
    var m = String(d.getMinutes()).padStart(2, "0");
    span.textContent = h + ":" + m;
  }

  update();
  setInterval(update, 1000);
})();
