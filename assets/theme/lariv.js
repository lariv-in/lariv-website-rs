(function () {
  if (window.__larivThemeBound) return;
  window.__larivThemeBound = true;

  var workerSteps = [1, 50, 500];
  var sizeSteps = ["small", "medium", "large"];
  var rawBenchmarkData = null;
  var currentCategoryFilter = "crud";
  var currentWorkerFilter = 1;
  var wsReqFilter = "small";
  var wsRespFilter = "small";

  function initHeaderScroll() {
    var header = document.querySelector(".site-header, .gjs-navbar.site-header, #header");
    if (!header || header.dataset.larivScrollBound) return;
    header.dataset.larivScrollBound = "true";
    window.addEventListener("scroll", function () {
      header.classList.toggle("scrolled", window.scrollY > 50);
    });
  }

  function initMobileNav() {
    document.querySelectorAll('[data-gjs-type="p_website.navbar"], .gjs-navbar').forEach(function (nav) {
      var toggleBtn = nav.querySelector(".mobile-nav-toggle");
      var navMenu = nav.querySelector(".gjs-navbar-links");
      if (!toggleBtn || !navMenu || toggleBtn.dataset.bound) return;
      toggleBtn.dataset.bound = "true";
      toggleBtn.addEventListener("click", function (e) {
        e.stopPropagation();
        var open = !navMenu.classList.contains("open");
        navMenu.classList.toggle("open", open);
        toggleBtn.classList.toggle("open", open);
        toggleBtn.setAttribute("aria-expanded", String(open));
      });
      document.addEventListener("click", function (e) {
        if (!navMenu.contains(e.target) && !toggleBtn.contains(e.target)) {
          navMenu.classList.remove("open");
          toggleBtn.classList.remove("open");
          toggleBtn.setAttribute("aria-expanded", "false");
        }
      });
      navMenu.querySelectorAll("a").forEach(function (link) {
        link.addEventListener("click", function () {
          navMenu.classList.remove("open");
          toggleBtn.classList.remove("open");
          toggleBtn.setAttribute("aria-expanded", "false");
        });
      });
    });
  }

  async function fetchLatestVersion() {
    try {
      var response = await fetch("https://api.github.com/repos/lariv-in/lariv-rs/tags");
      if (!response.ok) return;
      var tags = await response.json();
      if (!tags || !tags.length) return;
      var badge = document.querySelector(".badge[data-version-badge], .hero-content .badge, .gjs-hero .badge");
      if (badge) badge.textContent = "Framework " + tags[0].name;
    } catch (err) {
      console.error("Failed to fetch latest tag:", err);
    }
  }

  function avgRps(stats) {
    if (!stats) return 0;
    var value = stats.AvgRps != null ? stats.AvgRps : stats.AvgRPS;
    return Math.round(Number(value) || 0);
  }

  function avgLatencyMs(stats) {
    if (!stats || stats.AvgLatency == null) return 0;
    return Number(stats.AvgLatency) / 1e6;
  }

  function categoryData(category) {
    if (!rawBenchmarkData) return [];
    var keys = [category, String(category).toLowerCase(), String(category).toUpperCase()];
    for (var i = 0; i < keys.length; i++) {
      if (Array.isArray(rawBenchmarkData[keys[i]])) return rawBenchmarkData[keys[i]];
    }
    return [];
  }

  function wsStageName() {
    return "WS_" + wsReqFilter + "_req_" + wsRespFilter + "_resp";
  }

  function syncCategoryTabs() {
    document.querySelectorAll(".perf-cat-tab").forEach(function (btn) {
      var cat = btn.getAttribute("data-perf-category");
      btn.classList.toggle("active", cat === currentCategoryFilter);
    });
  }

  function setStageControlsVisible(visible) {
    var el = document.getElementById("perf-stage-controls");
    if (!el) return;
    if (!visible) {
      el.style.display = "none";
      el.innerHTML = "";
      return;
    }
    el.style.display = "flex";
    el.innerHTML =
      '<div class="perf-stage-row">' +
      '<span class="perf-stage-label">Request size</span>' +
      '<label class="perf-slider-label">Req: <span id="ws-req-label">' +
      capitalize(wsReqFilter) +
      "</span></label>" +
      '<input type="range" id="ws-req-slider" min="0" max="2" value="' +
      sizeSteps.indexOf(wsReqFilter) +
      '" step="1" class="perf-slider">' +
      "</div>" +
      '<div class="perf-stage-row">' +
      '<span class="perf-stage-label">Response size</span>' +
      '<label class="perf-slider-label">Resp: <span id="ws-resp-label">' +
      capitalize(wsRespFilter) +
      "</span></label>" +
      '<input type="range" id="ws-resp-slider" min="0" max="2" value="' +
      sizeSteps.indexOf(wsRespFilter) +
      '" step="1" class="perf-slider">' +
      "</div>";
    var reqSlider = document.getElementById("ws-req-slider");
    var respSlider = document.getElementById("ws-resp-slider");
    if (reqSlider) {
      reqSlider.addEventListener("input", function () {
        wsReqFilter = sizeSteps[parseInt(reqSlider.value, 10)] || "small";
        var label = document.getElementById("ws-req-label");
        if (label) label.textContent = capitalize(wsReqFilter);
        renderPerfCharts();
      });
    }
    if (respSlider) {
      respSlider.addEventListener("input", function () {
        wsRespFilter = sizeSteps[parseInt(respSlider.value, 10)] || "small";
        var label = document.getElementById("ws-resp-label");
        if (label) label.textContent = capitalize(wsRespFilter);
        renderPerfCharts();
      });
    }
  }

  function capitalize(s) {
    return s ? s.charAt(0).toUpperCase() + s.slice(1) : "";
  }

  function emptyBars(message) {
    return '<div style="color: var(--text-dark); padding: 2rem 0; text-align: center;">' + message + "</div>";
  }

  function renderBarList(container, list, valueKey, suffix, maxValue) {
    if (!container) return;
    container.innerHTML = list
      .map(function (item) {
        var value = valueKey === "rps" ? item.rps : parseFloat(item.latencyMs);
        var pct = Math.min(100, Math.round((value / maxValue) * 100));
        var fillClass = item.isLariv ? "bar-fill-lariv" : "bar-fill-other";
        var label = valueKey === "rps" ? item.rps.toLocaleString() + " req/s" : item.latencyMs + " ms";
        return (
          '<div class="bar-item"><div class="bar-header"><span>' +
          item.name +
          "</span><span>" +
          label +
          '</span></div><div class="bar-track"><div class="bar-fill ' +
          fillClass +
          '" style="width: ' +
          pct +
          '%;"></div></div></div>'
        );
      })
      .join("");
  }

  function renderPerfCharts() {
    if (!document.getElementById("performance")) return;
    if (!rawBenchmarkData) return;

    var entries = categoryData(currentCategoryFilter).filter(function (item) {
      return item.workers === currentWorkerFilter;
    });
    var isWs = currentCategoryFilter.toLowerCase() === "websocket";
    setStageControlsVisible(isWs);
    if (isWs) {
      var stage = wsStageName();
      entries = entries.filter(function (item) {
        return item.stage === stage;
      });
    }

    var descEl = document.getElementById("perf-category-desc");
    if (descEl) {
      var catLower = currentCategoryFilter.toLowerCase();
      if (catLower === "counter") {
        descEl.innerHTML = "JSON unmarshaling &rarr; integer increment &rarr; JSON marshaling";
      } else if (catLower === "crud") {
        descEl.innerHTML = "Database workload mix: Create, Update, List, and View Single record";
      } else if (catLower === "task") {
        descEl.innerHTML = "Submitting AI task to a queue &rarr; Waiting for AI task execution to return";
      } else if (catLower === "websocket") {
        descEl.innerHTML =
          "WebSocket round-trip: " +
          capitalize(wsReqFilter) +
          " request / " +
          capitalize(wsRespFilter) +
          " response";
      } else {
        descEl.innerHTML = "";
      }
    }

    var rpsContainer = document.getElementById("rps-bars-container");
    var latencyContainer = document.getElementById("latency-bars-container");
    if (entries.length === 0) {
      if (rpsContainer) rpsContainer.innerHTML = emptyBars("No benchmark metrics available for this configuration.");
      if (latencyContainer) latencyContainer.innerHTML = emptyBars("No benchmark metrics available for this configuration.");
      return;
    }

    var targetMap = {};
    entries.forEach(function (item) {
      var name = String(item.target || "").replace(" Local Server", "");
      var mapKey = item.stage ? item.target + "_" + item.stage : name;
      if (!targetMap[mapKey] || (item.duration && String(item.duration).indexOf("1m") !== -1)) {
        targetMap[mapKey] = {
          name: name,
          isLariv: name.indexOf("Lariv") !== -1,
          rps: avgRps(item.stats),
          latencyMs: avgLatencyMs(item.stats).toFixed(2),
        };
      }
    });

    var list = Object.keys(targetMap).map(function (k) {
      return targetMap[k];
    });
    list.sort(function (a, b) {
      return a.isLariv === b.isLariv ? 0 : a.isLariv ? 1 : -1;
    });

    var maxRPS = Math.max.apply(
      null,
      list.map(function (i) {
        return i.rps;
      }).concat([1])
    );
    var maxLatency = Math.max.apply(
      null,
      list.map(function (i) {
        return parseFloat(i.latencyMs);
      }).concat([0.001])
    );

    renderBarList(rpsContainer, list, "rps", " req/s", maxRPS);
    renderBarList(latencyContainer, list, "latency", " ms", maxLatency);
  }

  async function fetchBenchmarkMetrics() {
    if (!document.getElementById("performance")) return;
    var sources = [];
    var seeded = document.getElementById("performance").getAttribute("data-benchmark-src");
    if (seeded) sources.push(seeded);
    sources.push("/static/benchmark_metrics.json");
    sources.push("https://raw.githubusercontent.com/UniquityVentures/benchmarks/main/benchmark_metrics.json");

    for (var i = 0; i < sources.length; i++) {
      try {
        var res = await fetch(sources[i]);
        if (!res.ok) continue;
        rawBenchmarkData = await res.json();
        break;
      } catch (err) {
        console.warn("Unable to fetch benchmark results from", sources[i], err);
      }
    }
    renderPerfCharts();
  }

  function bindPerfControls() {
    var root = document.getElementById("performance");
    if (!root || root.dataset.larivPerfBound) return;
    root.dataset.larivPerfBound = "true";

    document.querySelectorAll(".perf-cat-tab").forEach(function (btn) {
      btn.addEventListener("click", function () {
        currentCategoryFilter = btn.getAttribute("data-perf-category") || "crud";
        syncCategoryTabs();
        renderPerfCharts();
      });
    });

    var slider = document.getElementById("worker-slider");
    if (slider) {
      slider.addEventListener("input", function () {
        currentWorkerFilter = workerSteps[parseInt(slider.value, 10)] || 1;
        var label = document.getElementById("worker-count-label");
        if (label) {
          label.textContent = currentWorkerFilter + (currentWorkerFilter === 1 ? " Worker" : " Workers");
        }
        renderPerfCharts();
      });
    }
  }

  var COPY_ICON_SVG =
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>';
  var CHECK_ICON_SVG =
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>';
  var toastTimeout;

  function showCopyToast(message) {
    var toast = document.getElementById("copy-toast");
    var toastMsg = document.getElementById("copy-toast-message");
    if (!toast || !toastMsg) return;
    toastMsg.textContent = message;
    toast.classList.add("show");
    clearTimeout(toastTimeout);
    toastTimeout = setTimeout(function () {
      toast.classList.remove("show");
    }, 2500);
  }

  function performCopyAction(btnElement, textToCopy, toastMessage) {
    if (!btnElement || !navigator.clipboard) return;
    navigator.clipboard.writeText(textToCopy).then(function () {
      var originalHTML = btnElement.innerHTML;
      btnElement.classList.add("copied");
      btnElement.innerHTML = CHECK_ICON_SVG + " Copied!";
      showCopyToast(toastMessage || "Copied to clipboard!");
      setTimeout(function () {
        btnElement.classList.remove("copied");
        btnElement.innerHTML = originalHTML;
      }, 2200);
    });
  }

  function initArticleHelpers() {
    var progress = document.getElementById("readingProgress");
    if (progress && !progress.dataset.bound) {
      progress.dataset.bound = "true";
      window.addEventListener("scroll", function () {
        var totalHeight = document.documentElement.scrollHeight - window.innerHeight;
        var progressPercent = totalHeight > 0 ? (window.scrollY / totalHeight) * 100 : 0;
        progress.style.width = Math.min(100, Math.max(0, progressPercent)) + "%";
      });
    }

    var articleBody = document.querySelector(".article-body");
    var readTimeEl = document.getElementById("reading-time-display");
    if (articleBody && readTimeEl) {
      var text = articleBody.innerText || articleBody.textContent || "";
      var words = text.trim().split(/\s+/).filter(function (w) {
        return w.length > 0;
      }).length;
      readTimeEl.textContent = Math.max(1, Math.ceil(words / 200)) + " min read";
    }

    document.querySelectorAll("[data-copy-article]").forEach(function (btn) {
      if (btn.dataset.bound) return;
      btn.dataset.bound = "true";
      btn.addEventListener("click", function () {
        performCopyAction(btn, window.location.href, "Article link copied to clipboard!");
      });
    });

    if (articleBody && !articleBody.dataset.copyBound) {
      articleBody.dataset.copyBound = "true";
      articleBody.querySelectorAll("pre").forEach(function (pre) {
        var btn = document.createElement("button");
        btn.className = "code-copy-btn";
        btn.innerHTML = COPY_ICON_SVG + " Copy";
        btn.addEventListener("click", function (e) {
          e.stopPropagation();
          var code = pre.querySelector("code") || pre;
          performCopyAction(btn, code.innerText, "Code snippet copied to clipboard!");
        });
        pre.appendChild(btn);
      });
      articleBody.querySelectorAll("table").forEach(function (table) {
        if (!table.parentElement.classList.contains("table-wrapper")) {
          var wrapper = document.createElement("div");
          wrapper.className = "table-wrapper";
          table.parentNode.insertBefore(wrapper, table);
          wrapper.appendChild(table);
        }
      });
    }
  }

  function boot() {
    initHeaderScroll();
    initMobileNav();
    fetchLatestVersion();
    bindPerfControls();
    fetchBenchmarkMetrics();
    initArticleHelpers();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
