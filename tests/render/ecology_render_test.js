// Render checks for templates/ecology.html.
//
// Loads the page's render functions and exercises them against a payload
// shaped like the real /api/ecology/overview response — no browser, no
// database. The point is that this view's value is entirely in *what it
// makes obvious*, so the assertions are about meaning, not markup:
// unreviewed third-party members must be flagged, platform-seeded ones
// must not be, and "zero approvals" must never render as the green
// all-clear state.
//
// Run:  node tests/render/ecology_render_test.js
const fs = require('fs');
const path = require('path');
const html = fs.readFileSync(
  path.join(__dirname, '..', '..', 'templates', 'ecology.html'), 'utf8');
const js = html.match(/<script>([\s\S]*)<\/script>/)[1]
  .replace(/\(async function load\(\)[\s\S]*$/, '');   // drop the fetch bootstrap

const doc = {};
global.document = { getElementById: id => (doc[id] = doc[id] || { innerHTML: '' }) };
eval(js);

// Payload shaped exactly like the verified production queries.
const P = {
  population: { published: 104, total_runs: 30,
    by_tier: { curated: 78, community: 11, system: 15 },
    by_niche: { research: 55, creative: 20, meta: 9 },
    by_provider: { anthropic: 103, deepseek: 1 } },
  habitats: [
    { name: 'fermi', kind: 'explicit', rule: 'admitted by review', population: 21,
      provenance: { curated_seed: 21 },
      members: [
        { agent_name: 'biotech_analyst', tier: 'curated',   agent_type: 'research', runs: 0, membership_source: 'curated_seed' },
        { agent_name: 'efra_forensic',   tier: 'community', agent_type: 'research', runs: 0, membership_source: 'curated_seed' },
      ] },
    { name: 'xaman_ek', kind: 'implicit', rule: 'publishing is joining', population: 104,
      provenance: { implicit: 104 }, members: [] },
  ],
  governance: { pending_requests: 0, approvals_ever: 0,
    unreviewed_members: [{ agent_name: 'efra_forensic', tier: 'community', owner: '010e541a-x', membership_source: 'curated_seed' }] },
  cohabitation: { pairs: [{ a: 'dyad_observer', b: 'anomaly_triager', distinct_teams: 2 }],
                  distinct_rosters: 8, total_workspaces: 199 },
};

const unrev = new Set(P.governance.unreviewed_members.map(m => m.agent_name));
census(P.population); governance(P.governance);
habitats(P.habitats, unrev); cohabitation(P.cohabitation);

let fail = 0;
const ok = (cond, label) => { console.log((cond ? '  PASS  ' : '  FAIL  ') + label); if (!cond) fail++; };

console.log('census:');
ok(doc.census.innerHTML.includes('104'), 'shows published population');
ok(doc.census.innerHTML.includes('curated 78'), 'shows tier breakdown');

console.log('governance:');
ok(doc.gov.innerHTML.includes('never been used'),
   'zero approvals reads as "never used", not as "clean"');
ok(!doc.gov.innerHTML.includes('gov-alert clean'), 'does NOT render the clean/green state');
ok(doc.gov.innerHTML.includes('efra_forensic'), 'names the unreviewed member');

console.log('habitats:');
ok(doc.habitats.innerHTML.includes('cell curated_seed flag'),
   'unreviewed third-party member is visually flagged');
ok(doc.habitats.innerHTML.includes('>biotech_analyst<') ||
   doc.habitats.innerHTML.includes('biotech_analyst'), 'renders platform member');
ok(!/class="cell curated_seed flag"[^>]*>biotech_analyst/.test(doc.habitats.innerHTML),
   'platform-seeded member is NOT flagged (only third-party)');
ok(doc.habitats.innerHTML.includes('p-implicit'), 'implicit habitat renders its own provenance');

console.log('cohabitation:');
ok(doc.cohab.innerHTML.includes('191 are exact template clones'),
   'states how many workspaces are template clones (199-8)');
ok(doc.cohab.innerHTML.includes('2 team(s)'), 'counts distinct teams, not workspace instances');

console.log('escaping:');
doc.gov.innerHTML = '';
governance({ pending_requests: 0, approvals_ever: 1,
  unreviewed_members: [{ agent_name: '<img src=x onerror=alert(1)>', tier: 'community', membership_source: 'x' }] });
ok(!doc.gov.innerHTML.includes('<img'), 'escapes hostile agent names');

console.log(fail === 0 ? '\nALL RENDER CHECKS PASSED' : `\n${fail} CHECK(S) FAILED`);
process.exit(fail === 0 ? 0 : 1);
