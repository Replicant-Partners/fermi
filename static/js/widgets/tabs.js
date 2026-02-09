// Tabs widget — tab switching with optional lazy-load callback
const Tabs = {
  init(containerId, defaultTab = 0, onChange) {
    const container = document.getElementById(containerId);
    if (!container) return;
    const buttons = container.querySelectorAll("[data-tab]");
    const panels = container.querySelectorAll("[data-tab-panel]");
    const loaded = new Set();

    function activate(idx) {
      buttons.forEach((b, i) => b.classList.toggle("active", i === idx));
      panels.forEach((p, i) => {
        p.style.display = i === idx ? "" : "none";
      });
      if (onChange && !loaded.has(idx)) {
        loaded.add(idx);
        const tabName = buttons[idx]?.getAttribute("data-tab") || String(idx);
        onChange(tabName, idx, panels[idx]);
      }
    }

    buttons.forEach((btn, i) => {
      btn.addEventListener("click", () => activate(i));
    });

    activate(defaultTab);
  },
};
