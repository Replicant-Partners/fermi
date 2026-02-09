// Tabs widget — simple tab switching
const Tabs = {
  init(containerId, defaultTab = 0) {
    const container = document.getElementById(containerId);
    if (!container) return;
    const buttons = container.querySelectorAll("[data-tab]");
    const panels = container.querySelectorAll("[data-tab-panel]");

    function activate(idx) {
      buttons.forEach((b, i) => b.classList.toggle("active", i === idx));
      panels.forEach((p, i) => {
        p.style.display = i === idx ? "" : "none";
      });
    }

    buttons.forEach((btn, i) => {
      btn.addEventListener("click", () => activate(i));
    });

    activate(defaultTab);
  },
};
