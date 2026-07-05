(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var container = el.querySelector(".ws-desktops");
  if (!container) return;

  var invoke = window.__zenith_invoke || window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;

  var timer = null;

  function render(workspaces, activeId) {
    container.innerHTML = "";
    for (var i = 0; i < workspaces.length; i++) {
      var w = workspaces[i];
      var dot = document.createElement("span");
      dot.className = "ws-dot" + (w.id === activeId ? " is-active" : "");
      dot.title = "Desktop " + w.label;
      (function (id) {
        dot.addEventListener("click", function () {
          invoke("switch_workspace", { id: id }).then(function () {
            load();
          }).catch(function (err) {
            console.error("[workspace] switch error:", err);
          });
        });
      })(w.id);
      container.appendChild(dot);
    }
  }

  function load() {
    invoke("get_workspaces")
      .then(function (workspaces) {
        return invoke("get_active_workspace").then(function (active) {
          render(workspaces, active);
        });
      })
      .catch(function (err) {
        console.error("[workspace] load error:", err);
      });
  }

  load();
  timer = setInterval(load, 5000);

  if (window.__TAURI__?.event?.listen) {
    window.__TAURI__.event.listen("zenith:workspace-changed", function () {
      load();
    });
  }
})();
