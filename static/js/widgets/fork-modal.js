/**
 * Fork Modal — shows pricing and options, submits fork request.
 * Depends on: api.js, toast.js, modal.js
 */
const ForkModal = {
  _agentId: null,
  _pricing: null,

  async open(agentId) {
    this._agentId = agentId;
    let el = document.getElementById('fork-modal');
    if (!el) {
      el = this._createDOM();
      document.body.appendChild(el);
      Modal.init('fork-modal');
    }

    // Fetch agent to get fork_pricing
    try {
      const agent = await API.get(`/api/agents/${agentId}`);
      this._pricing = agent.fork_pricing || { base_price: 0 };
    } catch {
      this._pricing = { base_price: 0 };
    }

    this._render();
    Modal.open('fork-modal');
  },

  _render() {
    const p = this._pricing;
    const baseCost = 2; // platform gas (fork_base)
    const authorBase = p.base_price || 0;
    const ontologyPrice = p.ontology_price;
    const embeddingPrice = p.embedding_price;

    const body = document.getElementById('fork-modal-body');
    body.innerHTML = `
      <div class="fork-pricing-grid">
        <div class="fork-line"><span>Platform fee</span><span>${baseCost} cr</span></div>
        <div class="fork-line"><span>Author base price</span><span>${authorBase} cr</span></div>
        ${ontologyPrice != null ? `
        <div class="fork-line fork-option">
          <label><input type="checkbox" id="fork-ontology"> Include ontology</label>
          <span>+${ontologyPrice} cr</span>
        </div>` : ''}
        ${embeddingPrice != null ? `
        <div class="fork-line fork-option">
          <label><input type="checkbox" id="fork-embeddings"> Include embeddings</label>
          <span>+${embeddingPrice} cr</span>
        </div>` : ''}
        <div class="fork-line fork-total"><span>Total</span><span id="fork-total">${baseCost + authorBase} cr</span></div>
      </div>
    `;

    // Wire up checkboxes to recalculate total
    const recalc = () => {
      let total = baseCost + authorBase;
      const ontoCb = document.getElementById('fork-ontology');
      const embCb = document.getElementById('fork-embeddings');
      if (ontoCb && ontoCb.checked) total += ontologyPrice || 0;
      if (embCb && embCb.checked) total += embeddingPrice || 0;
      document.getElementById('fork-total').textContent = total + ' cr';
    };
    const ontoCb = document.getElementById('fork-ontology');
    const embCb = document.getElementById('fork-embeddings');
    if (ontoCb) ontoCb.addEventListener('change', recalc);
    if (embCb) embCb.addEventListener('change', recalc);
  },

  async _submit() {
    const ontoCb = document.getElementById('fork-ontology');
    const embCb = document.getElementById('fork-embeddings');
    const body = {
      include_ontology: ontoCb ? ontoCb.checked : false,
      include_embeddings: embCb ? embCb.checked : false,
    };

    try {
      const result = await API.post(`/api/agents/${this._agentId}/fork`, body);
      Modal.close('fork-modal');
      Toast.show(`Forked! New agent: ${result.agent_name} (cost: ${result.total_cost} cr)`, 'success');
      setTimeout(() => { window.location.href = `/agent/${result.agent_name}`; }, 1500);
    } catch (e) {
      Toast.show(e.message || 'Fork failed', 'error');
    }
  },

  _createDOM() {
    const overlay = document.createElement('div');
    overlay.id = 'fork-modal';
    overlay.className = 'modal-overlay';
    overlay.innerHTML = `
      <div class="modal">
        <div class="modal-header">
          <h3>Fork Agent</h3>
          <button class="modal-close" onclick="Modal.close('fork-modal')">&times;</button>
        </div>
        <div class="modal-body" id="fork-modal-body">Loading...</div>
        <div class="modal-actions">
          <button class="btn" onclick="Modal.close('fork-modal')">Cancel</button>
          <button class="btn btn-primary" onclick="ForkModal._submit()">Fork</button>
        </div>
      </div>
    `;
    return overlay;
  }
};
