// Xaman Ek — The Bestiary Dungeon Master
//
// Two surfaces, one agent:
//   SIDEBAR  — persistent left-hand panel, working sessions, survives navigation
//   SPOTLIGHT — Ctrl+K quick panel, questions + commands, auto-opens on first use
//
// The sidebar holds the table of contents of what you're building.
// The spotlight is the current page of dialogue.
//
// Sessions are persisted server-side (/api/xaman/sessions).
// localStorage is used only as a fallback when unauthenticated.

const XamanEk = {
  // ── State ──────────────────────────────────────────────────────────────────
  _sidebar: null,
  _spotlight: null,
  _input: null,
  _sidebarVisible: false,
  _spotlightVisible: false,
  _debounce: null,
  _recentKey: "xaman-ek-recent",
  _currentUser: null,
  _userLoaded: false,
  _sessions: [],          // loaded from API
  _activeSession: null,   // { session_id, title, messages, ... }
  _sessionLoading: false,

  // ── Init ───────────────────────────────────────────────────────────────────
  init() {
    this._buildSidebar();
    this._buildSpotlight();
    this._buildFab();
    this._bindKeyboard();
    this._loadUser().then(() => {
      this._loadSessions();
    });
  },

  // ── Sidebar ────────────────────────────────────────────────────────────────
  _buildSidebar() {
    const sidebar = document.createElement("div");
    sidebar.id = "xaman-sidebar";
    sidebar.className = "xaman-sidebar";
    sidebar.innerHTML = `
      <div class="xaman-sidebar-header">
        <div class="xaman-sidebar-title">
          <span style="color:var(--yellow)">★</span> Xaman Ek
        </div>
        <div style="display:flex;gap:6px;align-items:center">
          <button class="xaman-sidebar-new" onclick="XamanEk.newSession()" title="New session">+</button>
          <button class="xaman-sidebar-close" onclick="XamanEk.closeSidebar()" title="Close">×</button>
        </div>
      </div>

      <div class="xaman-sidebar-context" id="xaman-sidebar-context"></div>

      <div class="xaman-sidebar-sessions" id="xaman-sidebar-sessions">
        <div class="xaman-sidebar-section-label">Sessions</div>
        <div id="xaman-sessions-list" style="color:var(--fg3);font-size:0.78em;padding:6px 12px">
          Loading...
        </div>
      </div>

      <div class="xaman-sidebar-chat" id="xaman-sidebar-chat" style="display:none">
        <div class="xaman-sidebar-back" onclick="XamanEk.showSessionsList()">← Sessions</div>
        <div class="xaman-sidebar-chat-title" id="xaman-sidebar-chat-title"></div>
        <div class="xaman-sidebar-messages" id="xaman-sidebar-messages"></div>
        <div class="xaman-sidebar-input-row">
          <textarea id="xaman-sidebar-input" class="xaman-sidebar-input"
            placeholder="Ask xamanEK..." rows="2"
            onkeydown="XamanEk._sidebarKeydown(event)"></textarea>
          <button class="xaman-sidebar-send" onclick="XamanEk.sendSidebarMessage()">↑</button>
        </div>
      </div>
    `;
    document.body.appendChild(sidebar);
    this._sidebar = sidebar;
    this._updateSidebarContext();
  },

  toggleSidebar() {
    this._sidebarVisible ? this.closeSidebar() : this.openSidebar();
  },

  openSidebar() {
    if (!this._sidebar) return;
    this._sidebar.classList.add("visible");
    this._sidebarVisible = true;
    document.body.classList.add("xaman-sidebar-open");
    this._updateSidebarContext();
    this._renderSessionsList();
  },

  closeSidebar() {
    if (!this._sidebar) return;
    this._sidebar.classList.remove("visible");
    this._sidebarVisible = false;
    document.body.classList.remove("xaman-sidebar-open");
  },

  _updateSidebarContext() {
    const el = document.getElementById("xaman-sidebar-context");
    if (!el) return;
    const ctx = this._pageContext();
    if (!ctx.label) { el.innerHTML = ""; return; }
    el.innerHTML = `
      <div class="xaman-sidebar-context-chip">
        <span style="color:var(--fg3);font-size:0.7em;text-transform:uppercase;letter-spacing:0.05em">On page</span>
        <span style="color:var(--aqua);font-size:0.78em;margin-left:4px">${this._esc(ctx.label)}</span>
      </div>`;
  },

  // ── Session list ───────────────────────────────────────────────────────────
  async _loadSessions() {
    if (!this._currentUser) return;
    try {
      const res = await fetch("/api/xaman/sessions");
      if (res.ok) {
        const data = await res.json();
        this._sessions = data.sessions || [];
      }
    } catch (_) {}
    this._renderSessionsList();
  },

  _renderSessionsList() {
    const el = document.getElementById("xaman-sessions-list");
    if (!el) return;

    if (!this._currentUser) {
      el.innerHTML = `<div style="color:var(--fg3);font-size:0.78em;padding:4px 0">
        <a href="/auth/google" style="color:var(--aqua)">Sign in</a> to save sessions.
      </div>`;
      return;
    }

    if (this._sessions.length === 0) {
      el.innerHTML = `<div style="color:var(--fg3);font-size:0.78em;padding:4px 0">
        No sessions yet.<br>
        <span style="color:var(--yellow)">+</span> to start one, or ask a question below.
      </div>
      <button onclick="XamanEk.newSession()" style="margin-top:6px;width:100%;padding:6px;background:var(--bg1);border:1px solid var(--bg3);color:var(--aqua);font-size:0.78em;cursor:pointer;font-family:inherit">
        Start a session
      </button>`;
      return;
    }

    const typeIcon = {
      agent_design: "🃏",
      composition_design: "🧩",
      workspace_help: "🏗️",
      free: "★",
    };

    el.innerHTML = this._sessions.map(s => `
      <div class="xaman-session-item" onclick="XamanEk.openSessionChat('${s.session_id}')">
        <div class="xaman-session-item-header">
          <span class="xaman-session-icon">${typeIcon[s.session_type] || "★"}</span>
          <span class="xaman-session-title">${this._esc(s.title || "Untitled session")}</span>
        </div>
        <div class="xaman-session-meta">
          ${s.session_type.replace("_", " ")} · ${this._timeAgo(s.last_active_at)}
        </div>
      </div>
    `).join("");
  },

  async newSession(type) {
    if (!this._currentUser) {
      this.openSpotlight();
      return;
    }
    const ctx = this._pageContext();
    try {
      const res = await fetch("/api/xaman/sessions", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          session_type: type || "free",
          page_context: ctx.path,
        }),
      });
      if (res.ok) {
        const data = await res.json();
        await this._loadSessions();
        this.openSessionChat(data.session_id);
      }
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  },

  async openSessionChat(sessionId) {
    const chat = document.getElementById("xaman-sidebar-chat");
    const list = document.getElementById("xaman-sidebar-sessions");
    if (!chat || !list) return;

    // Show chat, hide session list
    list.style.display = "none";
    chat.style.display = "flex";

    // Load session
    try {
      const res = await fetch(`/api/xaman/sessions/${sessionId}`);
      if (!res.ok) { this.showSessionsList(); return; }
      this._activeSession = await res.json();
      this._renderSessionChat();
    } catch (e) {
      this.showSessionsList();
    }
  },

  showSessionsList() {
    const chat = document.getElementById("xaman-sidebar-chat");
    const list = document.getElementById("xaman-sidebar-sessions");
    if (chat) chat.style.display = "none";
    if (list) list.style.display = "";
    this._activeSession = null;
  },

  _renderSessionChat() {
    const session = this._activeSession;
    if (!session) return;

    const titleEl = document.getElementById("xaman-sidebar-chat-title");
    const messagesEl = document.getElementById("xaman-sidebar-messages");
    if (titleEl) titleEl.textContent = session.title || "Session";

    if (!messagesEl) return;
    const messages = session.messages || [];
    if (messages.length === 0) {
      messagesEl.innerHTML = `<div class="xaman-sidebar-empty-chat">
        <div style="color:var(--yellow);font-size:1.2em;margin-bottom:8px">★</div>
        <div style="color:var(--fg3);font-size:0.78em;line-height:1.5">
          I'm xamanEK — the Bestiary's dungeon master.<br>
          I know every agent, every composition pattern, and<br>
          how the primitives fit together.<br><br>
          What are you building?
        </div>
      </div>`;
      return;
    }

    messagesEl.innerHTML = messages.map(m => `
      <div class="xaman-sidebar-message xaman-sidebar-message--${m.role}">
        <div class="xaman-sidebar-message-content">${this._renderMarkdown(m.content)}</div>
      </div>
    `).join("");

    // Scroll to bottom
    messagesEl.scrollTop = messagesEl.scrollHeight;
  },

  async sendSidebarMessage() {
    const input = document.getElementById("xaman-sidebar-input");
    const messagesEl = document.getElementById("xaman-sidebar-messages");
    if (!input || !this._activeSession) return;

    const message = input.value.trim();
    if (!message) return;

    input.value = "";
    input.style.height = "auto";

    // Optimistic render
    if (messagesEl) {
      messagesEl.innerHTML += `
        <div class="xaman-sidebar-message xaman-sidebar-message--user">
          <div class="xaman-sidebar-message-content">${this._esc(message)}</div>
        </div>
        <div class="xaman-sidebar-message xaman-sidebar-message--assistant" id="xaman-thinking">
          <div class="xaman-sidebar-message-content" style="color:var(--fg3)">Thinking...</div>
        </div>`;
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }

    const ctx = this._pageContext();
    try {
      const res = await fetch(`/api/xaman/sessions/${this._activeSession.session_id}/message`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          message,
          page_context: ctx.description,
        }),
      });

      const thinking = document.getElementById("xaman-thinking");
      if (thinking) thinking.remove();

      if (res.ok) {
        const data = await res.json();
        // Update active session title if auto-named
        if (data.title && !this._activeSession.title) {
          this._activeSession.title = data.title;
          const titleEl = document.getElementById("xaman-sidebar-chat-title");
          if (titleEl) titleEl.textContent = data.title;
        }
        if (messagesEl) {
          messagesEl.innerHTML += `
            <div class="xaman-sidebar-message xaman-sidebar-message--assistant">
              <div class="xaman-sidebar-message-content">${this._renderMarkdown(data.response)}</div>
            </div>`;
          messagesEl.scrollTop = messagesEl.scrollHeight;
        }
        // Refresh session list in background
        this._loadSessions();
      } else {
        if (messagesEl) {
          messagesEl.innerHTML += `
            <div class="xaman-sidebar-message xaman-sidebar-message--assistant">
              <div class="xaman-sidebar-message-content" style="color:var(--red)">
                Something went wrong. Try again.
              </div>
            </div>`;
        }
      }
    } catch (e) {
      const thinking = document.getElementById("xaman-thinking");
      if (thinking) thinking.remove();
      if (messagesEl) {
        messagesEl.innerHTML += `<div class="xaman-sidebar-message xaman-sidebar-message--assistant">
          <div class="xaman-sidebar-message-content" style="color:var(--red)">Network error.</div>
        </div>`;
      }
    }
  },

  _sidebarKeydown(e) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      this.sendSidebarMessage();
    }
  },

  // ── Spotlight (Ctrl+K) ─────────────────────────────────────────────────────
  _buildSpotlight() {
    const panel = document.createElement("div");
    panel.className = "xaman-panel";
    panel.innerHTML = `
      <div class="xaman-header">
        <span class="xaman-title">★ Xaman Ek</span>
        <div style="display:flex;gap:8px;align-items:center">
          <button onclick="XamanEk.openSidebar(); XamanEk.closeSpotlight();"
            style="background:none;border:none;color:var(--fg3);cursor:pointer;font-size:0.72em;padding:2px 6px;border:1px solid var(--bg3)"
            title="Open session sidebar">Sessions</button>
          <button class="xaman-close" onclick="XamanEk.closeSpotlight()">&times;</button>
        </div>
      </div>
      <div class="xaman-search">
        <input type="text" class="xaman-input"
          placeholder="Search agents, @agent query, how do I..." autocomplete="off" />
      </div>
      <div class="xaman-body">
        <div class="xaman-context" id="xaman-context"></div>
        <div class="xaman-recent" id="xaman-recent"></div>
        <div class="xaman-results" id="xaman-results"></div>
      </div>
    `;
    document.body.appendChild(panel);
    this._spotlight = panel;
    this._input = panel.querySelector(".xaman-input");

    this._input.addEventListener("input", () => {
      clearTimeout(this._debounce);
      this._debounce = setTimeout(() => this._search(this._input.value.trim()), 300);
    });
    this._input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.closeSpotlight();
      if (e.key === "Enter") {
        const q = this._input.value.trim();
        if (q.startsWith("@faucet ")) this._executeFaucet(q);
        else if (q.startsWith("@")) this._executeAgent(q);
      }
    });
  },

  _buildFab() {
    const fab = document.createElement("button");
    fab.className = "xaman-fab";
    fab.innerHTML = "&#9733;";
    fab.title = "Xaman Ek (Ctrl+K — spotlight / Ctrl+Shift+K — sidebar)";
    fab.addEventListener("click", () => this.toggleSpotlight());
    document.body.appendChild(fab);
  },

  _bindKeyboard() {
    document.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === "K") {
        e.preventDefault();
        this.toggleSidebar();
      } else if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        this.toggleSpotlight();
      }
    });
  },

  toggleSpotlight() { this._spotlightVisible ? this.closeSpotlight() : this.openSpotlight(); },

  openSpotlight() {
    if (!this._spotlight) return;
    this._spotlight.classList.add("visible");
    this._spotlightVisible = true;
    this._input.value = "";
    document.getElementById("xaman-results").innerHTML = "";
    this._renderRecent();
    this._renderContext();
    setTimeout(() => this._input.focus(), 50);
  },

  closeSpotlight() {
    if (!this._spotlight) return;
    this._spotlight.classList.remove("visible");
    this._spotlightVisible = false;
  },

  // Legacy aliases so existing onclick handlers keep working
  toggle() { this.toggleSpotlight(); },
  open()   { this.openSpotlight(); },
  close()  { this.closeSpotlight(); },

  // ── Page context ───────────────────────────────────────────────────────────
  _pageContext() {
    const path = window.location.pathname;
    const parts = path.split("/").filter(Boolean);

    if (parts[0] === "agent" && parts[1]) {
      return { path, label: parts[1], description: `Viewing agent: ${parts[1]}`, type: "agent", id: parts[1] };
    }
    if (parts[0] === "workspace" && parts[1]) {
      return { path, label: `workspace`, description: `In workspace: ${parts[1]}`, type: "workspace", id: parts[1] };
    }
    if (parts[0] === "observatory") {
      const agentId = new URLSearchParams(window.location.search).get("agent");
      return { path, label: "observatory", description: agentId ? `Observatory: ${agentId}` : "Observatory", type: "observatory", id: agentId };
    }
    if (path === "/dashboard") return { path, label: "dashboard", description: "Dashboard", type: "dashboard" };
    if (path === "/catalogue") return { path, label: "catalogue", description: "Agent catalogue", type: "catalogue" };
    if (path === "/profile") return { path, label: "profile", description: "Profile", type: "profile" };

    return { path, label: "", description: path, type: "other" };
  },

  // ── Auth ───────────────────────────────────────────────────────────────────
  async _loadUser() {
    if (this._userLoaded) return;
    try {
      const res = await fetch("/api/auth/me");
      if (res.ok) this._currentUser = await res.json();
    } catch (_) {}
    this._userLoaded = true;
  },

  _isAdmin() { return this._currentUser?.role === "admin"; },

  // ── Search (spotlight) ────────────────────────────────────────────────────
  async _search(query) {
    const results = document.getElementById("xaman-results");
    if (!query) {
      results.innerHTML = "";
      this._renderRecent();
      return;
    }

    const ctx = document.getElementById("xaman-context");
    const rec = document.getElementById("xaman-recent");
    if (ctx) ctx.innerHTML = "";
    if (rec) rec.innerHTML = "";

    if (query.startsWith("@faucet")) {
      if (!this._isAdmin()) {
        results.innerHTML = '<div class="xaman-hint" style="color:var(--red)">Admin access required</div>';
        return;
      }
      const parts = query.substring(8).trim().split(/\s+/);
      const userSearch = parts[0] || "";
      const amount = parts[1] || "";
      if (!userSearch) {
        results.innerHTML = '<div class="xaman-hint">@faucet &lt;user&gt; &lt;amount&gt;</div>';
      } else if (!amount) {
        if (userSearch === "self") {
          results.innerHTML = '<div class="xaman-hint">@faucet self &lt;amount&gt;</div>';
        } else {
          this._faucetSearchUsers(userSearch);
        }
      } else {
        results.innerHTML = `<div class="xaman-hint">Press Enter to grant <strong>${this._esc(amount)}</strong> credits to <strong>${this._esc(userSearch === "self" ? "yourself" : userSearch)}</strong></div>`;
      }
      return;
    }

    if (query.startsWith("@")) {
      const parts = query.substring(1).split(/\s+/);
      const agentName = parts[0] || "";
      const agentQuery = parts.slice(1).join(" ");
      results.innerHTML = agentQuery
        ? `<div class="xaman-hint">Press Enter to execute @${this._esc(agentName)}</div>`
        : `<div class="xaman-hint">Type a query after @${this._esc(agentName)} and press Enter</div>`;
      return;
    }

    const lq = query.toLowerCase();
    if (lq.startsWith("how do i") || lq.startsWith("how to") || lq.startsWith("help")) {
      this._renderHelp(lq);
      return;
    }

    results.innerHTML = '<div class="xaman-hint">Searching...</div>';

    try {
      const res = await fetch(`/api/agents?search=${encodeURIComponent(query)}&limit=6`);
      if (!res.ok) throw new Error("Search failed");
      const data = await res.json();
      const agents = data.agents || [];

      if (agents.length === 0) {
        results.innerHTML = '<div class="xaman-hint">No agents found</div>';
        return;
      }

      results.innerHTML = agents.map(a => {
        const name = a.display_alias || a.agent_name || a.name;
        const desc = (a.description || "").slice(0, 80);
        const tags = (a.tags || []).slice(0, 3);
        return `<a href="/agent/${a.agent_id}" class="xaman-result">
          <div class="xaman-result-name">${this._esc(name)}</div>
          <div class="xaman-result-desc">${this._esc(desc)}</div>
          ${tags.length ? `<div class="xaman-result-tags">${tags.map(t => `<span class="xaman-tag">${this._esc(t)}</span>`).join("")}</div>` : ""}
        </a>`;
      }).join("");

      this._saveRecent(query);
    } catch (_) {
      results.innerHTML = '<div class="xaman-hint">Search error</div>';
    }
  },

  // ── @agent execution ───────────────────────────────────────────────────────
  async _executeAgent(raw) {
    const parts = raw.substring(1).split(/\s+/);
    const agentName = parts[0];
    const query = parts.slice(1).join(" ");
    if (!agentName || !query) return;

    const results = document.getElementById("xaman-results");
    results.innerHTML = `<div class="xaman-hint" style="color:var(--yellow)">Executing @${this._esc(agentName)}...</div>`;

    try {
      const res = await fetch(`/api/agents/${encodeURIComponent(agentName)}/execute`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ query }),
      });

      if (res.status === 401) {
        results.innerHTML = '<div class="xaman-hint" style="color:var(--orange)">Sign in to execute agents</div>';
        return;
      }
      if (!res.ok) {
        const err = await res.text().catch(() => "Unknown error");
        results.innerHTML = `<div class="xaman-hint" style="color:var(--red)">Error: ${this._esc(err)}</div>`;
        return;
      }

      const data = await res.json();
      const answer = data.answer || data.result?.answer || data.result?.summary || "";
      const confidence = data.confidence ?? data.result?.confidence;
      const evidence = data.evidence || data.result?.evidence || [];

      let html = `<div class="xaman-exec-result">`;
      html += `<div class="xaman-exec-agent">@${this._esc(agentName)}</div>`;
      if (confidence != null) {
        const pct = Math.round(confidence * 100);
        const color = pct >= 70 ? "var(--green)" : pct >= 40 ? "var(--yellow)" : "var(--red)";
        html += `<div style="font-size:0.8em;color:${color};margin-bottom:6px">Confidence: ${pct}%</div>`;
      }
      if (answer) html += `<div class="xaman-exec-answer">${this._esc(answer).substring(0, 500)}</div>`;
      if (evidence.length > 0) {
        html += '<div style="margin-top:6px;font-size:0.8em;color:var(--fg3)">';
        evidence.slice(0, 3).forEach(e => {
          const text = typeof e === "string" ? e : (e.description || e.text || e.finding || "");
          if (text) html += `<div style="margin-bottom:2px">· ${this._esc(text).substring(0, 120)}</div>`;
        });
        html += "</div>";
      }
      html += `<a href="/agent/${encodeURIComponent(agentName)}" class="xaman-exec-link">View agent →</a>`;
      html += "</div>";
      results.innerHTML = html;
      this._saveRecent(raw);
    } catch (e) {
      document.getElementById("xaman-results").innerHTML = `<div class="xaman-hint" style="color:var(--red)">Network error</div>`;
    }
  },

  // ── @faucet ────────────────────────────────────────────────────────────────
  _faucetUserMap: {},

  async _faucetSearchUsers(search) {
    const results = document.getElementById("xaman-results");
    results.innerHTML = '<div class="xaman-hint">Searching users...</div>';
    try {
      const res = await fetch(`/api/admin/users?search=${encodeURIComponent(search)}&limit=5`);
      if (res.status === 403) { results.innerHTML = '<div class="xaman-hint" style="color:var(--red)">Admin required</div>'; return; }
      if (!res.ok) throw new Error();
      const data = await res.json();
      const users = data.users || [];
      if (users.length === 0) { results.innerHTML = '<div class="xaman-hint">No users found</div>'; return; }
      users.forEach(u => { this._faucetUserMap[u.user_id] = u.display_name || u.email || u.user_id; });
      results.innerHTML = users.map(u => {
        const name = u.display_name || u.email || u.user_id;
        return `<div class="xaman-result" style="cursor:pointer" data-uid="${this._esc(u.user_id)}" onclick="XamanEk._faucetSelectUser(this.dataset.uid)">
          <div class="xaman-result-name">${this._esc(name)}</div>
          <div class="xaman-result-desc">${this._esc(u.email || u.user_id)}</div>
        </div>`;
      }).join("");
    } catch (_) { results.innerHTML = '<div class="xaman-hint" style="color:var(--red)">Error</div>'; }
  },

  _faucetSelectUser(userId) {
    const current = this._input.value.trim();
    const parts = current.substring(8).trim().split(/\s+/);
    const amount = parts[1] || "";
    this._input.value = `@faucet ${userId} ${amount}`;
    this._input.focus();
    this._search(this._input.value.trim());
  },

  async _executeFaucet(raw) {
    const parts = raw.substring(8).trim().split(/\s+/);
    let userSearch = parts[0];
    const amount = parseInt(parts[1], 10);
    if (userSearch === "self" && this._currentUser?.user_id) userSearch = this._currentUser.user_id;
    if (!userSearch || !amount || isNaN(amount) || amount < 1 || amount > 10000) {
      document.getElementById("xaman-results").innerHTML = '<div class="xaman-hint" style="color:var(--orange)">Usage: @faucet &lt;user_id&gt; &lt;amount&gt; (max 10000)</div>';
      return;
    }
    const displayTarget = this._faucetUserMap[userSearch] || (userSearch === this._currentUser?.user_id ? "yourself" : userSearch);
    document.getElementById("xaman-results").innerHTML = `<div class="xaman-hint" style="color:var(--yellow)">Granting ${amount} credits...</div>`;
    try {
      const res = await fetch(`/api/admin/users/${encodeURIComponent(userSearch)}/grant`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ credits: amount, reason: "Faucet grant via Xaman Ek" }),
      });
      if (!res.ok) { document.getElementById("xaman-results").innerHTML = '<div class="xaman-hint" style="color:var(--red)">Grant failed</div>'; return; }
      const data = await res.json();
      document.getElementById("xaman-results").innerHTML = `<div class="xaman-exec-result">
        <div class="xaman-exec-agent" style="color:var(--green)">Granted</div>
        <div style="margin:8px 0">${data.credits} credits → ${this._esc(data.display_name || displayTarget)}</div>
      </div>`;
      this._input.value = "";
    } catch (_) {
      document.getElementById("xaman-results").innerHTML = '<div class="xaman-hint" style="color:var(--red)">Network error</div>';
    }
  },

  // ── Context suggestions (spotlight) ───────────────────────────────────────
  _renderContext() {
    const ctx = document.getElementById("xaman-context");
    if (!ctx) return;
    const page = this._pageContext();
    const suggestions = [];

    if (page.type === "agent") {
      suggestions.push({ label: "View ontology", href: `/agent/${page.id}/ontology` });
      suggestions.push({ label: "Embedding projector", href: `/agent/${page.id}/projector` });
      suggestions.push({ label: "Observatory", href: `/observatory?agent=${page.id}` });
      suggestions.push({ label: "Execute inline", action: `XamanEk._input.value='@${page.id} '; XamanEk._input.focus()` });
    } else if (page.type === "workspace") {
      this._renderWorkspaceContext(ctx);
      return;
    } else if (page.type === "observatory") {
      if (page.id) {
        suggestions.push({ label: "Trigger scan", action: `fetch('/api/observatory/agents/${page.id}/scan',{method:'POST'}).then(()=>location.reload())` });
        suggestions.push({ label: "HITL queue", href: "/observatory/hitl" });
        suggestions.push({ label: "Agent detail", href: `/agent/${page.id}` });
      }
    } else if (page.type === "dashboard") {
      suggestions.push({ label: "Create agent", href: "/agents/new" });
      suggestions.push({ label: "New workspace", action: "document.getElementById('create-ws-btn')?.click()" });
      suggestions.push({ label: "Browse catalogue", href: "/catalogue" });
    } else if (page.type === "catalogue") {
      suggestions.push({ label: "Create agent", href: "/agents/new" });
      suggestions.push({ label: "Design checklist", href: "/docs/building-your-agent-deck" });
    }

    // Always offer "open session" when in spotlight
    suggestions.push({ label: "★ Open sessions", action: "XamanEk.openSidebar(); XamanEk.closeSpotlight();" });

    if (suggestions.length === 0) { ctx.innerHTML = ""; return; }
    ctx.innerHTML = '<div class="xaman-context-label">Quick actions</div>' +
      suggestions.map(s => s.href
        ? `<a href="${s.href}" class="xaman-context-item">${this._esc(s.label)}</a>`
        : `<div class="xaman-context-item" onclick="${s.action}" style="cursor:pointer">${this._esc(s.label)}</div>`
      ).join("");
  },

  // Workspace interaction guide (unchanged from original)
  _INTERACTION_PATTERNS: {
    "compound-agent": { icon: "⚙", role: "Orchestrator", hint: "Coordinates other agents.", examples: ["Run the full pipeline on this brief", "Create content for [platform]"] },
    coherence: { icon: "☸", role: "Evaluator", hint: "Evaluates and improves coherence.", examples: ["How coherent is this workspace?", "Diagnose the coordination failure"] },
    creative: { icon: "✎", role: "Creator", hint: "Generates or transforms content.", examples: ["Apply vintage style to this image", "Generate an image of [description]"] },
    research: { icon: "🔍", role: "Researcher", hint: "Evidence-based analysis.", examples: ["Research [topic] and summarize", "What are the trends in [domain]?"] },
    meta: { icon: "★", role: "Guide", hint: "Platform navigation and advice.", examples: ["What agents should I hire for [goal]?"] },
  },

  async _renderWorkspaceContext(ctx) {
    const wsId = window.location.pathname.split("/").pop();
    if (!wsId) { ctx.innerHTML = ""; return; }
    ctx.innerHTML = '<div class="xaman-context-label">Loading workspace guide...</div>';
    try {
      const res = await fetch(`/api/workspaces/${wsId}`);
      if (!res.ok) { ctx.innerHTML = ""; return; }
      const ws = await res.json();
      const agents = ws.agents || [];
      if (agents.length === 0) {
        ctx.innerHTML = `<div class="xaman-context-label">Workspace Guide</div>
          <div class="xaman-hint" style="font-size:0.78rem;line-height:1.5;padding:0 4px">
            No agents yet. Hire agents to start.<br>
            <span style="color:var(--yellow)">Tip:</span> Start with <strong>cohere_and_coordinate</strong>.
          </div>
          <div class="xaman-context-item" style="cursor:pointer" onclick="document.getElementById('hire-modal')?.classList.add('visible'); XamanEk.close()">Hire an agent</div>`;
        return;
      }
      const classified = agents.map(a => {
        const tags = a.tags || [];
        const type = a.agent_type || "";
        let pattern = null;
        if (tags.includes("compound-agent")) pattern = this._INTERACTION_PATTERNS["compound-agent"];
        else if (type === "coherence" || tags.includes("coherence")) pattern = this._INTERACTION_PATTERNS["coherence"];
        else if (type === "creative" || tags.includes("creative")) pattern = this._INTERACTION_PATTERNS["creative"];
        else if (type === "research" || tags.includes("research")) pattern = this._INTERACTION_PATTERNS["research"];
        else if (type === "meta") pattern = this._INTERACTION_PATTERNS["meta"];
        return { ...a, pattern };
      });
      let html = '<div class="xaman-context-label">Interaction Guide</div>';
      for (const a of classified) {
        const name = a.display_alias || a.agent_name;
        const p = a.pattern || {};
        const icon = p.icon || "◆";
        const hint = p.hint || "Invoke with a query.";
        const examples = (p.examples || []).map(ex =>
          `<div onclick="XamanEk._insertWorkspaceQuery('${a.agent_name}','${this._esc(ex)}')" style="cursor:pointer;color:var(--aqua);font-size:0.72rem;padding:2px 0">→ ${this._esc(ex)}</div>`
        ).join("");
        html += `<div style="margin-bottom:10px;padding:4px 0;border-bottom:1px solid var(--bg2)">
          <div style="display:flex;align-items:center;gap:6px;margin-bottom:2px">
            <span>${icon}</span><strong style="color:var(--fg1);font-size:0.8rem">@${this._esc(name)}</strong>
          </div>
          <div style="font-size:0.72rem;color:var(--fg3);line-height:1.4;margin-bottom:3px">${this._esc(hint)}</div>
          ${examples}
        </div>`;
      }
      html += `<div style="margin-top:8px;display:flex;gap:6px;flex-wrap:wrap">
        <div class="xaman-context-item" style="cursor:pointer" onclick="document.getElementById('hire-modal')?.classList.add('visible'); XamanEk.close()">+ Hire</div>
        <div class="xaman-context-item" style="cursor:pointer" onclick="XamanEk.close(); document.getElementById('chat-input')?.focus()">Chat</div>
      </div>`;
      ctx.innerHTML = html;
    } catch (_) { ctx.innerHTML = ""; }
  },

  _insertWorkspaceQuery(agentName, example) {
    const input = document.getElementById("chat-input");
    if (input) {
      input.value = `@${agentName} ${example}`;
      input.focus();
      input.style.height = "auto";
      input.style.height = Math.min(input.scrollHeight, 120) + "px";
    }
    this.close();
  },

  // ── Help ───────────────────────────────────────────────────────────────────
  _renderHelp(query) {
    const routes = [
      { match: ["fork", "duplicate", "copy"], title: "Fork an agent", steps: ["Navigate to the agent page", "Click Fork in the Manage tab", "Choose what to include: ontology, embeddings", "Fork appears in your dashboard as a draft"], link: "/catalogue" },
      { match: ["create", "new agent", "build", "make"], title: "Create an agent", steps: ["Dashboard → + New Agent", "Fill the wizard: basics, model, tools, prompt, review", "Save as draft, publish when ready"], link: "/agents/new" },
      { match: ["eval", "test", "evaluate", "quality"], title: "Run an eval", steps: ["Agent page → Eval tab", "Add test cases (auto-seeded from sample_queries)", "Run Eval or Run + Judge for LLM scoring", "Check run history for regressions"] },
      { match: ["execute", "run", "query"], title: "Execute an agent", steps: ["Agent page → Execute Agent button", "Or: @agent_name your query in Xaman Ek spotlight (Ctrl+K)"] },
      { match: ["credit", "buy", "cost", "faucet"], title: "Credits", steps: ["Dashboard → Buy Credits", "Execution: 1cr per 1k tokens + 10% gas", "Admin: @faucet <user> <amount>"], link: "/dashboard" },
      { match: ["dream", "consolidat", "ontology", "knowledge"], title: "Dreaming & consolidation", steps: ["Agent → Manage tab → Dreaming Budget", "Top up dream credits", "Consolidate Now — extracts rules from episodes", "View in Knowledge tab"] },
      { match: ["workspace", "team", "hire", "composition"], title: "Workspaces & compositions", steps: ["Dashboard → My Workspaces → + New", "Hire agents (5cr each)", "@mention agents in chat — they see full history", "Add cohere_and_coordinate as your strategist"] },
      { match: ["coherence", "tec", "principle"], title: "Coherence evaluation", steps: ["Hire cohere_and_coordinate", "@cohere_and_coordinate How coherent is this workspace?", "Scores 7 Thagard principles", "Diagnoses and routes to HITL if needed"] },
      { match: ["valence", "personality", "affect"], title: "Agent valence", steps: ["Agent → Manage tab → Agent Valence section", "Set primary affect, arousal (calm↔urgent), valence (critical↔constructive)", "Shapes how the agent collaborates in compositions", "Composition Dreaming detects valence homophily"] },
      { match: ["simops", "bioreactor", "process", "cascade"], title: "SimOps process agents", steps: ["simops_advisor starts a 6-turn conversation to build your process config", "simops_cascade runs forward/backward energy balance", "simops_predictor learns from SOSA observation history", "simops_optimizer finds inputs to hit a production target"] },
    ];

    let best = null, bestScore = 0;
    for (const route of routes) {
      const score = route.match.filter(m => query.includes(m)).length;
      if (score > bestScore) { best = route; bestScore = score; }
    }

    if (!best) {
      document.getElementById("xaman-results").innerHTML = '<div class="xaman-hint">Try searching for an agent or @agent query</div>';
      return;
    }

    let html = `<div class="xaman-help"><div class="xaman-help-title">${this._esc(best.title)}</div><ol class="xaman-help-steps">`;
    best.steps.forEach(s => { html += `<li>${this._esc(s)}</li>`; });
    html += "</ol>";
    if (best.link) html += `<a href="${best.link}" class="xaman-help-link">Go there →</a>`;
    html += "</div>";
    document.getElementById("xaman-results").innerHTML = html;
  },

  // ── Recent ─────────────────────────────────────────────────────────────────
  _renderRecent() {
    const el = document.getElementById("xaman-recent");
    if (!el) return;
    const recent = this._getRecent();
    el.innerHTML = recent.length === 0
      ? '<div class="xaman-hint">Search agents, @agent query, or ask how to…</div>'
      : '<div class="xaman-recent-label">Recent</div>' +
        recent.map(q => `<div class="xaman-recent-item" onclick="XamanEk._input.value='${this._esc(q)}'; XamanEk._search('${this._esc(q)}')">${this._esc(q)}</div>`).join("");
  },

  _getRecent() {
    try { return JSON.parse(localStorage.getItem(this._recentKey)) || []; } catch { return []; }
  },

  _saveRecent(query) {
    let recent = this._getRecent().filter(q => q !== query);
    recent.unshift(query);
    localStorage.setItem(this._recentKey, JSON.stringify(recent.slice(0, 5)));
  },

  // ── Utilities ──────────────────────────────────────────────────────────────
  _esc(s) {
    if (!s) return "";
    const d = document.createElement("div");
    d.textContent = String(s);
    return d.innerHTML;
  },

  _renderMarkdown(text) {
    if (!text) return "";
    // Very lightweight: bold, code, line breaks
    return this._esc(text)
      .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
      .replace(/`(.+?)`/g, '<code style="background:var(--bg1);padding:1px 4px;border-radius:2px;font-size:0.9em">$1</code>')
      .replace(/\n/g, "<br>");
  },

  _timeAgo(isoString) {
    if (!isoString) return "";
    const diff = Date.now() - new Date(isoString).getTime();
    const m = Math.floor(diff / 60000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  },
};

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => XamanEk.init());
} else {
  XamanEk.init();
}
