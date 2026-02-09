/* Theme toggle — shared across all templates
   Requires a <button id="theme-toggle"> in the page. */

(function () {
  var themes = ["hasui", "op1"];
  var labels = { hasui: "OP-1", op1: "HASUI" };

  function setTheme(theme) {
    document.documentElement.className = "theme-" + theme;
    localStorage.setItem("fermi-theme", theme);
    var btn = document.getElementById("theme-toggle");
    if (btn) btn.textContent = labels[theme];
  }

  // Migrate legacy key
  if (!localStorage.getItem("fermi-theme") && localStorage.getItem("abw-theme")) {
    localStorage.setItem("fermi-theme", localStorage.getItem("abw-theme"));
    localStorage.removeItem("abw-theme");
  }

  // Apply saved theme immediately (before DOMContentLoaded)
  var saved = localStorage.getItem("fermi-theme");
  if (saved && themes.indexOf(saved) !== -1) {
    document.documentElement.className = "theme-" + saved;
  }

  // Wire up button + shortcut after DOM ready
  function init() {
    var s = localStorage.getItem("fermi-theme");
    if (s && themes.indexOf(s) !== -1) setTheme(s);

    var btn = document.getElementById("theme-toggle");
    if (btn) {
      btn.addEventListener("click", function () {
        var current = localStorage.getItem("fermi-theme") || "hasui";
        setTheme(current === "hasui" ? "op1" : "hasui");
      });
    }

    document.addEventListener("keydown", function (e) {
      if (e.ctrlKey && e.key === "t") {
        e.preventDefault();
        if (btn) btn.click();
      }
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
