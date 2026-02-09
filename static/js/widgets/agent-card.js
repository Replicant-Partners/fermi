// Agent card rendering widget — shared across catalogue, dashboard, workspace
const AgentCard = {
  // Deterministic hue from agent name
  avatarHue(name) {
    const hues = [200, 140, 30, 340, 270, 50, 170, 10];
    return hues[(name || "").charCodeAt(0) % hues.length];
  },

  // Escape HTML
  _esc(s) {
    const d = document.createElement("div");
    d.textContent = s || "";
    return d.innerHTML;
  },

  // Catalogue grid card (index.html)
  renderCard(agent, opts = {}) {
    const agentId = agent.agent_name || agent.name;
    const displayName =
      agent.display_alias && agent.display_alias !== ""
        ? agent.display_alias
        : agentId;
    const initial = (displayName || "?").charAt(0).toUpperCase();
    const hue = this.avatarHue(agentId);
    const description =
      agent.description ||
      agent.metadata?.description ||
      "A mysterious agent";
    const tier = agent.tier || "curated";
    const executions =
      agent.execution_stats?.total_executions ||
      agent.usage?.total_executions ||
      0;
    const model = (
      agent.model ||
      agent.capabilities?.model ||
      "claude"
    )
      .split("/")
      .pop()
      .split("-")
      .slice(0, 2)
      .join("-");
    const tags = agent.tags || agent.metadata?.tags || [];
    const tagHtml = tags
      .slice(0, 3)
      .map((t) => `<span class="tag">${this._esc(t)}</span>`)
      .join("");

    return `
      <div class="specimen-header">
        <div class="specimen-avatar" id="avatar-${CSS.escape(agentId)}"
             style="background: hsl(${hue}, 45%, 35%); color: #fff; font-size: 1.4em; font-weight: 300;">
          ${initial}
        </div>
        <div>
          <div class="specimen-name">${this._esc(displayName)}</div>
          ${agent.display_alias && agent.display_alias !== "" ? `<div class="specimen-system-name">${this._esc(agentId)}</div>` : ""}
          <span class="badge">${this._esc(tier)}</span>
        </div>
      </div>
      <div class="specimen-body">
        <div class="specimen-description">${this._esc(description)}</div>
        ${tagHtml ? `<div class="specimen-tags">${tagHtml}</div>` : ""}
        <div class="specimen-footer">
          <div class="specimen-stat">
            <span class="meta-label">Runs</span>
            <span class="specimen-stat-value">${executions}</span>
          </div>
          <div class="specimen-stat">
            <span class="meta-label">Model</span>
            <span class="specimen-stat-value">${this._esc(model)}</span>
          </div>
          <div class="specimen-stat">
            <span class="meta-label">Type</span>
            <span class="specimen-stat-value">${this._esc(agent.agent_type || "—")}</span>
          </div>
        </div>
      </div>`;
  },

  // Dashboard compact row
  renderRow(agent, opts = {}) {
    const agentId = agent.agent_name || agent.name;
    const displayName =
      agent.display_alias && agent.display_alias !== ""
        ? agent.display_alias
        : agentId;
    const initial = (displayName || "?").charAt(0).toUpperCase();
    const hue = this.avatarHue(agentId);
    return `
      <div class="agent-dot" style="background:hsl(${hue},45%,35%);color:#fff;font-size:0.55rem;display:flex;align-items:center;justify-content:center;width:18px;height:18px;border-radius:50%">${initial}</div>
      <span class="member-name">${this._esc(displayName)}</span>`;
  },

  // Workspace hire/add modal item
  renderModalItem(agent, opts = {}) {
    const agentId = agent.agent_name || agent.name;
    const displayName =
      agent.display_alias && agent.display_alias !== ""
        ? agent.display_alias
        : agentId;
    const initial = (displayName || "?").charAt(0).toUpperCase();
    const hue = this.avatarHue(agentId);
    const desc =
      agent.description ||
      agent.metadata?.description ||
      "";
    return `
      <div class="agent-dot" style="background:hsl(${hue},45%,35%);color:#fff;font-size:0.7rem;display:flex;align-items:center;justify-content:center;width:24px;height:24px;border-radius:50%">${initial}</div>
      <div style="flex:1;min-width:0">
        <div style="font-weight:500">${this._esc(displayName)}</div>
        ${desc ? `<div style="font-size:0.75em;color:var(--gray);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${this._esc(desc)}</div>` : ""}
      </div>`;
  },
};
