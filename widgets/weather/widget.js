(function () {
  var el = document.currentScript && document.currentScript.parentElement;
  if (!el) return;

  var invoke = window.__zenith_invoke;
  var applyIcons = window.__zenith_applyIcons;
  var weatherIconFn = window.__zenith_weatherIcon;
  var openWeather = window.__zenith_openWeather;
  if (!invoke || !weatherIconFn || !openWeather) return;

  var wrap = el.querySelector(".wx-wrap");
  var tempEl = el.querySelector(".wx-temp");
  var iconEl = el.querySelector(".wx-ic");
  if (!wrap || !tempEl || !iconEl) return;

  var lastGoodSnapshot = null;

  // Called on mount: read cached snapshot from Rust (fast, no API call).
  function loadCache() {
    invoke("weather_get_cache")
      .then(function (snap) {
        if (snap && snap.ok && snap.current) {
          lastGoodSnapshot = snap;
          render(snap);
        }
      })
      .catch(function () { /* ignore */ });
  }

  // Fetch latest weather via the Rust backend (handles DPAPI key + cache).
  function refresh() {
    invoke("weather_refresh")
      .then(function (snap) {
        if (snap && snap.ok) {
          lastGoodSnapshot = snap;
          render(snap);
        } else {
          // Error — keep showing last good data, but mark as error.
          wrap.classList.add("is-error");
          if (snap?.error) {
            wrap.title = "Weather error: " + snap.error;
          }
        }
      })
      .catch(function (e) {
        wrap.classList.add("is-error");
        wrap.title = "Weather error: " + e;
      });
  }

  function render(snap) {
    wrap.classList.remove("is-error", "is-loading");
    wrap.title = "";

    var current = snap.current;
    var daily = snap.daily;
    var units = snap.units || "metric";
    var city = snap.city || "";

    if (!current) return;

    var code = current.weather?.[0]?.id;
    var iconCode = current.weather?.[0]?.icon;
    var name = weatherIconFn(code, iconCode);
    if (typeof window.__zenith_setIcon === "function") {
      window.__zenith_setIcon(iconEl, name, { size: 16 });
    } else {
      iconEl.dataset.icon = name;
      iconEl.dataset.size = "16";
      if (applyIcons) applyIcons(iconEl);
    }

    var temp = current.temp;
    if (temp !== undefined && temp !== null) {
      tempEl.textContent = Math.round(temp) + "\u00B0";
    }

    // Tooltip with summary
    var desc = current.weather?.[0]?.description || "";
    var feels = current.feels_like !== undefined ? "Feels " + Math.round(current.feels_like) + "\u00B0" : "";
    var hum = current.humidity !== undefined ? "Humidity " + current.humidity + "%" : "";
    var wind = current.wind_speed !== undefined ? "Wind " + Math.round(current.wind_speed) + (units === "imperial" ? " mph" : " m/s") : "";
    var parts = [city, desc, feels, hum, wind].filter(Boolean);
    wrap.title = parts.join(" | ");
  }

  // Click opens the forecast window (skip when arranging)
  wrap.addEventListener("click", function () {
    if (document.body.classList.contains("is-arranging")) return;
    try { openWeather(wrap); } catch (err) { /* swallow */ }
  });

  loadCache();
  refresh();
  setInterval(refresh, 1800000); // 30 min default; config change picked up on next run
})();