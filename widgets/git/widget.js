(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
  var setIcon = window.__zenith_setIcon;
  if (!invoke) return;

  var TAG = "[git-js]";
  function dlog() {
    try {
      var args = Array.prototype.slice.call(arguments);
      args.unshift(TAG);
      console.log.apply(console, args);
    } catch (e) {}
  }

  var wrap = el.querySelector(".git-wrap");
  if (!wrap) return;

  if (setIcon) {
    var icEl = wrap.querySelector(".git-ic");
    if (icEl) setIcon(icEl, "git-branch", { size: 14 });
  }

  function paint(state) {
    var total = (state && state.total_failed) || 0;
    var anyAccount = state && state.inventories && state.inventories.length > 0;
    var anyError = state && state.inventories.some(function (i) { return i.last_error && i.last_error.length > 0; });

    wrap.classList.remove("is-empty", "is-broken", "is-clean", "is-active");
    if (!anyAccount) {
      wrap.classList.add("is-empty");
      return;
    }
    if (total > 0) {
      wrap.classList.add("is-active");
      return;
    }
    if (anyError) {
      wrap.classList.add("is-broken");
      return;
    }
    wrap.classList.add("is-clean");
  }

  wrap.addEventListener("click", function (e) {
    e.preventDefault();
    e.stopPropagation();
    var rect = wrap.getBoundingClientRect();
    invoke("open_git_manager", { x: rect.left, y: rect.bottom });
  });

  invoke("get_git_state", { accountId: null })
    .then(function (state) {
      dlog("initial state total=" + state.total_failed);
      paint(state);
    })
    .catch(function (e) { dlog("init err", e); });

  if (listen) {
    listen("zenith:git-changed", function (e) {
      var state = e && e.payload;
      if (!state) return;
      paint(state);
    });
  }
})();
