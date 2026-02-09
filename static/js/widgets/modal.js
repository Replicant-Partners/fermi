// Modal widget — standardizes on .active class
const Modal = {
  open(id) {
    const el = document.getElementById(id);
    if (el) el.classList.add("active");
  },

  close(id) {
    const el = document.getElementById(id);
    if (el) el.classList.remove("active");
  },

  toggle(id) {
    const el = document.getElementById(id);
    if (el) el.classList.toggle("active");
  },

  // Close on overlay click (call once per modal after DOM ready)
  init(id) {
    const el = document.getElementById(id);
    if (!el) return;
    el.addEventListener("click", (e) => {
      if (e.target === el) el.classList.remove("active");
    });
  },
};
