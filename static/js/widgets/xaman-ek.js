// Xaman Ek — the Bestiary Navigator
// Persistent search/execute companion, available on all pages after login
const XamanEk = {
  _panel: null,
  _input: null,
  _results: null,
  _visible: false,
  _debounce: null,
  _recentKey: "xaman-ek-recent",

  init() {
    // Create FAB (floating action button)
    const fab = document.createElement("button");
    fab.className = "xaman-fab";
    fab.innerHTML = "&#9733;"; // ★
    fab.title = "Xaman Ek — Bestiary Navigator (Ctrl+K)";
    fab.addEventListener("click", () => this.toggle());
    document.body.appendChild(fab);

    // Create panel
    const panel = document.createElement("div");
    panel.className = "xaman-panel";
    panel.innerHTML = `
      <div class="xaman-header">
        <span class="xaman-title">Xaman Ek</span>
        <button class="xaman-close" onclick="XamanEk.close()">&times;</button>
      </div>
      <div class="xaman-search">
        <input type="text" class="xaman-input" placeholder="Search specimens, @agent query, how do I..." autocomplete="off" />
      </div>
      <div class="xaman-body">
        <div class="xaman-context" id="xaman-context"></div>
        <div class="xaman-recent" id="xaman-recent"></div>
        <div class="xaman-results" id="xaman-results"></div>
      </div>
    `;
    document.body.appendChild(panel);

    this._panel = panel;
    this._input = panel.querySelector(".xaman-input");
    this._results = panel.querySelector("#xaman-results");

    // Wire search
    this._input.addEventListener("input", () => {
      clearTimeout(this._debounce);
      this._debounce = setTimeout(
        () => this._search(this._input.value.trim()),
        300,
      );
    });
    this._input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.close();
      if (e.key === "Enter") {
        const q = this._input.value.trim();
        if (q.startsWith("@")) this._executeAgent(q);
      }
    });

    // Keyboard shortcut: Ctrl+K
    document.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        this.toggle();
      }
    });

    // Show recent + context on init
    this._renderRecent();
  },

  toggle() {
    this._visible ? this.close() : this.open();
  },

  open() {
    if (!this._panel) return;
    this._panel.classList.add("visible");
    this._visible = true;
    this._input.value = "";
    this._results.innerHTML = "";
    this._renderRecent();
    this._renderContext();
    setTimeout(() => this._input.focus(), 50);
  },

  close() {
    if (!this._panel) return;
    this._panel.classList.remove("visible");
    this._visible = false;
  },

  async _search(query) {
    if (!query) {
      this._results.innerHTML = "";
      this._renderRecent();
      return;
    }

    // Hide context + recent when searching
    const ctx = document.getElementById("xaman-context");
    const rec = document.getElementById("xaman-recent");
    if (ctx) ctx.innerHTML = "";
    if (rec) rec.innerHTML = "";

    // @agent syntax — show execute hint
    if (query.startsWith("@")) {
      const parts = query.substring(1).split(/\s+/);
      const agentName = parts[0] || "";
      const agentQuery = parts.slice(1).join(" ");
      if (!agentQuery) {
        this._results.innerHTML = `<div class="xaman-hint">Type a query after @${this._esc(agentName)} and press Enter</div>`;
      } else {
        this._results.innerHTML = `<div class="xaman-hint">Press Enter to execute @${this._esc(agentName)}</div>`;
      }
      return;
    }

    // Help routing — "how do I..." pattern
    const lq = query.toLowerCase();
    if (
      lq.startsWith("how do i") ||
      lq.startsWith("how to") ||
      lq.startsWith("help")
    ) {
      this._renderHelp(lq);
      return;
    }

    // Standard search
    this._results.innerHTML = '<div class="xaman-hint">Searching...</div>';

    try {
      const res = await fetch(
        `/api/agents?search=${encodeURIComponent(query)}&limit=6`,
      );
      if (!res.ok) throw new Error("Search failed");
      const data = await res.json();
      const agents = data.agents || [];

      if (agents.length === 0) {
        this._results.innerHTML =
          '<div class="xaman-hint">No specimens found</div>';
        return;
      }

      this._results.innerHTML = agents
        .map((a) => {
          const name = a.display_alias || a.agent_name || a.name;
          const desc = (a.description || a.metadata?.description || "").slice(
            0,
            80,
          );
          const tags = (a.tags || a.metadata?.tags || []).slice(0, 3);
          const id = a.agent_id;
          return `
          <a href="/agent/${id}" class="xaman-result">
            <div class="xaman-result-name">${this._esc(name)}</div>
            <div class="xaman-result-desc">${this._esc(desc)}</div>
            ${tags.length ? `<div class="xaman-result-tags">${tags.map((t) => `<span class="xaman-tag">${this._esc(t)}</span>`).join("")}</div>` : ""}
          </a>
        `;
        })
        .join("");

      this._saveRecent(query);
    } catch (err) {
      this._results.innerHTML = '<div class="xaman-hint">Search error</div>';
    }
  },

  // ── @agent execution ──
  async _executeAgent(raw) {
    const parts = raw.substring(1).split(/\s+/);
    const agentName = parts[0];
    const query = parts.slice(1).join(" ");
    if (!agentName || !query) return;

    this._results.innerHTML = `<div class="xaman-hint" style="color:var(--yellow)">Executing @${this._esc(agentName)}...</div>`;

    try {
      const res = await fetch(
        `/api/agents/${encodeURIComponent(agentName)}/execute`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ query }),
        },
      );

      if (res.status === 401) {
        this._results.innerHTML =
          '<div class="xaman-hint" style="color:var(--orange)">Sign in to execute agents</div>';
        return;
      }
      if (!res.ok) {
        const err = await res.text().catch(() => "Unknown error");
        this._results.innerHTML = `<div class="xaman-hint" style="color:var(--red)">Error: ${this._esc(err)}</div>`;
        return;
      }

      const data = await res.json();
      const answer =
        data.answer || data.result?.answer || data.result?.summary || "";
      const confidence = data.confidence ?? data.result?.confidence;
      const evidence = data.evidence || data.result?.evidence || [];

      let html = `<div class="xaman-exec-result">`;
      html += `<div class="xaman-exec-agent">@${this._esc(agentName)}</div>`;

      if (confidence != null) {
        const pct = Math.round(confidence * 100);
        const color =
          pct >= 70
            ? "var(--green)"
            : pct >= 40
              ? "var(--yellow)"
              : "var(--red)";
        html += `<div style="font-size:0.8em;color:${color};margin-bottom:6px">Confidence: ${pct}%</div>`;
      }

      if (answer) {
        html += `<div class="xaman-exec-answer">${this._esc(answer).substring(0, 500)}</div>`;
      }

      if (evidence.length > 0) {
        html += '<div style="margin-top:6px;font-size:0.8em;color:var(--fg3)">';
        evidence.slice(0, 3).forEach((e) => {
          const text =
            typeof e === "string"
              ? e
              : e.description || e.text || e.finding || "";
          if (text)
            html += `<div style="margin-bottom:2px">· ${this._esc(text).substring(0, 120)}</div>`;
        });
        html += "</div>";
      }

      html += `<a href="/agent/${encodeURIComponent(agentName)}" class="xaman-exec-link">View specimen →</a>`;
      html += "</div>";
      this._results.innerHTML = html;

      this._saveRecent(raw);
    } catch (e) {
      this._results.innerHTML = `<div class="xaman-hint" style="color:var(--red)">Network error: ${this._esc(e.message)}</div>`;
    }
  },

  // ── Context-aware suggestions ──
  _renderContext() {
    const ctx = document.getElementById("xaman-context");
    if (!ctx) return;

    const path = window.location.pathname;
    const suggestions = [];

    if (
      path.startsWith("/agent/") &&
      !path.includes("/ontology") &&
      !path.includes("/projector")
    ) {
      const agentId = path.split("/")[2];
      if (agentId) {
        suggestions.push({
          label: "View ontology",
          href: `/agent/${agentId}/ontology`,
        });
        suggestions.push({
          label: "Embedding projector",
          href: `/agent/${agentId}/projector`,
        });
        suggestions.push({
          label: "Execute inline",
          action: `XamanEk._input.value='@${agentId} '; XamanEk._input.focus()`,
        });
      }
    } else if (path.startsWith("/agent/") && path.includes("/ontology")) {
      const agentId = path.split("/")[2];
      suggestions.push({
        label: "Back to specimen",
        href: `/agent/${agentId}`,
      });
      suggestions.push({
        label: "Embedding projector",
        href: `/agent/${agentId}/projector`,
      });
    } else if (path.startsWith("/agent/") && path.includes("/projector")) {
      const agentId = path.split("/")[2];
      suggestions.push({
        label: "Back to specimen",
        href: `/agent/${agentId}`,
      });
      suggestions.push({
        label: "View ontology",
        href: `/agent/${agentId}/ontology`,
      });
    } else if (path === "/catalogue" || path === "/") {
      suggestions.push({
        label: "Search specimens",
        action: "XamanEk._input.value=''; XamanEk._input.focus()",
      });
      suggestions.push({
        label: "Bestiary projector",
        href: "/projections/bestiary",
      });
      suggestions.push({ label: "Create agent", href: "/agents/new" });
    } else if (path === "/dashboard") {
      suggestions.push({ label: "Create agent", href: "/agents/new" });
      suggestions.push({
        label: "Assemble composition",
        action: "document.getElementById('create-ws-btn')?.click()",
      });
      suggestions.push({ label: "Browse catalogue", href: "/catalogue" });
    }

    if (suggestions.length === 0) {
      ctx.innerHTML = "";
      return;
    }

    ctx.innerHTML =
      '<div class="xaman-context-label">Quick actions</div>' +
      suggestions
        .map((s) => {
          if (s.href) {
            return `<a href="${s.href}" class="xaman-context-item">${this._esc(s.label)}</a>`;
          }
          return `<div class="xaman-context-item" onclick="${s.action}" style="cursor:pointer">${this._esc(s.label)}</div>`;
        })
        .join("");
  },

  // ── Help routing ──
  _renderHelp(query) {
    const routes = [
      {
        match: ["fork", "duplicate", "copy"],
        title: "Fork an agent",
        steps: [
          "Navigate to the agent page",
          'Click "Fork This Agent" (non-owner) or find it in the Manage tab (owner)',
          "Choose what to include: ontology, embeddings",
          "The fork appears in your dashboard as a draft",
        ],
        link: "/catalogue",
      },
      {
        match: ["create", "new agent", "build", "make"],
        title: "Create an agent",
        steps: [
          "Go to Dashboard → + New Agent (or /agents/new)",
          "Fill in the 5-step wizard: basics, model, tools, prompt, review",
          "Save as draft, then publish when ready",
        ],
        link: "/agents/new",
      },
      {
        match: ["eval", "test", "evaluate", "quality"],
        title: "Run an eval",
        steps: [
          "Open your agent → Eval tab",
          "Add test cases (or they auto-seed from sample queries)",
          'Click "Run Eval" or "Run + Judge" for LLM scoring',
          "Check run history for regressions",
        ],
      },
      {
        match: ["execute", "run", "query"],
        title: "Execute an agent",
        steps: [
          'Open the agent page → click "Execute Agent"',
          "Type your query and press Run",
          "Or use Xaman Ek: @agent_name your query",
        ],
      },
      {
        match: ["credit", "buy", "pay", "cost"],
        title: "Credits & costs",
        steps: [
          "Go to Dashboard → Buy Credits section",
          "Execution costs: 1 credit per 1000 tokens + 10% gas",
          "Other costs: publishing (1cr), avatar generation (3cr), eval runs (2cr)",
        ],
        link: "/dashboard",
      },
      {
        match: ["dream", "consolidat", "ontology", "knowledge"],
        title: "Dreaming & consolidation",
        steps: [
          "Open your agent → Manage tab → Dreaming Budget",
          "Top up dream credits from your wallet",
          'Click "Consolidate Now" to extract rules from episodes',
          "View results in Knowledge tab → ontology/projector links",
        ],
      },
      {
        match: ["workspace", "team", "hire"],
        title: "Workspaces",
        steps: [
          "Go to Dashboard → My Workspaces → + New Workspace",
          "Hire agents to your workspace (5 credits each)",
          "Chat with agents, share context, fund the workspace budget",
        ],
        link: "/dashboard",
      },
      {
        match: ["publish", "public", "share", "visib"],
        title: "Publishing an agent",
        steps: [
          "Open your agent → Manage tab",
          'Set visibility to "public"',
          'Click "Publish" (runs validation checks first)',
          "Costs 1 credit; agent appears in the catalogue",
        ],
      },
    ];

    // Find best match
    let best = null;
    let bestScore = 0;
    for (const route of routes) {
      const score = route.match.filter((m) => query.includes(m)).length;
      if (score > bestScore) {
        best = route;
        bestScore = score;
      }
    }

    if (!best) {
      this._results.innerHTML =
        '<div class="xaman-hint">Try searching for an agent or using @agent to execute</div>';
      return;
    }

    let html = `<div class="xaman-help">`;
    html += `<div class="xaman-help-title">${this._esc(best.title)}</div>`;
    html += '<ol class="xaman-help-steps">';
    best.steps.forEach((s) => {
      html += `<li>${this._esc(s)}</li>`;
    });
    html += "</ol>";
    if (best.link) {
      html += `<a href="${best.link}" class="xaman-help-link">Go there →</a>`;
    }
    html += "</div>";
    this._results.innerHTML = html;
  },

  _renderRecent() {
    const el = document.getElementById("xaman-recent");
    if (!el) return;
    const recent = this._getRecent();
    if (recent.length === 0) {
      el.innerHTML =
        '<div class="xaman-hint">Type to search the bestiary</div>';
      return;
    }
    el.innerHTML =
      '<div class="xaman-recent-label">Recent</div>' +
      recent
        .map(
          (q) =>
            `<div class="xaman-recent-item" onclick="XamanEk._input.value='${this._esc(q)}'; XamanEk._search('${this._esc(q)}')">${this._esc(q)}</div>`,
        )
        .join("");
  },

  _getRecent() {
    try {
      return JSON.parse(localStorage.getItem(this._recentKey)) || [];
    } catch {
      return [];
    }
  },

  _saveRecent(query) {
    let recent = this._getRecent().filter((q) => q !== query);
    recent.unshift(query);
    recent = recent.slice(0, 5);
    localStorage.setItem(this._recentKey, JSON.stringify(recent));
  },

  _esc(s) {
    if (!s) return "";
    const d = document.createElement("div");
    d.textContent = s;
    return d.innerHTML;
  },
};

// Auto-init after DOM ready
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => XamanEk.init());
} else {
  XamanEk.init();
}
