// Nav widget — unified header navigation across all pages
const Nav = {
  init(opts = {}) {
    const current = opts.current || "";
    const title = opts.title || "Agent Bestiary";
    const titleHref = opts.titleHref || "/";
    const subtitle = opts.subtitle || null;
    const back = opts.back || null; // { label, href }
    const stat = opts.stat || null; // { value, label, id }

    const header = document.createElement("header");
    header.className = "nav-header";
    header.innerHTML = `
      <div class="nav-left">
        ${back ? `<a href="${back.href}" class="nav-back">${back.label}</a>` : ""}
        <a href="${titleHref}" class="nav-logo">${title}</a>
        ${subtitle ? `<span class="nav-subtitle">${subtitle}</span>` : ""}
        ${stat ? `<div class="nav-stat"><span class="nav-stat-value" id="${stat.id || ""}">${stat.value || "--"}</span><span class="nav-stat-label">${stat.label}</span></div>` : ""}
      </div>
      <div class="nav-right">
        <nav class="nav-links">
          <a href="/catalogue" class="${current === "catalogue" ? "active" : ""}">Catalogue</a>
          <a href="/marketplace" class="${current === "marketplace" ? "active" : ""}">Marketplace</a>
          <a href="/docs" class="${current === "docs" ? "active" : ""}">Docs</a>
          <a href="/dashboard" class="${current === "dashboard" ? "active" : ""}">Dashboard</a>
          <a href="/profile" class="${current === "profile" ? "active" : ""}">Profile</a>
        </nav>
        <div class="nav-bell" id="nav-bell" style="display:none">
          <button class="nav-bell-btn" onclick="Nav._toggleNotifications()" title="Notifications">
            <span class="nav-bell-icon">&#9998;</span>
            <span class="nav-bell-badge" id="nav-bell-badge" style="display:none">0</span>
          </button>
          <div class="nav-bell-dropdown" id="nav-bell-dropdown">
            <div class="nav-bell-header">
              <span>Notifications</span>
              <button onclick="Nav._markAllRead()" style="background:none;border:none;color:var(--aqua);font-size:0.8em;cursor:pointer;font-family:inherit">Mark all read</button>
            </div>
            <div id="nav-bell-list" style="max-height:300px;overflow-y:auto">
              <div style="padding:12px;color:var(--fg3);font-size:0.85em">Loading...</div>
            </div>
          </div>
        </div>
        <div id="user-area" class="auth-area"></div>
        <button id="theme-toggle" class="theme-toggle-btn" title="Toggle Theme (Ctrl+T)">OP-1</button>
      </div>
    `;

    // Insert as first child of body
    const body = document.body;
    const first = body.firstChild;
    if (first) {
      body.insertBefore(header, first);
    } else {
      body.appendChild(header);
    }

    // Auto-load auth.js if not already present
    if (typeof initAuth === "undefined") {
      const s = document.createElement("script");
      s.src = "/static/js/auth.js";
      document.body.appendChild(s);
    } else {
      initAuth();
    }

    // Load notifications if logged in
    Nav._loadNotificationCount();

    return header;
  },

  async _loadNotificationCount() {
    try {
      const res = await fetch("/api/notifications?unread=true&limit=1");
      if (!res.ok) return; // not logged in or no endpoint
      const data = await res.json();
      const count = data.unread_count || (data.notifications || []).length || 0;
      const bell = document.getElementById("nav-bell");
      if (bell) bell.style.display = "";
      if (count > 0) {
        const badge = document.getElementById("nav-bell-badge");
        if (badge) {
          badge.textContent = count > 99 ? "99+" : count;
          badge.style.display = "";
        }
      }
    } catch {
      /* silently ignore */
    }
  },

  _bellOpen: false,

  _toggleNotifications() {
    const dd = document.getElementById("nav-bell-dropdown");
    if (!dd) return;
    this._bellOpen = !this._bellOpen;
    dd.classList.toggle("visible", this._bellOpen);
    if (this._bellOpen) this._loadNotifications();
  },

  async _loadNotifications() {
    const list = document.getElementById("nav-bell-list");
    if (!list) return;
    list.innerHTML =
      '<div style="padding:12px;color:var(--fg3);font-size:0.85em">Loading...</div>';
    try {
      const res = await fetch("/api/notifications?limit=10");
      if (!res.ok) throw new Error("Failed");
      const data = await res.json();
      const notifs = data.notifications || [];
      if (notifs.length === 0) {
        list.innerHTML =
          '<div style="padding:12px;color:var(--fg3);font-size:0.85em">No notifications</div>';
        return;
      }
      list.innerHTML = notifs
        .map((n) => {
          const time = n.created_at
            ? new Date(n.created_at).toLocaleDateString()
            : "";
          const unread = !n.read ? "font-weight:500;" : "opacity:0.7;";
          return `<div style="padding:8px 12px;border-bottom:1px solid var(--bg2);font-size:0.84em;${unread}">
          <div style="color:var(--fg1)">${Nav._esc(n.message || n.title || "")}</div>
          <div style="color:var(--fg3);font-size:0.8em;margin-top:2px">${time}</div>
        </div>`;
        })
        .join("");
    } catch {
      list.innerHTML =
        '<div style="padding:12px;color:var(--red);font-size:0.85em">Failed to load</div>';
    }
  },

  async _markAllRead() {
    try {
      await fetch("/api/notifications/read-all", { method: "PUT" });
      const badge = document.getElementById("nav-bell-badge");
      if (badge) badge.style.display = "none";
      // Clear the list — they're all read now
      const list = document.getElementById("nav-bell-list");
      if (list)
        list.innerHTML =
          '<div style="padding:12px;color:var(--fg3);font-size:0.85em">No notifications</div>';
    } catch {
      /* ignore */
    }
  },

  _esc(s) {
    if (!s) return "";
    const d = document.createElement("div");
    d.textContent = s;
    return d.innerHTML;
  },
};
