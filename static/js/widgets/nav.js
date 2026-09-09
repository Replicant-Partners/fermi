// Nav widget — unified header navigation across all pages
const Nav = {
  _user: null,
  _bellOpen: false,
  _userMenuOpen: false,
  _moreOpen: false,

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
        <!-- Two groups, because the product has two verbs and a flat list of
             seven links hid both. COMPOSE is what you make: an agent, a
             composition, an app and its workspaces. CURATE is what you manage:
             the pulses those things emit, the loops they improve through, the
             gates that constrain them, and the evaluators that judge them.

             Older surfaces are still reachable under "More" rather than
             deleted, so the new ones can be compared against them before
             anything is removed. -->
        <nav class="nav-links nav-grouped">
          <span class="nav-group">
            <span class="nav-group-label">compose</span>
            <a href="/bestiary" class="${current === "bestiary" ? "active" : ""}">Bestiary</a>
            <a href="/apps" class="${current === "apps" ? "active" : ""}">Apps</a>
            <!-- New belongs here: the platform's primary verb had no entry in
                 the persistent nav, so you could only start creating from
                 somewhere that already thought to offer it.

                 Contracts was here too and is gone. A contract is not a thing
                 you go and make on its own. It is a step of composing an agent
                 (view 4 of /agents/new) and a panel of configuring one (the
                 Configure shelf on /specimen/:name). It is the same widget in
                 both, static/js/widgets/contract-builder.js, and both places
                 already carry it. A third entry point to a component that is
                 meaningless without an agent to attach it to invited a reader
                 to build one in a vacuum, which is exactly what the standalone
                 page allowed and nothing could then use.

                 /contracts still serves. It is where the two hosts mount from
                 and it is a legitimate scratch surface; it is simply not a
                 destination the nav proposes.

                 NOTE: no backticks in this comment, and that is load-bearing.
                 An HTML comment inside a JS template literal is string content,
                 not a comment, so the first backtick ends the literal and the
                 rest of the file parses as code. This has been written five
                 times in this repository, most recently right here. -->
            <a href="/agents/new" class="${current === "agents-new" ? "active" : ""}">New</a>
          </span>
          <span class="nav-group">
            <span class="nav-group-label">curate</span>
            <a href="/stream" class="${current === "stream" ? "active" : ""}">Pulses</a>
            <!-- Loops, Gates and Evaluators were three entries pointing at one
                 page, /loops, distinguished by which of its four tabs opened.
                 All three are gone.

                 Not because the machinery is wrong. The loops turn, the gates
                 decide, the evaluators score. But none of the four views told a
                 reader anything they could act on: loop4 NOT WIRED, SILENT 61
                 arrow anchored 0, a wall of UNGRADED artifacts. Platform-wide
                 aggregates over machinery, with no subject a person has any
                 relationship to.

                 There is no useful GLOBAL view of a loop. A loop is a path an
                 artifact takes, so the place it becomes legible is the artifact.
                 Pulses lists them and a trace shows one crossing its
                 checkpoints, with the gate that decided each and whether it
                 could refuse. That answers the same questions about a subject
                 the reader chose, which is the difference between information
                 and a dashboard.

                 Observatory keeps its entry: it is per-agent, and per-agent is
                 where scores and flags for a specimen belong. -->
            <a href="/observatory" class="${current === "observatory" ? "active" : ""}">Observatory</a>
          </span>
          <span class="nav-group nav-group-quiet">
            <a href="/docs" class="${current === "docs" ? "active" : ""}">Docs</a>
            <span class="nav-more">
              <button class="nav-more-btn" onclick="Nav._toggleMore()">More &#9662;</button>
              <div class="nav-more-dd" id="nav-more-dd">
                <div class="nav-dropdown-label">Earlier surfaces</div>
                <a href="/rounds" class="nav-dropdown-item">Rounds</a>
                <a href="/ecology" class="nav-dropdown-item">Ecology</a>
                <a href="/declarations" class="nav-dropdown-item">Declarations</a>
                <a href="/catalogue" class="nav-dropdown-item">Catalogue</a>
              </div>
            </span>
          </span>
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
        <div class="nav-user-menu" id="nav-user-menu">
          <div id="nav-user-area"></div>
        </div>
        <button id="theme-toggle" style="display:none">OP-1</button>
      </div>
    `;

    const body = document.body;
    const first = body.firstChild;
    if (first) {
      body.insertBefore(header, first);
    } else {
      body.appendChild(header);
    }

    // Close dropdowns on outside click
    document.addEventListener("click", function (e) {
      // Close notification dropdown
      if (Nav._bellOpen && !e.target.closest(".nav-bell")) {
        Nav._bellOpen = false;
        var dd = document.getElementById("nav-bell-dropdown");
        if (dd) dd.classList.remove("visible");
      }
      // Close the "More" list
      if (Nav._moreOpen && !e.target.closest(".nav-more")) {
        Nav._moreOpen = false;
        var mo = document.getElementById("nav-more-dd");
        if (mo) mo.classList.remove("visible");
      }
      // Close user menu
      if (Nav._userMenuOpen && !e.target.closest(".nav-user-menu")) {
        Nav._userMenuOpen = false;
        var um = document.getElementById("nav-user-dropdown");
        if (um) um.classList.remove("visible");
      }
    });

    // Load auth state and build user menu
    Nav._loadAuth();

    return header;
  },

  async _loadAuth() {
    try {
      const res = await fetch("/api/auth/me");
      if (!res.ok) throw new Error("not authenticated");
      const user = await res.json();
      Nav._user = user;
      Nav._renderUserMenu(user);
      Nav._loadNotificationCount();
    } catch {
      Nav._user = null;
      Nav._renderSignIn();
    }
  },

  _renderUserMenu(user) {
    const area = document.getElementById("nav-user-area");
    if (!area) return;

    const name = user.display_name || user.email || "User";
    const initial = name.charAt(0).toUpperCase();
    const isAdmin = user.role === "admin";
    const currentTheme = document.documentElement.classList.contains(
      "theme-op1",
    )
      ? "op1"
      : "hasui";
    const themeLabel = currentTheme === "op1" ? "Hasui (Dark)" : "OP-1 (Light)";
    const themeIcon = currentTheme === "op1" ? "\u263D" : "\u2600";

    area.innerHTML = `
      <button class="nav-user-btn" onclick="Nav._toggleUserMenu()">
        <span class="nav-user-initial">${Nav._esc(initial)}</span>
        <span class="nav-user-name">${Nav._esc(name)}</span>
        <span class="nav-user-caret">&#9662;</span>
      </button>
      <div class="nav-user-dropdown" id="nav-user-dropdown">
        <div class="nav-dropdown-label">${Nav._esc(user.email || name)}</div>
        <a href="/dashboard" class="nav-dropdown-item">Dashboard</a>
        <a href="/profile" class="nav-dropdown-item">Profile</a>
        <a href="/settings" class="nav-dropdown-item">Settings</a>
        ${isAdmin ? '<a href="/admin" class="nav-dropdown-item">Admin</a>' : ""}
        <div class="nav-dropdown-sep"></div>
        <button class="nav-dropdown-item nav-dropdown-theme" onclick="Nav._toggleTheme()">
          <span id="nav-theme-icon">${themeIcon}</span>
          <span id="nav-theme-label">${themeLabel}</span>
        </button>
        <button class="nav-dropdown-item nav-dropdown-signout" onclick="Nav._signOut()">Sign Out</button>
      </div>
    `;
  },

  _renderSignIn() {
    const area = document.getElementById("nav-user-area");
    if (!area) return;

    area.innerHTML = `
      <button class="nav-signin-btn" onclick="Nav._toggleUserMenu()">
        Sign In <span class="nav-user-caret">&#9662;</span>
      </button>
      <div class="nav-user-dropdown" id="nav-user-dropdown">
        <a href="/auth/google" class="nav-dropdown-item">Sign in with Google</a>
        <a href="/auth/github" class="nav-dropdown-item">Sign in with GitHub</a>
      </div>
    `;
  },

  _toggleMore() {
    const dd = document.getElementById("nav-more-dd");
    if (!dd) return;
    Nav._moreOpen = !Nav._moreOpen;
    dd.classList.toggle("visible", Nav._moreOpen);
  },

  _toggleUserMenu() {
    const dd = document.getElementById("nav-user-dropdown");
    if (!dd) return;
    Nav._userMenuOpen = !Nav._userMenuOpen;
    dd.classList.toggle("visible", Nav._userMenuOpen);
  },

  _toggleTheme() {
    // Delegate to the theme-toggle button if present (theme.js handles the logic)
    var btn = document.getElementById("theme-toggle");
    if (btn) {
      btn.click();
    } else {
      // Fallback if theme.js not loaded
      const html = document.documentElement;
      const isOp1 = html.classList.contains("theme-op1");
      html.className = isOp1 ? "theme-hasui" : "theme-op1";
      localStorage.setItem("fermi-theme", isOp1 ? "hasui" : "op1");
    }
    // Update icon and label
    const newIsOp1 = document.documentElement.classList.contains("theme-op1");
    var icon = document.getElementById("nav-theme-icon");
    var label = document.getElementById("nav-theme-label");
    if (icon) icon.textContent = newIsOp1 ? "\u263D" : "\u2600";
    if (label) label.textContent = newIsOp1 ? "Hasui (Dark)" : "OP-1 (Light)";
  },

  async _signOut() {
    try {
      await fetch("/auth/logout", { method: "POST" });
    } catch {
      /* ignore */
    }
    window.location.reload();
  },

  async _loadNotificationCount() {
    try {
      const res = await fetch("/api/notifications?unread=true&limit=1");
      if (!res.ok) return;
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

// `const Nav` is a binding in the global LEXICAL environment, and that is not
// the same thing as a property of `window`. In a classic script only `var` and
// function declarations become `window` properties, so:
//
//     Nav.init({ ... })                 // resolves, works
//     if (window.Nav) Nav.init({ ... }) // window.Nav is undefined, skipped
//
// Eighteen templates use the first form. Nine use the second, and every one of
// them silently rendered with no navigation at all: bestiary, declarations,
// flow, gate, loops, rounds, specimen, stream, trace. Those are the platform's
// primary surfaces, and the reason nobody saw a stack trace is that the guard
// was written to be careful.
//
// That is the defect worth naming. `if (window.Nav)` reads as defensive and
// what it actually did was convert a one-line omission into an invisible loss
// of the header on the pages that matter most, on every load, for as long as
// nobody happened to look. A guard whose false branch is silent cannot tell you
// it took the false branch.
//
// So the guard is made TRUE rather than removed: if this file ever fails to
// load, `window.Nav` is undefined and those nine pages still render their
// content instead of throwing. That is the check those templates were reaching
// for, and now it is the check they get.
window.Nav = Nav;
