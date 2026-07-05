(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var container = el.querySelector(".ws-desktops");
  if (!container) return;

  var invoke = window.__zenith_invoke || window.__TAURI_INTERNALS__?.invoke;
  if (!invoke) return;

  var workspaces = [];
  var activeId = 0;

  function render() {
    container.innerHTML = "";
    for (var i = 0; i < workspaces.length; i++) {
      var w = workspaces[i];
      var dot = document.createElement("span");
      dot.className = "ws-dot" + (w.id === activeId ? " is-active" : "");
      dot.title = w.label || "Desktop " + (w.id + 1);
      dot.dataset.index = w.id;
      (function (id) {
        dot.addEventListener("click", function () {
          if (id === activeId) return;
          setActive(id);
          invoke("switch_workspace", { id: id }).then(load).catch(function (err) {
            console.error("[workspace] switch error:", err);
            load();
          });
        });
        dot.addEventListener("contextmenu", function (e) {
          e.preventDefault();
          e.stopPropagation();
          invoke("show_workspace_context_menu", { desktopId: id });
        });
      })(w.id);
      container.appendChild(dot);
    }
  }

  function setActive(id) {
    activeId = id;
    var dots = container.querySelectorAll(".ws-dot");
    for (var i = 0; i < dots.length; i++) {
      dots[i].className = "ws-dot" + (String(dots[i].dataset.index) === String(id) ? " is-active" : "");
    }
  }

  function load() {
    invoke("get_workspaces")
      .then(function (ws) {
        workspaces = ws;
        return invoke("get_active_workspace").then(function (active) {
          activeId = active;
          render();
        });
      })
      .catch(function (err) {
        console.error("[workspace] load error:", err);
      });
  }

  container.addEventListener("wheel", function (e) {
    var len = workspaces.length;
    if (len < 2) return;
    var dir = e.deltaY > 0 ? 1 : -1;
    var next = (activeId + dir + len) % len;
    setActive(next);
    invoke("switch_workspace", { id: next }).then(load).catch(function (err) {
      console.error("[workspace] scroll switch error:", err);
      load();
    });
  }, { passive: true });

  container.addEventListener("contextmenu", function (e) {
    e.preventDefault();
    e.stopPropagation();
  });

  load();

  // Guard: register listeners only once to prevent accumulation on re-layout
  if (window.__zenith_ws_listening) return;
  window.__zenith_ws_listening = true;

  if (window.__zenith_listen) {
    window.__zenith_listen("zenith:workspace-changed", function () {
      load();
    });

    window.__zenith_listen("zenith:workspace-rename", function (ev) {
      var id = Number(ev.payload);
      var currentName = workspaces[id] ? workspaces[id].label : "Desktop " + (id + 1);
      invoke("show_rename_dialog", { id: id, currentName: currentName }).catch(function (err) {
        console.error("[workspace] show rename dialog error:", err);
      });
    });

    window.__zenith_listen("zenith:workspace-delete", function (ev) {
      var id = Number(ev.payload);
      invoke("confirm_delete_desktop", { id: id }).then(function (deleted) {
        if (deleted) load();
      }).catch(function (err) {
        console.error("[workspace] delete error:", err);
        load();
      });
    });

    window.__zenith_listen("zenith:workspace-create", function () {
      invoke("create_desktop").then(load).catch(function (err) {
        console.error("[workspace] create error:", err);
      });
    });

    window.__zenith_listen("zenith:workspace-move-here", function (ev) {
      var id = Number(ev.payload);
      invoke("move_window_to_desktop", { id: id }).then(load).catch(function (err) {
        console.error("[workspace] move error:", err);
      });
    });

    window.__zenith_listen("zenith:workspace-move-to", function (ev) {
      var id = Number(ev.payload);
      invoke("move_window_to_desktop", { id: id }).then(load).catch(function (err) {
        console.error("[workspace] move error:", err);
      });
    });

    window.__zenith_listen("zenith:workspace-toggle-pin", function () {
      invoke("toggle_pin_window").catch(function (err) {
        console.error("[workspace] pin error:", err);
      });
    });
  }
})();
