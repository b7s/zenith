(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  if (!invoke) return;

  var root = el && el.querySelector(".al-root");
  var chip = root && root.querySelector(".al-chip");
  var chipIc = root && root.querySelector(".al-chip-ic");
  var info = root && root.querySelector(".al-info");
  var nextEl = info && info.querySelector(".al-next");
  var labelEl = info && info.querySelector(".al-label");
  var removeEl = root && root.querySelector(".al-remove");

  var showLabel = true;
  var showWhenNoAlarms = true;
  var rotateUpcoming = true;
  var rotateSeconds = 30;

  if (chip) {
    chip.setAttribute("title", "Open calendar");
    chip.setAttribute("aria-label", "Open calendar");
  }

  // Pre-create the "soon" dot inside `.al-next` so the layout is stable
  // and the dot toggles via class only on `render()`.
  if (nextEl && !nextEl.querySelector(".al-soon-dot")) {
    var soonDot = document.createElement("span");
    soonDot.className = "al-soon-dot";
    soonDot.setAttribute("aria-hidden", "true");
    nextEl.appendChild(soonDot);
  }

  if (chipIc && window.__zenith_setIcon) {
    window.__zenith_setIcon(chipIc, "alarm-clock", { size: 12 });
  }

  if (root) {
    root.addEventListener("click", function () {
      if (document.body.classList.contains("is-arranging")) return;
      var rect = el.getBoundingClientRect();
      invoke("open_calendar", {
        x: rect.left,
        y: rect.bottom,
        wide: false,
        single: true,
        mode: "events",
      }).catch(function () {});
    });
  }

  function pad(n) { return n < 10 ? "0" + n : "" + n; }

  function nextOccurrence(ev) {
    var d = ev.date.split("-");
    var yy = parseInt(d[0], 10);
    var mo = parseInt(d[1], 10) - 1;
    var dd = parseInt(d[2], 10);
    if (isNaN(yy) || isNaN(mo) || isNaN(dd)) return null;
    var hasTime = !!ev.time;
    var hh = 0, mm = 0;
    if (hasTime) {
      var parts = ev.time.split(":");
      hh = parseInt(parts[0], 10);
      mm = parseInt(parts[1], 10);
      if (isNaN(hh) || isNaN(mm)) return null;
    }
    var now = new Date();
    var base = new Date(yy, mo, dd, hh, mm, 0, 0);
    if (ev.recurrence === "none") {
      // All-day events have no time — keep them on the bar from start of
      // their day until end of day.
      if (!hasTime) return base >= now ? base : new Date(yy, mo, dd, 23, 59, 59, 999);
      // Future one-shot — show its scheduled moment.
      if (base >= now) return base;
      // Past one-shot — only keep visible during the late grace window
      // (7 min). Beyond that, the event is "missed" and drops out of the
      // bar so the next upcoming entry can take its slot.
      var lateMs = now.getTime() - base.getTime();
      if (lateMs <= LATE_WINDOW_MS) return base;
      return null;
    } else if (ev.recurrence === "daily") {
      var c = new Date(now);
      c.setHours(hh, mm, 0, 0);
      if (c < now) c.setDate(c.getDate() + 1);
      return c;
    } else if (ev.recurrence === "weekly") {
      if (!ev.weekdays) return null;
      var w = new Date(now);
      w.setHours(hh, mm, 0, 0);
      for (var i = 0; i < 14; i++) {
        if ((ev.weekdays & (1 << w.getDay())) !== 0 && w >= now) return w;
        w.setDate(w.getDate() + 1);
      }
      return null;
    } else if (ev.recurrence === "monthly") {
      var m = new Date(now);
      m.setDate(dd);
      m.setHours(hh, mm, 0, 0);
      if (m < now) m.setMonth(m.getMonth() + 1);
      return m;
    }
    return null;
  }

  var SOON_WINDOW_MS = 6 * 60 * 1000;   // event within 6 minutes → show dot
  var LATE_WINDOW_MS = 7 * 60 * 1000;   // event up to 7 min past → keep visible
  var rotateTimer = null;
  var currentIndex = 0;
  var alarms = [];
  // Identity of the entry last painted — used to bail out the rotation
  // animation when the underlying event hasn't actually changed (e.g. a
  // 30s `update()` poll firing while rotation is also live).
  var lastRenderedId = null;
  // ID of the alarm currently shown on the bar — the one the remove button
  // deletes. Cleared when no alarm is shown so the button is inert.
  var currentAlarmId = null;

  // Remove the alarm currently displayed on the bar. Deleting it from the
  // event store also stops the Windows alarm (the alarm-firing thread scans
  // the store on its next tick, so a removed event never fires/pops up).
  if (removeEl) {
    removeEl.textContent = "×";
    removeEl.addEventListener("click", function (e) {
      e.stopPropagation();
      if (document.body.classList.contains("is-arranging")) return;
      var id = currentAlarmId;
      if (!id) return;
      var inv = window.__zenith_invoke;
      if (!inv) return;
      inv("delete_event", { id: id }).then(function () {
        update();
      }).catch(function () {
        update();
      });
    });
  }

  // React immediately to changes made elsewhere (calendar add/edit/delete)
  // instead of waiting up to 30s for the next poll.
  if (window.__zenith_listen) {
    window.__zenith_listen("zenith:events-updated", function () {
      update();
    });
  }

  function rotateSchedule() {
    if (rotateTimer) {
      clearInterval(rotateTimer);
      rotateTimer = null;
    }
    if (!rotateUpcoming || alarms.length < 2) return;
    var seconds = Math.max(1, rotateSeconds | 0);
    rotateTimer = setInterval(function () {
      if (!rotateUpcoming || alarms.length < 2) {
        clearInterval(rotateTimer);
        rotateTimer = null;
        return;
      }
      currentIndex = (currentIndex + 1) % alarms.length;
      render();
    }, seconds * 1000);
  }

  function render() {
    if (root) root.classList.toggle("has-alarm", alarms.length > 0);
    if (alarms.length === 0) {
      if (nextEl) nextEl.textContent = "";
      if (labelEl) labelEl.textContent = "";
      if (info) info.style.display = "none";
      el.style.display = showWhenNoAlarms ? "" : "none";
      lastRenderedId = null;
      currentAlarmId = null;
      return;
    }
    el.style.display = "";
    if (info) info.style.display = "";

    var next = alarms[currentIndex] || alarms[0];
    var entryChanged = lastRenderedId !== next.ev.id;
    currentAlarmId = next.ev.id;
    lastRenderedId = next.ev.id;

    var hasTime = !!next.ev.time;
    var timeLabel = hasTime
      ? pad(next.at.getHours()) + ":" + pad(next.at.getMinutes())
      : "All Day";
    var titleLabel = next.ev.title || "";
    var hasTitle = showLabel && !!titleLabel;

    if (nextEl) nextEl.textContent = timeLabel;
    if (labelEl) {
      if (hasTitle) {
        labelEl.textContent = titleLabel;
        labelEl.style.display = "";
      } else {
        labelEl.style.display = "none";
      }
    }
    // Collapse the info width when no label is rendered so the chip
    // doesn't carry an empty 12rem slot next to the time.
    if (info) {
      info.classList.toggle("is-time-only", !hasTitle);
    }
    var total = alarms.length;
    var dot = total > 1 ? " (" + (currentIndex + 1) + "/" + total + ")" : "";
    el.title = hasTitle
      ? timeLabel + " \u00b7 " + titleLabel + dot
      : timeLabel + dot;
    if (info) info.removeAttribute("title");

    // Soon dot: only when this event fires within 6 minutes from now
    // (alarm clock + 6 min countdown cue).
    var msUntil = next.at.getTime() - Date.now();
    var soon = msUntil >= 0 && msUntil <= SOON_WINDOW_MS;
    var soonDot = nextEl && nextEl.querySelector(".al-soon-dot");
    if (soonDot) {
      soonDot.classList.toggle("is-on", !!soon);
      soonDot.setAttribute("aria-hidden", soon ? "false" : "true");
    }

    if (info) {
      // Drive the entrance animation only when the queue is live (≥2) AND
      // the underlying event genuinely changed. Same-id re-renders (e.g.
      // the 30s `update()` poll) leave the animation untouched — stripping
      // the enter class mid-keyframe is what caused the "sometimes the
      // transition does not fire" glitch.
      var rotating = rotateUpcoming && alarms.length > 1 && entryChanged;
      if (alarms.length < 2 && rotateTimer) {
        clearInterval(rotateTimer);
        rotateTimer = null;
      }
      if (rotating) {
        // Stagger: time animates immediately, title follows 150ms later.
        // Both spans share the same keyframe; the delay is set per class
        // in CSS. Force reflow before re-adding so the animation restarts.
        if (nextEl) {
          nextEl.classList.remove("al-next--enter");
          void nextEl.offsetWidth;
          nextEl.classList.add("al-next--enter");
        }
        if (labelEl) {
          labelEl.classList.remove("al-label--enter");
          void labelEl.offsetWidth;
          labelEl.classList.add("al-label--enter");
        }
      } else if (alarms.length < 2) {
        // Queue collapsed — clear any leftover animation classes so the
        // single entry sits still.
        if (nextEl) nextEl.classList.remove("al-next--enter");
        if (labelEl) labelEl.classList.remove("al-label--enter");
      }
    }
  }

  function update() {
    invoke("get_events").then(function (events) {
      if (!events || events.length === 0) {
        alarms = [];
        if (rotateTimer) { clearInterval(rotateTimer); rotateTimer = null; }
        currentIndex = 0;
        render();
        return;
      }
      var list = events.filter(function (e) {
        return e.enabled !== false;
      }).map(function (e) {
        return { ev: e, at: nextOccurrence(e) };
      }).filter(function (x) {
        return x.at !== null;
      });

      list.sort(function (a, b) {
        return a.at.getTime() - b.at.getTime();
      });

      alarms = list.slice(0, 5);
      if (currentIndex >= alarms.length) currentIndex = 0;
      if (alarms.length === 0) {
        if (rotateTimer) { clearInterval(rotateTimer); rotateTimer = null; }
        render();
        return;
      }
      // Avoid resetting the rotation timer on every 30s poll — restarting
      // it would cancel a pending tick and the bar would never visibly
      // advance. We only restart when `loadConfig()` runs (config change).
      if (!rotateTimer && rotateUpcoming && alarms.length > 1) {
        rotateSchedule();
      }
      render();
    }).catch(function () {
      alarms = [];
      if (rotateTimer) { clearInterval(rotateTimer); rotateTimer = null; }
      currentIndex = 0;
      render();
    });
  }

  function loadConfig() {
    invoke("get_config").then(function (cfg) {
      var wc = (cfg.widgets && cfg.widgets.config && cfg.widgets.config["alarms"]) || {};
      showLabel = wc.show_label !== false;
      showWhenNoAlarms = wc.show_when_no_alarms !== false;
      rotateUpcoming = wc.rotate_upcoming !== false;
      var rs = parseInt(wc.rotate_seconds, 10);
      rotateSeconds = isFinite(rs) && rs >= 1 ? rs : 15;
      if (rotateTimer) { clearInterval(rotateTimer); rotateTimer = null; }
      rotateSchedule();
      update();
    }).catch(function () {
      update();
    });
  }

  loadConfig();
  update();
  setInterval(update, 30000);
})();
