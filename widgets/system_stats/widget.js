(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  if (!invoke) return;

  var root = el.querySelector(".sys-root");
  if (!root) return;

  var cfg = {
    style: "bar",
    format: "percent",
    show_cpu: true,
    show_ram: true,
    show_gpu: true,
    show_hd: true,
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
          if (wc.refresh_seconds >= 1 && wc.refresh_seconds <= 10)
            cfg.refresh_seconds = wc.refresh_seconds;
          if (wc.history_size >= 5 && wc.history_size <= 40)
            cfg.history_size = wc.history_size;
        }
        buildUI();
      }).catch(function () {});
    } catch (_) {}
  }

  var cpuEl, ramEl, gpuEl, hdEl;
  var cpuFillEl, ramFillEl, gpuFillEl, hdFillEl;
  var cpuPctEl, ramPctEl, gpuPctEl, hdPctEl;
  var cpuDots, ramDots, gpuDots, hdDots;
  var cpuGraphEl, ramGraphEl, gpuGraphEl, hdGraphEl;
  var cpuHistory, ramHistory, gpuHistory, hdHistory;
  var cpuGraphPath, ramGraphPath, gpuGraphPath, hdGraphPath;

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

  function heatClass(pct) {
    if (pct >= 85) return "is-hot";
    if (pct >= 60) return "is-warn";
    return "";
  }

  function buildUI() {
    root.dataset.style = cfg.style;

    if (cfg.style === "dots") {
      root.innerHTML = '<span class="sys-wrap"></span>';
      var w = root.firstElementChild;

      if (cfg.show_cpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">CPU</span><span class="sys-dots"></span><span class="sys-pct"></span>';
        w.append(r);
        cpuEl = r;
        cpuDots = r.querySelector(".sys-dots");
        cpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_ram) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">RAM</span><span class="sys-dots"></span><span class="sys-pct"></span>';
        w.append(r);
        ramEl = r;
        ramDots = r.querySelector(".sys-dots");
        ramPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_gpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">GPU</span><span class="sys-dots"></span><span class="sys-pct"></span>';
        w.append(r);
        gpuEl = r;
        gpuDots = r.querySelector(".sys-dots");
        gpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_hd) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">HD</span><span class="sys-dots"></span><span class="sys-pct"></span>';
        w.append(r);
        hdEl = r;
        hdDots = r.querySelector(".sys-dots");
        hdPctEl = r.querySelector(".sys-pct");
      }

      var n = 10;
      if (cpuDots) buildDots(cpuDots, n);
      if (ramDots) buildDots(ramDots, n);
      if (gpuDots) buildDots(gpuDots, n);
      if (hdDots) buildDots(hdDots, n);

    } else if (cfg.style === "graph") {
      root.innerHTML = '<span class="sys-wrap"></span>';
      var w = root.firstElementChild;
      cpuHistory = new Float64Array(cfg.history_size);
      cpuHistory.fill(0);
      ramHistory = new Float64Array(cfg.history_size);
      ramHistory.fill(0);
      gpuHistory = new Float64Array(cfg.history_size);
      gpuHistory.fill(0);
      hdHistory = new Float64Array(cfg.history_size);
      hdHistory.fill(0);

      if (cfg.show_cpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">CPU</span><svg class="sys-graph" preserveAspectRatio="none" viewBox="0 0 100 16"><path></path></svg><span class="sys-pct"></span>';
        w.append(r);
        cpuEl = r;
        cpuGraphEl = r.querySelector(".sys-graph");
        cpuGraphPath = cpuGraphEl.querySelector("path");
        cpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_ram) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">RAM</span><svg class="sys-graph" preserveAspectRatio="none" viewBox="0 0 100 16"><path></path></svg><span class="sys-pct"></span>';
        w.append(r);
        ramEl = r;
        ramGraphEl = r.querySelector(".sys-graph");
        ramGraphPath = ramGraphEl.querySelector("path");
        ramPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_gpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">GPU</span><svg class="sys-graph" preserveAspectRatio="none" viewBox="0 0 100 16"><path></path></svg><span class="sys-pct"></span>';
        w.append(r);
        gpuEl = r;
        gpuGraphEl = r.querySelector(".sys-graph");
        gpuGraphPath = gpuGraphEl.querySelector("path");
        gpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_hd) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">HD</span><svg class="sys-graph" preserveAspectRatio="none" viewBox="0 0 100 16"><path></path></svg><span class="sys-pct"></span>';
        w.append(r);
        hdEl = r;
        hdGraphEl = r.querySelector(".sys-graph");
        hdGraphPath = hdGraphEl.querySelector("path");
        hdPctEl = r.querySelector(".sys-pct");
      }

    } else {
      root.innerHTML = '<span class="sys-wrap"></span>';
      var w = root.firstElementChild;

      if (cfg.show_cpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">CPU</span><span class="sys-bar"><span class="sys-fill"></span></span><span class="sys-pct"></span>';
        w.append(r);
        cpuEl = r;
        cpuFillEl = r.querySelector(".sys-fill");
        cpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_ram) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">RAM</span><span class="sys-bar"><span class="sys-fill"></span></span><span class="sys-pct"></span>';
        w.append(r);
        ramEl = r;
        ramFillEl = r.querySelector(".sys-fill");
        ramPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_gpu) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">GPU</span><span class="sys-bar"><span class="sys-fill"></span></span><span class="sys-pct"></span>';
        w.append(r);
        gpuEl = r;
        gpuFillEl = r.querySelector(".sys-fill");
        gpuPctEl = r.querySelector(".sys-pct");
      }

      if (cfg.show_hd) {
        var r = document.createElement("span");
        r.className = "sys-row";
        r.innerHTML = '<span class="sys-label">HD</span><span class="sys-bar"><span class="sys-fill"></span></span><span class="sys-pct"></span>';
        w.append(r);
        hdEl = r;
        hdFillEl = r.querySelector(".sys-fill");
        hdPctEl = r.querySelector(".sys-pct");
      }
    }
  }

  function buildDots(parent, n) {
    for (var i = 0; i < n; i++) {
      var d = document.createElement("span");
      d.className = "sys-dot";
      parent.append(d);
    }
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

  function updateUI(data) {
    var cpu = data.cpu_percent || 0;
    var ghz = data.cpu_ghz || 0;
    var rUsed = data.ram_used || 0;
    var rTotal = data.ram_total || 0;
    var rPct = data.ram_percent || 0;
    var gpu = data.gpu_percent || 0;
    var hUsed = data.hd_used || 0;
    var hTotal = data.hd_total || 0;
    var hPct = data.hd_percent || 0;

    if (cpuEl) {
      var txt = cpuText(cpu, ghz);
      if (cpuPctEl) {
        if (cfg.format === "percent") {
          cpuPctEl.innerHTML = txt + '<span class="sys-pct-suffix">%</span>';
        } else {
          cpuPctEl.textContent = txt;
        }
      }
    }

    if (ramEl) {
      var txt = formatVal(rPct, rUsed, rTotal);
      if (ramPctEl) {
        if (cfg.format === "percent") {
          ramPctEl.innerHTML = txt + '<span class="sys-pct-suffix">%</span>';
        } else {
          ramPctEl.textContent = txt;
        }
      }
    }

    if (gpuEl) {
      var txt = cpuText(gpu, 0);
      if (gpuPctEl) {
        if (cfg.format === "percent") {
          gpuPctEl.innerHTML = txt + '<span class="sys-pct-suffix">%</span>';
        } else {
          gpuPctEl.textContent = txt;
        }
      }
    }

    if (hdEl) {
      var txt = formatVal(hPct, hUsed, hTotal);
      if (hdPctEl) {
        if (cfg.format === "percent") {
          hdPctEl.innerHTML = txt + '<span class="sys-pct-suffix">%</span>';
        } else {
          hdPctEl.textContent = txt;
        }
      }
    }

    if (cpuFillEl) {
      cpuFillEl.style.width = cpu + "%";
      cpuFillEl.className = "sys-fill " + heatClass(cpu);
    }

    if (ramFillEl) {
      ramFillEl.style.width = rPct + "%";
      ramFillEl.className = "sys-fill " + heatClass(rPct);
    }

    if (gpuFillEl) {
      gpuFillEl.style.width = gpu + "%";
      gpuFillEl.className = "sys-fill " + heatClass(gpu);
    }

    if (hdFillEl) {
      hdFillEl.style.width = hPct + "%";
      hdFillEl.className = "sys-fill " + heatClass(hPct);
    }

    if (cpuDots) updateDots(cpuDots, cpu, 10);
    if (ramDots) updateDots(ramDots, rPct, 10);
    if (gpuDots) updateDots(gpuDots, gpu, 10);
    if (hdDots) updateDots(hdDots, hPct, 10);

    if (cpuHistory) pushHistory(cpuHistory, cpu);
    if (ramHistory) pushHistory(ramHistory, rPct);
    if (gpuHistory) pushHistory(gpuHistory, gpu);
    if (hdHistory) pushHistory(hdHistory, hPct);
    if (cpuGraphPath && cpuHistory) updateGraph(cpuGraphEl, cpuHistory, cpu);
    if (ramGraphPath && ramHistory) updateGraph(ramGraphEl, ramHistory, rPct);
    if (gpuGraphPath && gpuHistory) updateGraph(gpuGraphEl, gpuHistory, gpu);
    if (hdGraphPath && hdHistory) updateGraph(hdGraphEl, hdHistory, hPct);
  }

  loadConfig();

  var pollTimer = null;

  function refresh() {
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
