// Toast notification widget
// CSS lives in common.css (.toast, .toast-success, .toast-error, .toast-info)
const Toast = {
  show(message, type = "success") {
    const existing = document.querySelector(".toast");
    if (existing) existing.remove();
    const el = document.createElement("div");
    el.className = `toast toast-${type}`;
    el.textContent = message;
    document.body.appendChild(el);
    requestAnimationFrame(() => el.classList.add("show"));
    setTimeout(() => {
      el.classList.remove("show");
      setTimeout(() => el.remove(), 300);
    }, 4000);
  },
};
