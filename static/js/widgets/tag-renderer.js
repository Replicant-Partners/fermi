// Tag renderer — renders tag pills with category-based styling
const TagRenderer = {
  // Category → color mappings (Gruvbox palette)
  _colors: {
    status: { success: 'var(--green)', error: 'var(--red)', timeout: 'var(--orange)', 'low-confidence': 'var(--yellow)', partial: 'var(--yellow)' },
    tool: 'var(--aqua)',
    iterations: 'var(--blue)',
    cost: { free: 'var(--green)', low: 'var(--green)', medium: 'var(--yellow)', high: 'var(--orange)' },
    model: 'var(--purple)',
    confidence: { high: 'var(--green)', medium: 'var(--yellow)', low: 'var(--red)' },
    // Why the LLM stopped. `end_turn`/`stop` are clean exits; `max_tokens`,
    // `length` and `tool_use` mean the run was cut short and the answer (if
    // any) came from a flush turn. Colour accordingly so a list of episodes
    // shows at a glance which runs actually finished.
    stop: {
      end_turn: 'var(--green)',
      stop: 'var(--green)',
      max_tokens: 'var(--red)',
      length: 'var(--red)',
      tool_use: 'var(--orange)',
      tool_calls: 'var(--orange)',
      refusal: 'var(--red)',
      content_filter: 'var(--red)',
      pause_turn: 'var(--yellow)',
    },
    degraded: 'var(--orange)',
    // How the caller decided to ask. `undeclared` means the agent published
    // no output contract, so it got a generic prompt and its reply is the
    // likeliest not to parse — a card-quality signal, not an agent-quality
    // one, so it reads as a caution rather than an error.
    qsrc: {
      agent_template: 'var(--green)',
      declared_contract: 'var(--green)',
      user_authored: 'var(--blue)',
      undeclared: 'var(--yellow)',
    },
    // Whether the prompt matched an input the agent actually advertises.
    ibind: {
      no_text_input: 'var(--red)',
      undeclared: 'var(--yellow)',
    },
    recomposed: 'var(--blue)',
  },

  color(tag) {
    // Split on the first colon only, so a value containing one (e.g. a
    // model name like `model:claude-3:latest`) still resolves its category.
    const idx = tag.indexOf(':');
    const cat = idx === -1 ? tag : tag.slice(0, idx);
    const val = idx === -1 ? '' : tag.slice(idx + 1);
    const entry = this._colors[cat];
    if (!entry) return 'var(--fg3)';
    if (typeof entry === 'string') return entry;
    return entry[val] || 'var(--fg3)';
  },

  render(tags, opts = {}) {
    if (!tags || tags.length === 0) return '';
    const max = opts.max || 6;
    const shown = tags.slice(0, max);
    return '<span class="tag-pills">' +
      shown.map(t => {
        const c = this.color(t);
        return `<span class="tag-pill" style="border-color:${c};color:${c}">${this._esc(t)}</span>`;
      }).join('') +
      (tags.length > max ? `<span class="tag-pill" style="color:var(--gray)">+${tags.length - max}</span>` : '') +
      '</span>';
  },

  _esc(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }
};
