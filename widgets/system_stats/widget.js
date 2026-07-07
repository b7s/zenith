(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  if (!invoke) return;
  var applyIcons = window.__zenith_applyIcons;

  var root = el.querySelector(".sys-root");
  if (!root) return;

  var cfg = {
    style: "bar",
    format: "percent",
    show_cpu: true,
    show_ram: true,
    show_gpu: true,
    show_hd: true,
    show_network: true,
    selected_gpus: [],
    selected_hds: [],
    selected_networks: [],
    refresh_seconds: 3,
    history_size: 20,
  };

  function loadConfig() {
    try {
      invoke("get_config").then(function (config) {
        var wc = config.widgets && config.widgets.config && config.widgets.config.system_stats;
        if (wc) {
          if (wc.style) cfg.style = wc.style;
          if (wc.format) cfg.format = wc.format;
          if (typeof wc.show_cpu === "boolean") cfg.show_cpu = wc.show_cpu;
          if (typeof wc.show_ram === "boolean") cfg.show_ram = wc.show_ram;
          if (typeof wc.show_gpu === "boolean") cfg.show_gpu = wc.show_gpu;
          if (typeof wc.show_hd === "boolean") cfg.show_hd = wc.show_hd;
          if (typeof wc.show_network === "boolean") cfg.show_network = wc.show_network;
          if (Array.isArray(wc.selected_gpus)) cfg.selected_gpus = wc.selected_gpus;
          if (Array.isArray(wc.selected_hds)) cfg.selected_hds = wc.selected_hds;
          if (Array.isArray(wc.selected_networks)) cfg.selected_networks = wc.selected_networks;
          if (wc.refresh_seconds >= 1 && wc.refresh_seconds <= 10)
            cfg.refresh_seconds = wc.refresh_seconds;
          if (wc.history_size >= 5 && wc.history_size <= 40)
            cfg.history_size = wc.history_size;
        }
        buildUI();
      }).catch(function () {});
    } catch (_) {}
  }

  var wrap;
  var cpuEl, ramEl;
  var cpuFillEl, ramFillEl;
  var cpuPctEl, ramPctEl;
  var cpuDots, ramDots;
  var cpuGraphEl, ramGraphEl;
  var cpuHistory, ramHistory;
  var cpuGraphPath, ramGraphPath;

  function rowHtml(label, style) {
    if (style === "dots") {
      return '<span class="sys-label">' + label + '</span><span class="sys-dots"></span><span class="sys-pct"></span>';
    }
    if (style === "graph") {
      return '<span class="sys-label">' + label + '</span><svg class="sys-graph" preserveAspectRatio="none" viewBox="0 0 100 16"><path></path></svg><span class="sys-pct"></span>';
    }
    return '<span class="sys-label">' + label + '</span><span class="sys-bar"><span class="sys-fill"></span></span><span class="sys-pct"></span>';
  }

  function buildUI() {
    root.dataset.style = cfg.style;
    root.innerHTML = '<span class="sys-wrap"></span>';
    wrap = root.firstElementChild;

    if (cfg.show_ram) {
      var r = document.createElement("span");
      r.className = "sys-row";
      r.innerHTML = rowHtml("RAM", cfg.style);
      wrap.append(r);
      ramEl = r;
      ramFillEl = r.querySelector(".sys-fill");
      ramPctEl = r.querySelector(".sys-pct");
      ramDots = r.querySelector(".sys-dots");
      ramGraphEl = r.querySelector(".sys-graph");
      ramGraphPath = r.querySelector(".sys-graph path");
    }

    if (cfg.show_cpu) {
      var r = document.createElement("span");
      r.className = "sys-row";
      r.innerHTML = rowHtml("CPU", cfg.style);
      wrap.append(r);
      cpuEl = r;
      cpuFillEl = r.querySelector(".sys-fill");
      cpuPctEl = r.querySelector(".sys-pct");
      cpuDots = r.querySelector(".sys-dots");
      cpuGraphEl = r.querySelector(".sys-graph");
      cpuGraphPath = r.querySelector(".sys-graph path");
    }

    if (cfg.style === "graph") {
      cpuHistory = new Float64Array(cfg.history_size);
      cpuHistory.fill(0);
      ramHistory = new Float64Array(cfg.history_size);
      ramHistory.fill(0);
    }

    if (cfg.show_ram && cfg.style === "dots" && ramDots) buildDots(ramDots, 10);
    if (cfg.show_cpu && cfg.style === "dots" && cpuDots) buildDots(cpuDots, 10);
  }

  function buildDots(parent, n) {
    for (var i = 0; i < n; i++) {
      var d = document.createElement("span");
      d.className = "sys-dot";
      parent.append(d);
    }
  }

  function formatVal(pct, used, total) {
    if (cfg.format === "raw") {
      if (total === 0) return "0B";
      return formatBytes(used);
    }
    if (cfg.format === "both") {
      return Math.round(pct) + "% " + formatBytes(used);
    }
    return Math.round(pct) + "";
  }

  function formatBytes(b) {
    if (b >= 1073741824) return (b / 1073741824).toFixed(1) + " GB";
    if (b >= 1048576) return (b / 1048576).toFixed(0) + " MB";
    return (b / 1024).toFixed(0) + " KB";
  }

  function formatRate(bps) {
    if (bps >= 1048576) return (bps / 1048576).toFixed(1) + " MB/s";
    if (bps >= 1024) return (bps / 1024).toFixed(1) + " KB/s";
    return Math.round(bps) + " B/s";
  }

  function heatClass(pct) {
    if (pct >= 85) return "is-hot";
    if (pct >= 60) return "is-warn";
    return "";
  }

  function updateDots(parent, pct, n) {
    var full = Math.max(0, Math.min(n, Math.round((pct / 100) * n)));
    var dots = parent.children;
    for (var i = 0; i < n; i++) {
      var d = dots[i];
      if (i < full) {
        d.className = "sys-dot is-on " + heatClass(pct);
      } else {
        d.className = "sys-dot";
      }
    }
  }

  function updateGraph(pathEl, history, pct) {
    var n = history.length;
    if (n === 0) return;
    var w = 100;
    var h = 16;
    var parts = [];
    for (var i = 0; i < n; i++) {
      var x = (i / (n - 1 || 1)) * w;
      var y = h - (Math.min(100, Math.max(0, history[i])) / 100) * h;
      if (i === 0) parts.push("M" + x.toFixed(1) + "," + y.toFixed(1));
      else parts.push("L" + x.toFixed(1) + "," + y.toFixed(1));
    }
    pathEl.setAttribute("d", parts.join(""));
    pathEl.className.baseVal = heatClass(pct);
  }

  function pushHistory(buf, val) {
    var n = buf.length;
    for (var i = 0; i < n - 1; i++) buf[i] = buf[i + 1];
    buf[n - 1] = val;
  }

  function cpuText(pct, ghz) {
    if (cfg.format === "raw") {
      return (ghz || 0).toFixed(2) + " GHz";
    }
    if (cfg.format === "both") {
      return Math.round(pct) + "% " + (ghz || 0).toFixed(2) + " GHz";
    }
    return Math.round(pct) + "";
  }

  function setRowPct(pctEl, txt, isPercent) {
    if (isPercent) {
      pctEl.innerHTML = txt + '<span class="sys-pct-suffix">%</span>';
    } else {
      pctEl.textContent = txt;
    }
  }

  var gpuHistories = {};
  var hdHistories = {};

  function getHistory(key) {
    if (cfg.style !== "graph") return null;
    if (!gpuHistories[key]) {
      gpuHistories[key] = new Float64Array(cfg.history_size);
      gpuHistories[key].fill(0);
    }
    return gpuHistories[key];
  }

  function getHdHistory(key) {
    if (cfg.style !== "graph") return null;
    if (!hdHistories[key]) {
      hdHistories[key] = new Float64Array(cfg.history_size);
      hdHistories[key].fill(0);
    }
    return hdHistories[key];
  }

  function updateUI(data) {
    var cpu = data.cpu_percent || 0;
    var ghz = data.cpu_ghz || 0;
    var rUsed = data.ram_used || 0;
    var rTotal = data.ram_total || 0;
    var rPct = data.ram_percent || 0;
    var gpuArr = (data.gpu || []).filter(function (g, i) {
      if (cfg.selected_gpus.length === 0) return i === 0;
      return cfg.selected_gpus.indexOf(g.name) !== -1;
    });
    var hdArr = (data.hd || []).filter(function (h) {
      if (cfg.selected_hds.length === 0) return h.mount === "C:";
      return cfg.selected_hds.indexOf(h.mount) !== -1;
    });
    var netArr = (data.network || []).filter(function (n, i) {
      if (cfg.selected_networks.length === 0) return i === 0;
      return cfg.selected_networks.indexOf(n.name) !== -1;
    });

    var isPct = cfg.format === "percent";

    // CPU
    if (cpuEl) {
      var txt = cpuText(cpu, ghz);
      if (cpuPctEl) setRowPct(cpuPctEl, txt, isPct);
    }
    if (cpuFillEl) {
      cpuFillEl.style.width = cpu + "%";
      cpuFillEl.className = "sys-fill " + heatClass(cpu);
    }
    if (cpuDots) updateDots(cpuDots, cpu, 10);
    if (cpuHistory) pushHistory(cpuHistory, cpu);
    if (cpuGraphPath && cpuHistory) updateGraph(cpuGraphPath, cpuHistory, cpu);

    // RAM
    if (ramEl) {
      var txt = formatVal(rPct, rUsed, rTotal);
      if (ramPctEl) setRowPct(ramPctEl, txt, isPct);
    }
    if (ramFillEl) {
      ramFillEl.style.width = rPct + "%";
      ramFillEl.className = "sys-fill " + heatClass(rPct);
    }
    if (ramDots) updateDots(ramDots, rPct, 10);
    if (ramHistory) pushHistory(ramHistory, rPct);
    if (ramGraphPath && ramHistory) updateGraph(ramGraphPath, ramHistory, rPct);

    // Dynamic rows: remove old [data-group] and rebuild
    var old = wrap.querySelectorAll('[data-group]');
    for (var i = 0; i < old.length; i++) old[i].remove();

    if (cfg.show_gpu) {
      for (var i = 0; i < gpuArr.length; i++) {
        var g = gpuArr[i];
        var r = document.createElement("span");
        r.className = "sys-row";
        r.dataset.group = "gpu";
        r.innerHTML = rowHtml(g.name, cfg.style);
        wrap.append(r);

        var pctEl = r.querySelector(".sys-pct");
        var fillEl = r.querySelector(".sys-fill");
        var dotsEl = r.querySelector(".sys-dots");
        var graphEl = r.querySelector(".sys-graph");
        var graphPath = r.querySelector(".sys-graph path");

        if (dotsEl) { buildDots(dotsEl, 10); updateDots(dotsEl, g.percent, 10); }
        if (fillEl) {
          fillEl.style.width = g.percent + "%";
          fillEl.className = "sys-fill " + heatClass(g.percent);
        }
        if (graphEl && graphPath) {
          var hist = getHistory("gpu_" + i);
          pushHistory(hist, g.percent);
          updateGraph(graphPath, hist, g.percent);
        }

        var txt = cpuText(g.percent, 0);
        setRowPct(pctEl, txt, isPct);
      }
    }

    if (cfg.show_hd) {
      for (var i = 0; i < hdArr.length; i++) {
        var h = hdArr[i];
        var r = document.createElement("span");
        r.className = "sys-row";
        r.dataset.group = "hd";
        r.innerHTML = rowHtml(h.mount, cfg.style);
        wrap.append(r);

        var pctEl = r.querySelector(".sys-pct");
        var fillEl = r.querySelector(".sys-fill");
        var dotsEl = r.querySelector(".sys-dots");
        var graphEl = r.querySelector(".sys-graph");
        var graphPath = r.querySelector(".sys-graph path");

        if (dotsEl) { buildDots(dotsEl, 10); updateDots(dotsEl, h.percent, 10); }
        if (fillEl) {
          fillEl.style.width = h.percent + "%";
          fillEl.className = "sys-fill " + heatClass(h.percent);
        }
        if (graphEl && graphPath) {
          var hist = getHdHistory("hd_" + i);
          pushHistory(hist, h.percent);
          updateGraph(graphPath, hist, h.percent);
        }

        var txt = formatVal(h.percent, h.used, h.total);
        setRowPct(pctEl, txt, isPct);
      }
    }

    if (cfg.show_network) {
      for (var i = 0; i < netArr.length; i++) {
        var n = netArr[i];
        var r = document.createElement("span");
        r.className = "sys-row sys-net-row";
        r.dataset.group = "network";
        r.title = n.name;
        r.innerHTML =
          '<span class="sys-net">' +
          '<span class="sys-net-item sys-net-send">' +
          '<i class="sys-net-icon" data-icon="chevron-up" data-size="10"></i>' +
          '<span class="sys-net-val">' + formatRate(n.send_bps) + "</span>" +
          "</span>" +
          '<span class="sys-net-sep"></span>' +
          '<span class="sys-net-item sys-net-recv">' +
          '<i class="sys-net-icon" data-icon="chevron-down" data-size="10"></i>' +
          '<span class="sys-net-val">' + formatRate(n.recv_bps) + "</span>" +
          "</span>" +
          "</span>";
        wrap.append(r);
        if (applyIcons)applyIcons(r);
      }
    }
  }

  loadConfig();

  var pollTimer = null;

  function refresh() {
    if (!wrap) return;
    invoke("get_system_stats")
      .then(function (data) { updateUI(data); })
      .catch(function () {});
  }

  function startPoll() {
    refresh();
    var ms = Math.max(1000, Math.min(10000, cfg.refresh_seconds * 1000));
    pollTimer = setInterval(refresh, ms);
  }

  startPoll();

  var observer = new MutationObserver(function () {
    if (!document.contains(el)) {
      if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
      observer.disconnect();
    }
  });
  observer.observe(document.body, { childList: true, subtree: true });
})();
