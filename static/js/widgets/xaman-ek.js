// Xaman Ek — the Bestiary Navigator
// Persistent search/execute companion, available on all pages after login
const XamanEk = {
  _panel: null,
  _input: null,
  _results: null,
  _visible: false,
  _debounce: null,
  _recentKey: "xaman-ek-recent",
  _currentUser: null,
  _userLoaded: false,

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
        if (q.startsWith("@faucet ")) {
          this._executeFaucet(q);
        } else if (q.startsWith("@")) {
          this._executeAgent(q);
        }
      }
    });

    // Keyboard shortcut: Ctrl+K
    document.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        this.toggle();
      }
    });

    // Load current user (for admin commands)
    this._loadUser();

    // Show recent + context on init
    this._renderRecent();
  },

  async _loadUser() {
    if (this._userLoaded) return;
    try {
      const res = await fetch("/api/auth/me");
      if (res.ok) this._currentUser = await res.json();
    } catch {}
    this._userLoaded = true;
  },

  _isAdmin() {
    return this._currentUser?.role === "admin";
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

    // @faucet syntax — admin credit grant
    if (query.startsWith("@faucet")) {
      if (!this._isAdmin()) {
        this._results.innerHTML =
          '<div class="xaman-hint" style="color:var(--red)">Admin access required</div>';
        return;
      }
      const parts = query.substring(8).trim().split(/\s+/);
      const userSearch = parts[0] || "";
      const amount = parts[1] || "";
      if (!userSearch) {
        this._results.innerHTML =
          '<div class="xaman-hint">@faucet &lt;user&gt; &lt;amount&gt; — grant credits to a user<br><span style="color:var(--fg3);font-size:0.9em">@faucet self 500 — grant to yourself</span></div>';
      } else if (!amount) {
        if (userSearch === "self") {
          this._results.innerHTML =
            '<div class="xaman-hint">@faucet self &lt;amount&gt; — type the credit amount</div>';
        } else {
          this._faucetSearchUsers(userSearch);
        }
      } else {
        this._results.innerHTML = `<div class="xaman-hint">Press Enter to grant <strong>${this._esc(amount)}</strong> credits to <strong>${this._esc(userSearch)}</strong></div>`;
      }
      return;
    }

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

  // ── @faucet — admin credit grant ──
  async _faucetSearchUsers(search) {
    this._results.innerHTML =
      '<div class="xaman-hint">Searching users...</div>';
    try {
      const res = await fetch(
        `/api/admin/users?search=${encodeURIComponent(search)}&limit=5`,
      );
      if (res.status === 403) {
        this._results.innerHTML =
          '<div class="xaman-hint" style="color:var(--red)">Admin access required</div>';
        return;
      }
      if (!res.ok) throw new Error("Search failed");
      const data = await res.json();
      const users = data.users || [];
      if (users.length === 0) {
        this._results.innerHTML =
          '<div class="xaman-hint">No users found</div>';
        return;
      }
      this._results.innerHTML = users
        .map((u) => {
          const name = u.display_name || u.email || u.user_id;
          const detail = u.email && u.email !== name ? u.email : u.user_id;
          return `<div class="xaman-result" style="cursor:pointer" data-uid="${this._esc(u.user_id)}" onclick="XamanEk._faucetSelectUser(this.dataset.uid)">
          <div class="xaman-result-name">${this._esc(name)}</div>
          <div class="xaman-result-desc">${this._esc(detail)} · ${this._esc(u.role || "developer")}</div>
        </div>`;
        })
        .join("");
    } catch (e) {
      this._results.innerHTML =
        '<div class="xaman-hint" style="color:var(--red)">Error searching users</div>';
    }
  },

  _faucetSelectUser(userId) {
    // Replace the search term with the selected user_id, keep cursor at end for amount
    const current = this._input.value.trim();
    const parts = current.substring(8).trim().split(/\s+/);
    const amount = parts[1] || "";
    this._input.value = `@faucet ${userId} ${amount}`;
    this._input.focus();
    // Trigger search to update hint
    this._search(this._input.value.trim());
  },

  async _executeFaucet(raw) {
    const parts = raw.substring(8).trim().split(/\s+/);
    let userSearch = parts[0];
    const amount = parseInt(parts[1], 10);

    // @faucet self <amount> — shorthand for granting to yourself
    if (userSearch === "self" && this._currentUser?.user_id) {
      userSearch = this._currentUser.user_id;
    }

    if (!userSearch || !amount || isNaN(amount) || amount < 1) {
      this._results.innerHTML =
        '<div class="xaman-hint" style="color:var(--orange)">Usage: @faucet &lt;user_id&gt; &lt;amount&gt;</div>';
      return;
    }

    if (amount > 10000) {
      this._results.innerHTML =
        '<div class="xaman-hint" style="color:var(--orange)">Max 10,000 credits per grant</div>';
      return;
    }

    this._results.innerHTML = `<div class="xaman-hint" style="color:var(--yellow)">Granting ${amount} credits to ${this._esc(userSearch)}...</div>`;

    try {
      const res = await fetch(
        `/api/admin/users/${encodeURIComponent(userSearch)}/grant`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            credits: amount,
            reason: "Faucet grant via Xaman Ek",
          }),
        },
      );

      if (res.status === 403) {
        this._results.innerHTML =
          '<div class="xaman-hint" style="color:var(--red)">Admin access required</div>';
        return;
      }
      if (!res.ok) {
        const err = await res.text().catch(() => "Unknown error");
        this._results.innerHTML = `<div class="xaman-hint" style="color:var(--red)">Error: ${this._esc(err)}</div>`;
        return;
      }

      const data = await res.json();
      this._results.innerHTML = `<div class="xaman-exec-result">
        <div class="xaman-exec-agent" style="color:var(--green)">Granted</div>
        <div style="margin:8px 0;font-size:0.95em">${data.credits} credits → ${this._esc(data.user_id)}</div>
        <div style="font-size:0.8em;color:var(--fg3)">${this._esc(data.reason)}</div>
      </div>`;

      this._input.value = "";
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
    } else if (path.startsWith("/workspace/")) {
      // Workspace-aware context — fetch agents and show interaction guide
      this._renderWorkspaceContext(ctx);
      return;
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

  // ── Interaction patterns knowledge base ──
  // Maps agent tags/types to interaction hints
  _INTERACTION_PATTERNS: {
    // Compound agents — orchestrate others
    "compound-agent": {
      icon: "&#9881;",
      role: "Orchestrator",
      hint: "Coordinates other agents. @mention it with a high-level goal and it will plan the work.",
      examples: [
        "Create a post about [topic] for [platform]",
        "Run the full pipeline on this brief",
      ],
    },
    // Coherence agents — evaluate and coordinate
    coherence: {
      icon: "&#9878;",
      role: "Evaluator",
      hint: "Reads the conversation and scores coherence. Ask it to evaluate after other agents have contributed.",
      examples: [
        "How coherent is this workspace right now?",
        "Evaluate the conversation and tell each agent what to focus on",
        "We seem stuck — diagnose the coordination failure",
      ],
    },
    // Creative agents — make things
    creative: {
      icon: "&#9998;",
      role: "Creator",
      hint: "Generates or transforms content. Give it specific creative direction.",
      examples: [
        "Apply a vintage style to the image at images/draft.png",
        "Generate an image of [description]",
      ],
    },
    // Research agents — find things
    research: {
      icon: "&#128270;",
      role: "Researcher",
      hint: "Investigates topics and returns evidence-based analysis.",
      examples: [
        "Research [topic] and summarize key findings",
        "What are the trends in [domain]?",
      ],
    },
    // Meta agents — guide and coach
    meta: {
      icon: "&#9733;",
      role: "Guide",
      hint: "Helps you navigate the platform and understand how to use other agents.",
      examples: [
        "What agents should I hire for [goal]?",
        "How does [feature] work?",
      ],
    },
  },

  // Workspace-specific context rendering
  async _renderWorkspaceContext(ctx) {
    const wsId = window.location.pathname.split("/").pop();
    if (!wsId) {
      ctx.innerHTML = "";
      return;
    }

    ctx.innerHTML =
      '<div class="xaman-context-label">Loading workspace guide...</div>';

    try {
      const res = await fetch(`/api/workspaces/${wsId}`);
      if (!res.ok) {
        ctx.innerHTML = "";
        return;
      }
      const ws = await res.json();
      const agents = ws.agents || [];

      if (agents.length === 0) {
        ctx.innerHTML = `
          <div class="xaman-context-label">Workspace Guide</div>
          <div class="xaman-hint" style="font-size:0.78rem;line-height:1.5;padding:0 4px">
            No agents yet. Hire agents to start collaborating.<br>
            <span style="color:var(--yellow)">Tip:</span> Try hiring <strong>cohere_and_coordinate</strong> — it can evaluate and guide any team.
          </div>
          <div class="xaman-context-item" style="cursor:pointer" onclick="document.getElementById('hire-modal')?.classList.add('visible'); XamanEk.close()">Hire an agent</div>`;
        return;
      }

      // Classify agents by their interaction pattern
      const classified = agents.map((a) => {
        const tags = a.tags || [];
        const type = a.agent_type || "";
        let pattern = null;

        // Check tags first (compound-agent is most specific)
        if (tags.includes("compound-agent")) {
          pattern = this._INTERACTION_PATTERNS["compound-agent"];
        } else if (type === "coherence" || tags.includes("coherence")) {
          pattern = this._INTERACTION_PATTERNS["coherence"];
        } else if (type === "creative" || tags.includes("creative")) {
          pattern = this._INTERACTION_PATTERNS["creative"];
        } else if (type === "research" || tags.includes("research")) {
          pattern = this._INTERACTION_PATTERNS["research"];
        } else if (type === "meta" || tags.includes("meta")) {
          pattern = this._INTERACTION_PATTERNS["meta"];
        }

        return { ...a, pattern };
      });

      // Build the guide HTML
      let html = '<div class="xaman-context-label">Interaction Guide</div>';

      // Group by role
      const groups = {};
      for (const a of classified) {
        const role = a.pattern ? a.pattern.role : "Specialist";
        if (!groups[role]) groups[role] = [];
        groups[role].push(a);
      }

      // Render each group
      for (const [role, groupAgents] of Object.entries(groups)) {
        const pattern = groupAgents[0].pattern;
        const icon = pattern ? pattern.icon : "&#9670;";

        for (const a of groupAgents) {
          const name = a.display_alias || a.agent_name;
          const p = a.pattern || {};
          const hint = p.hint || "Invoke with a query.";
          const examples = p.examples || [];
          const exampleHtml =
            examples.length > 0
              ? examples
                  .map(
                    (ex) =>
                      `<div class="xaman-ws-example" onclick="XamanEk._insertWorkspaceQuery('${a.agent_name}', '${this._esc(ex)}')" style="cursor:pointer;padding:2px 0;color:var(--aqua);font-size:0.72rem" title="Click to insert">&rarr; ${this._esc(ex)}</div>`,
                  )
                  .join("")
              : "";

          html += `
            <div style="margin-bottom:10px;padding:4px 0;border-bottom:1px solid var(--bg2)">
              <div style="display:flex;align-items:center;gap:6px;margin-bottom:2px">
                <span style="font-size:0.85rem">${icon}</span>
                <strong style="color:var(--fg1);font-size:0.8rem">@${this._esc(name)}</strong>
                <span style="font-size:0.65rem;color:var(--fg3);background:var(--bg2);padding:0 4px;border-radius:2px">${this._esc(role)}</span>
              </div>
              <div style="font-size:0.72rem;color:var(--fg3);line-height:1.4;margin-bottom:3px">${this._esc(hint)}</div>
              ${exampleHtml}
            </div>`;
        }
      }

      // Multi-agent workflow tip
      const hasCoherence = classified.some(
        (a) => a.pattern && a.pattern.role === "Evaluator",
      );
      const hasCreator = classified.some(
        (a) =>
          a.pattern &&
          (a.pattern.role === "Creator" || a.pattern.role === "Orchestrator"),
      );

      if (hasCoherence && hasCreator) {
        html += `
          <div style="margin-top:6px;padding:6px;background:var(--bg1);border-radius:4px;font-size:0.72rem;line-height:1.5;color:var(--fg2)">
            <strong style="color:var(--yellow)">Multi-agent pattern:</strong><br>
            1. Ask the creator/orchestrator to produce work<br>
            2. Ask the evaluator to assess coherence<br>
            3. Feed the evaluation back to the creator<br>
            Each agent sees the full chat history.
          </div>`;
      } else if (agents.length >= 2) {
        html += `
          <div style="margin-top:6px;padding:6px;background:var(--bg1);border-radius:4px;font-size:0.72rem;line-height:1.5;color:var(--fg2)">
            <strong style="color:var(--yellow)">Tip:</strong> @mention one agent at a time.
            Each agent reads the full chat — they see what others said.
            You're the conductor threading the conversation.
          </div>`;
      }

      // Quick actions
      html += `
        <div style="margin-top:8px;display:flex;gap:6px;flex-wrap:wrap">
          <div class="xaman-context-item" style="cursor:pointer" onclick="document.getElementById('hire-modal')?.classList.add('visible'); XamanEk.close()">+ Hire</div>
          <div class="xaman-context-item" style="cursor:pointer" onclick="XamanEk.close(); document.getElementById('chat-input')?.focus()">Chat</div>
        </div>`;

      ctx.innerHTML = html;
    } catch (e) {
      ctx.innerHTML = "";
    }
  },

  // Insert an @mention query into the workspace chat input
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
        match: ["credit", "buy", "pay", "cost", "faucet", "grant"],
        title: "Credits & costs",
        steps: [
          "Go to Dashboard → Buy Credits section",
          "Execution costs: 1 credit per 1000 tokens + 10% gas",
          "Other costs: publishing (1cr), avatar generation (3cr), eval runs (2cr)",
          "Admin: use @faucet <user> <amount> to grant credits",
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
          "Chat with agents using @agent_name — they run with full tools",
          "Each agent reads the full chat history — they see what others said",
          "Use Ctrl+K in a workspace for an interaction guide per agent",
        ],
        link: "/dashboard",
      },
      {
        match: [
          "compound",
          "orchestrat",
          "pipeline",
          "multi-agent",
          "coordinate",
        ],
        title: "Compound agents in workspaces",
        steps: [
          "Compound agents orchestrate other agents (e.g. social_media_studio)",
          "Hire the compound agent + its specialist agents into one workspace",
          "@mention the compound agent with a high-level goal",
          "It will coordinate specialists — each gets full tool access",
          "Use cohere_and_coordinate to evaluate the team's coherence",
        ],
      },
      {
        match: ["coherence", "evaluate", "tec", "principle"],
        title: "Coherence evaluation",
        steps: [
          "Hire cohere_and_coordinate into your workspace",
          "@cohere_and_coordinate How coherent is this workspace?",
          "It reads chat history, runs TEC evaluation, diagnoses issues",
          "Scores 7 Thagard principles: Symmetry, Explanation, Analogy, Data Priority, Contradiction, Competition, Acceptability",
          "Use its feedback to guide other agents",
        ],
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
