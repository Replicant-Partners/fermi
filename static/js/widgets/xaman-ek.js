// Xaman Ek — the Bestiary Navigator
// Persistent search/execute companion, available on all pages after login
const XamanEk = {
  _panel: null,
  _input: null,
  _results: null,
  _visible: false,
  _debounce: null,
  _recentKey: 'xaman-ek-recent',

  init() {
    // Create FAB (floating action button)
    const fab = document.createElement('button');
    fab.className = 'xaman-fab';
    fab.innerHTML = '&#9733;'; // ★
    fab.title = 'Xaman Ek — Bestiary Navigator (Ctrl+K)';
    fab.addEventListener('click', () => this.toggle());
    document.body.appendChild(fab);

    // Create panel
    const panel = document.createElement('div');
    panel.className = 'xaman-panel';
    panel.innerHTML = `
      <div class="xaman-header">
        <span class="xaman-title">Xaman Ek</span>
        <button class="xaman-close" onclick="XamanEk.close()">&times;</button>
      </div>
      <div class="xaman-search">
        <input type="text" class="xaman-input" placeholder="Search specimens, @agent query..." autocomplete="off" />
      </div>
      <div class="xaman-body">
        <div class="xaman-recent" id="xaman-recent"></div>
        <div class="xaman-results" id="xaman-results"></div>
      </div>
    `;
    document.body.appendChild(panel);

    this._panel = panel;
    this._input = panel.querySelector('.xaman-input');
    this._results = panel.querySelector('#xaman-results');

    // Wire search
    this._input.addEventListener('input', () => {
      clearTimeout(this._debounce);
      this._debounce = setTimeout(() => this._search(this._input.value.trim()), 300);
    });
    this._input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.close();
    });

    // Keyboard shortcut: Ctrl+K
    document.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        this.toggle();
      }
    });

    // Show recent on init
    this._renderRecent();
  },

  toggle() {
    this._visible ? this.close() : this.open();
  },

  open() {
    if (!this._panel) return;
    this._panel.classList.add('visible');
    this._visible = true;
    this._input.value = '';
    this._results.innerHTML = '';
    this._renderRecent();
    setTimeout(() => this._input.focus(), 50);
  },

  close() {
    if (!this._panel) return;
    this._panel.classList.remove('visible');
    this._visible = false;
  },

  async _search(query) {
    if (!query) {
      this._results.innerHTML = '';
      this._renderRecent();
      return;
    }

    // @agent syntax — execute inline (Phase 4, stub for now)
    if (query.startsWith('@')) {
      this._results.innerHTML = '<div class="xaman-hint">Agent execution coming soon</div>';
      return;
    }

    this._results.innerHTML = '<div class="xaman-hint">Searching...</div>';

    try {
      const res = await fetch(`/api/agents?search=${encodeURIComponent(query)}&limit=6`);
      if (!res.ok) throw new Error('Search failed');
      const data = await res.json();
      const agents = data.agents || [];

      if (agents.length === 0) {
        this._results.innerHTML = '<div class="xaman-hint">No specimens found</div>';
        return;
      }

      this._results.innerHTML = agents.map(a => {
        const name = a.display_alias || a.agent_name || a.name;
        const desc = (a.description || a.metadata?.description || '').slice(0, 80);
        const tags = (a.tags || a.metadata?.tags || []).slice(0, 3);
        const id = a.agent_id;
        return `
          <a href="/agent/${id}" class="xaman-result">
            <div class="xaman-result-name">${this._esc(name)}</div>
            <div class="xaman-result-desc">${this._esc(desc)}</div>
            ${tags.length ? `<div class="xaman-result-tags">${tags.map(t => `<span class="xaman-tag">${this._esc(t)}</span>`).join('')}</div>` : ''}
          </a>
        `;
      }).join('');

      // Save to recent
      this._saveRecent(query);
    } catch (err) {
      this._results.innerHTML = `<div class="xaman-hint">Search error</div>`;
    }
  },

  _renderRecent() {
    const el = document.getElementById('xaman-recent');
    if (!el) return;
    const recent = this._getRecent();
    if (recent.length === 0) {
      el.innerHTML = '<div class="xaman-hint">Type to search the bestiary</div>';
      return;
    }
    el.innerHTML = '<div class="xaman-recent-label">Recent</div>' +
      recent.map(q => `<div class="xaman-recent-item" onclick="XamanEk._input.value='${this._esc(q)}'; XamanEk._search('${this._esc(q)}')">${this._esc(q)}</div>`).join('');
  },

  _getRecent() {
    try { return JSON.parse(localStorage.getItem(this._recentKey)) || []; }
    catch { return []; }
  },

  _saveRecent(query) {
    let recent = this._getRecent().filter(q => q !== query);
    recent.unshift(query);
    recent = recent.slice(0, 5);
    localStorage.setItem(this._recentKey, JSON.stringify(recent));
  },

  _esc(s) {
    if (!s) return '';
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }
};

// Auto-init after DOM ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => XamanEk.init());
} else {
  XamanEk.init();
}
