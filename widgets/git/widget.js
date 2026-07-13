(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var listen = window.__zenith_listen;
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

  // Build the Git logo SVG programmatically (path data in one place — widget.js)
  var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 78 78");
  svg.setAttribute("fill", "currentColor");
  svg.classList.add("git-ic");
  var path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("transform", "translate(10 10) rotate(-45 29 29)");
  path.setAttribute("d", "M5,58c-2.76142,0 -5,-2.23858 -5,-5v-48c0,-2.76142 2.23858,-5 5,-5h33v12.54404c-2.06553,0.94801 -3.5,3.03446 -3.5,5.45596c0,0.73514 0.13221,1.43941 0.37415,2.09031l-15.28384,15.28384c-0.6509,-0.24194 -1.35517,-0.37415 -2.09031,-0.37415c-3.31371,0 -6,2.68629 -6,6c0,3.31371 2.68629,6 6,6c3.31371,0 6,-2.68629 6,-6c0,-0.73514 -0.13221,-1.43941 -0.37415,-2.09031l14.87415,-14.87415l0,11.50851c-2.06553,0.94801 -3.5,3.03446 -3.5,5.45596c0,3.31371 2.68629,6 6,6c3.31371,0 6,-2.68629 6,-6c0,-2.42149 -1.43447,-4.50795 -3.5,-5.45596l0,-12.08808c2.06553,-0.94801 3.5,-3.03446 3.5,-5.45596c0,-2.42149 -1.43447,-4.50795 -3.5,-5.45596l0,-12.54404h10c2.76142,0 5,2.23858 5,5v48c0,2.76142 -2.23858,5 -5,5z");
  svg.append(path);
  wrap.append(svg);

  function paint(state) {
    var total = (state && state.total_failed) || 0;
    var openPrs = (state && state.total_open_prs) || 0;
    var anyAccount = state && state.inventories && state.inventories.length > 0;
    var anyError = state && state.inventories.some(function (i) { return i.last_error && i.last_error.length > 0; });

    wrap.classList.remove("is-empty", "is-broken", "is-clean", "is-active", "has-pr");
    if (!anyAccount) {
      wrap.classList.add("is-empty");
      return;
    }
    if (total > 0) {
      wrap.classList.add("is-active");
    } else if (anyError) {
      wrap.classList.add("is-broken");
    } else {
      wrap.classList.add("is-clean");
    }
    if (openPrs > 0) {
      wrap.classList.add("has-pr");
    }

    var parts = [];
    if (total > 0) parts.push(total + " failed CI");
    if (openPrs > 0) parts.push(openPrs + " open PR");
    wrap.title = parts.length > 0 ? parts.join(" · ") : "Git — no issues";
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
