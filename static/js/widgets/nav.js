// Nav widget — unified header navigation across all pages
const Nav = {
  init(opts = {}) {
    const current = opts.current || '';
    const title = opts.title || 'Agent Bestiary';
    const titleHref = opts.titleHref || '/';
    const subtitle = opts.subtitle || null;
    const back = opts.back || null; // { label, href }
    const stat = opts.stat || null; // { value, label, id }

    const header = document.createElement('header');
    header.className = 'nav-header';
    header.innerHTML = `
      <div class="nav-left">
        ${back ? `<a href="${back.href}" class="nav-back">${back.label}</a>` : ''}
        <a href="${titleHref}" class="nav-logo">${title}</a>
        ${subtitle ? `<span class="nav-subtitle">${subtitle}</span>` : ''}
        ${stat ? `<div class="nav-stat"><span class="nav-stat-value" id="${stat.id || ''}">${stat.value || '--'}</span><span class="nav-stat-label">${stat.label}</span></div>` : ''}
      </div>
      <div class="nav-right">
        <nav class="nav-links">
          <a href="/catalogue" class="${current === 'catalogue' ? 'active' : ''}">Catalogue</a>
          <a href="/dashboard" class="${current === 'dashboard' ? 'active' : ''}">Dashboard</a>
          <a href="/profile" class="${current === 'profile' ? 'active' : ''}">Profile</a>
        </nav>
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

    return header;
  }
};
