// Render checks for templates/ecology.html.
//
// Loads the page's render functions and exercises them against payloads
// shaped like the real /api/ecology/* responses -- no browser, no
// database. Specimen fixtures are read from actual agent_card.json files
// on disk, so the assertions run against authentic data shapes rather
// than what I imagined the shape to be.
//
// This view's value is entirely in *what it makes obvious*, so the
// assertions are about meaning, not markup: unreviewed third-party
// members must be flagged, platform-seeded ones must not be, "zero
// approvals" must never render as the green all-clear, and a declared
// contract must never be presented as membership.
//
// Run:  node tests/render/ecology_render_test.js
const fs = require('fs');
const path = require('path');
const ROOT = path.join(__dirname, '..', '..');

const html = fs.readFileSync(path.join(ROOT, 'templates', 'ecology.html'), 'utf8');
const src = html.match(/<script>([\s\S]*)<\/script>/)[1]
  .replace(/\(async function load\(\)[\s\S]*$/, '');   // drop the fetch bootstrap

// Minimal DOM: elements remember their innerHTML; querySelectorAll is a no-op
// because we assert on innerHTML, not on wiring.
const doc = {};
global.document = {
  getElementById: id => (doc[id] = doc[id] || { innerHTML: '' }),
  querySelectorAll: () => [],
  querySelector: () => ({ scrollTop: 0 }),
};
global.history = { replaceState() {} };
global.location = { search: '', pathname: '/ecology' };

// The page declares its functions with `const`, which does not leak out of
// an `eval`. Wrap in a Function body (non-strict, so declarations are
// function-scoped) and return a handle that closes over them. Getters and
// setters for SPEC/OVERVIEW because the page holds them in `let`.
const page = new Function(src + `
  return {
    binomial, groupKey, isFlagged, composition, renderSheet, renderFieldGuide,
    tax, valence, fermiHab, provOf,
    get SPEC() { return SPEC; },       set SPEC(v) { SPEC = v; },
    get OVERVIEW() { return OVERVIEW; }, set OVERVIEW(v) { OVERVIEW = v; },
  };`)();

const { binomial, groupKey, isFlagged, composition, renderSheet, renderFieldGuide, tax } = page;

// ── Fixtures from real cards on disk ──────────────────────────────
function card(name) {
  const p = path.join(ROOT, 'agents', 'curated', name, 'agent_card.json');
  const c = JSON.parse(fs.readFileSync(p, 'utf8'));
  return {
    agent_id: c.agent_id, agent_type: c.agent_type, tier: 'curated',
    description: c.description, llm_provider: 'anthropic',
    accepts: c.accepts || [], produces: c.produces || [],
    dependencies: c.dependencies || {}, capabilities: c.capabilities || {},
    metadata: c.metadata || {}, execution_stats: { total_executions: 0, total_cost_usd: 0 },
    habitats: [],
  };
}

const macro = card('macro_forecaster');
// A third-party agent holding fermi membership with no review behind it.
const sneaky = {
  agent_id: 'efra_forensic', agent_type: 'research', tier: 'community',
  description: 'FORENSIC is the risk and trust engine.',
  accepts: ['evidence'], produces: ['diagnosis'],
  capabilities: { fermi_contract: { finding_labels: ['red_flag'], multiplier_range: [0.5, 2.0] } },
  metadata: {}, execution_stats: {}, habitats: [{ orchestra: 'fermi', source: 'curated_seed' }],
};
const legit = { ...macro, habitats: [{ orchestra: 'fermi', source: 'approved' }] };

let fail = 0;
const ok = (cond, label) => { console.log((cond ? '  PASS  ' : '  FAIL  ') + label); if (!cond) fail++; };

// ── Specimen classification ───────────────────────────────────────
console.log('classification (from the real macro_forecaster card):');
ok(binomial(macro) === 'Analyticus macro_forecaster',
   `renders a binomial from taxonomy (got "${binomial(macro)}")`);
ok(groupKey(macro, 'family') === 'Investigatidae', 'groups by family');
ok(groupKey({ agent_id: 'x', metadata: {} }, 'family') === 'Incertae sedis',
   'an untaxonomised card falls into Incertae sedis, not "undefined"');

console.log('provenance:');
ok(isFlagged(sneaky) === true, 'community member with curated_seed IS flagged');
ok(isFlagged(legit) === false, 'community-visible but approved member is NOT flagged');
ok(isFlagged({ ...macro, tier: 'curated', habitats: [{ orchestra: 'fermi', source: 'curated_seed' }] }) === false,
   'platform-seeded curated member is NOT flagged (expected provenance)');
ok(isFlagged({ ...macro, habitats: [] }) === false, 'non-member is not flagged');

// ── Composition graph ─────────────────────────────────────────────
console.log('composition (produces -> accepts):');
page.SPEC = [macro, sneaky, { agent_id: 'consumer', accepts: ['evidence'], produces: [], metadata: {} }];
const comp = composition(macro);
ok(comp.feeds.some(f => f.id === 'consumer' && f.via.includes('evidence')),
   'macro_forecaster can feed an agent that accepts `evidence`');
ok(comp.feeds.some(f => f.id === 'efra_forensic'),
   'and efra_forensic, which also accepts evidence');
ok(composition(sneaky).fedBy.some(f => f.id === 'macro_forecaster'),
   'the reverse edge resolves: efra_forensic can be fed by macro_forecaster');
ok(!comp.feeds.some(f => f.id === 'macro_forecaster'), 'never composes with itself');

// ── Specimen sheet ────────────────────────────────────────────────
console.log('specimen sheet:');
renderSheet(macro);
let h = doc.sheet.innerHTML;
ok(h.includes('Analyticus macro_forecaster'), 'shows the binomial');
ok(h.includes('Investigatidae'), 'shows the taxonomic lineage');
ok(h.includes('foresight'), 'shows primary affect from valence');
ok(h.includes('analytical'), 'shows personality traits');
ok(h.includes('Domain knowledge'), 'renders the domain-knowledge panel');
ok(h.includes('yield_curve') || h.includes('base_rate') || h.includes('recession'),
   'surfaces actual domain-knowledge content');
ok(h.includes('sample queries') || h.includes('Sample'), 'renders sample queries');
ok(h.includes('Observatory'), 'cross-links to the clinical view');

// ── I/O contract and material properties ──────────────────────────
console.log('I/O contract:');
ok(h.includes('material interface'), 'renders the I/O contract panel');
ok(h.includes('labels, so composability with them is asserted, not verified'),
   'says plainly that untyped ports are asserted, not verified — a label match must \n           not read as a schema match');
ok(h.includes('Label match on produces'),
   'the feeds/fed-by panels caveat that they are label matches');

// A card WITH a typed contract must show the schema instead of the caveat.
const typed = {
  ...macro,
  capabilities: { ...macro.capabilities, output_contract: {
    domain: 'foraging_forecast', produces_schema: 'kask_wild/condition_forecast',
    calibration: { signal: 'forage_observation', comparison: 'predicted_vs_actual',
                   resolution_delay: '1-7 days' } } },
};
renderSheet(typed);
let t = doc.sheet.innerHTML;
ok(t.includes('Typed output contract'), 'shows a typed contract when one is declared');
ok(t.includes('kask_wild/condition_forecast'), 'shows the schema identifier');
ok(t.includes('forage_observation'), 'shows what calibrates the output');
ok(!t.includes('asserted, not verified'),
   'and drops the caveat, because this one IS verified by a schema');

console.log('instruments (material properties):');
const instrumented = {
  ...macro,
  capabilities: { ...macro.capabilities,
    mcp_tools: [{ name: 'adaptogen_species_search', description: 'Search species by name.' }] },
  requires_secrets: [{ name: 'FMP_API_KEY', label: 'Financial data' }],
};
renderSheet(instrumented);
t = doc.sheet.innerHTML;
ok(t.includes('Callable tools (1)'), 'counts and lists callable MCP tools');
ok(t.includes('adaptogen_species_search'), 'names the tool');
ok(t.includes('Search species by name.'), 'shows what the tool does');
ok(t.includes('FMP_API_KEY'), 'surfaces required credentials as a material dependency');
ok(t.includes('Substrate') && t.includes('claude'), 'shows model/provider substrate');

renderSheet({ agent_id: 'bare2', metadata: {}, execution_stats: {}, habitats: [], capabilities: {} });
t = doc.sheet.innerHTML;
ok(t.includes('works from its prompt alone'),
   'an agent with no instruments says so rather than showing an empty panel');

console.log('composition — declared pipeline:');
const piped = {
  ...macro,
  workflow_template: { description: '3-stage pipeline', stages: [
    { name: 'Intake', agent: 'ar_card_producer', accepts: ['logo'], produces: ['brief'] },
    { name: 'Review', agent: null, accepts: ['brief'], produces: ['verdict'],
      description: 'Needs a reviewer' },
  ]},
};
renderSheet(piped);
t = doc.sheet.innerHTML;
ok(t.includes('Internal pipeline'), 'renders declared pipeline stages');
ok(t.includes('open slot'),
   'an unfilled stage is called an open slot — the most concrete composition \n           affordance in the corpus');
ok(t.includes('stage open'), 'and is visually distinguished');
ok(t.includes('logo') && t.includes('brief'), 'shows each stage\'s own accepts/produces');

renderSheet(sneaky);
h = doc.sheet.innerHTML;
ok(h.includes('admitted without review'), 'flags an unreviewed member on its sheet');
ok(h.includes('fermi · curated_seed'), 'states provenance explicitly on the badge');
ok(h.includes('It is not membership'),
   'a fermi_contract is labelled a capability, never presented as membership');

renderSheet({ agent_id: 'bare', metadata: {}, execution_stats: {}, habitats: [] });
h = doc.sheet.innerHTML;
ok(h.includes('Undescribed'), 'a card with no taxonomy says so rather than rendering blanks');
ok(h.includes('xaman_ek only'), 'a non-member shows implicit habitat only');

// ── Field guide ───────────────────────────────────────────────────
console.log('field guide:');
page.OVERVIEW = {
  population: { published: 104, total_runs: 30, by_tier: { curated: 78, community: 11, system: 15 },
                by_niche: { research: 55 }, by_provider: { anthropic: 103 } },
  habitats: [{ name: 'fermi', kind: 'explicit', rule: 'admitted by review',
               population: 12, provenance: { curated_seed: 12 } }],
  governance: { pending_requests: 0, approvals_ever: 0,
                unreviewed_members: [{ agent_name: 'guidance_tracker' }] },
  cohabitation: { pairs: [], distinct_rosters: 8, total_workspaces: 199 },
};
page.SPEC = [macro, sneaky];
renderFieldGuide();
h = doc.sheet.innerHTML;
ok(h.includes('never been used'), 'zero approvals reads as "never used", not clean');
ok(!h.includes('gov-alert clean'), 'does NOT render the green all-clear');
ok(h.includes('guidance_tracker'), 'names the unreviewed member');
ok(h.includes('191 are exact template clones'), 'explains the collapsed template clones');
ok(h.includes('undescribed'), 'reports how many specimens lack a taxonomy');

// ── Escaping ──────────────────────────────────────────────────────
console.log('escaping:');
renderSheet({ agent_id: '<img src=x onerror=alert(1)>', description: '<script>bad()</script>',
              metadata: {}, execution_stats: {}, habitats: [] });
h = doc.sheet.innerHTML;
ok(!h.includes('<img'), 'escapes a hostile agent id');
ok(!h.includes('<script>bad'), 'escapes a hostile description');

console.log(fail === 0 ? '\nALL RENDER CHECKS PASSED' : `\n${fail} CHECK(S) FAILED`);
process.exit(fail === 0 ? 0 : 1);
