#!/usr/bin/env python3
"""
WC 2026 — Aligned Respawn.

The original spawn_team_priors.py used speculative team-to-group assignments
that don't match the actual draw. This script:

  1. HARD-DELETES the 60 existing workspaces (48 team-priors + 12 group-paths)
     listed in workspace_map.json.
  2. Spawns 48 team-prior workspaces with the correct groups and Elo ratings
     derived from the real WC 2026 draw + Elo Sports Index.
  3. Spawns 12 group-path workspaces with corrected dependencies.
  4. Overwrites workspace_map.json.

Run order (operator):

    python3 respawn_aligned.py          # destructive; needs confirmation
    curl POST /api/apps/fermi_forecast/sync-auto-hire  # add agents
    python3 backfill_observations.py    # populate the 20 played matches

Env:
    FERMI_API_URL  (default: https://agent-bestiary.world)
    FERMI_API_KEY  (or ABW_API_KEY)
"""

import json
import os
import sys
import time

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
TIMEOUT = 60

# ═══════════════════════════════════════════════════════════════════
# 2026 FIFA World Cup — actual 48-team draw
# Elo Sports Index ratings, June 2026 snapshot.
# Group assignments cross-checked against published fixtures.
# ═══════════════════════════════════════════════════════════════════

TEAMS = [
    # Group A
    {"team_id": "MEX", "team_name": "Mexico",          "group": "A", "confederation": "CONCACAF", "is_host": True,  "elo": 1881},
    {"team_id": "ZAF", "team_name": "South Africa",    "group": "A", "confederation": "CAF",      "is_host": False, "elo": 1511},
    {"team_id": "KOR", "team_name": "South Korea",     "group": "A", "confederation": "AFC",      "is_host": False, "elo": 1786},
    {"team_id": "CZE", "team_name": "Czechia",         "group": "A", "confederation": "UEFA",     "is_host": False, "elo": 1712},
    # Group B
    {"team_id": "CAN", "team_name": "Canada",          "group": "B", "confederation": "CONCACAF", "is_host": True,  "elo": 1767},
    {"team_id": "BIH", "team_name": "Bosnia & Herzegovina", "group": "B", "confederation": "UEFA", "is_host": False, "elo": 1616},
    {"team_id": "QAT", "team_name": "Qatar",           "group": "B", "confederation": "AFC",      "is_host": False, "elo": 1447},
    {"team_id": "CHE", "team_name": "Switzerland",     "group": "B", "confederation": "UEFA",     "is_host": False, "elo": 1865},
    # Group C
    {"team_id": "BRA", "team_name": "Brazil",          "group": "C", "confederation": "CONMEBOL", "is_host": False, "elo": 1978},
    {"team_id": "MAR", "team_name": "Morocco",         "group": "C", "confederation": "CAF",      "is_host": False, "elo": 1760},
    {"team_id": "HAI", "team_name": "Haiti",           "group": "C", "confederation": "CONCACAF", "is_host": False, "elo": 1536},
    {"team_id": "SCO", "team_name": "Scotland",        "group": "C", "confederation": "UEFA",     "is_host": False, "elo": 1794},
    # Group D
    {"team_id": "USA", "team_name": "United States",   "group": "D", "confederation": "CONCACAF", "is_host": True,  "elo": 1780},
    {"team_id": "PRY", "team_name": "Paraguay",        "group": "D", "confederation": "CONMEBOL", "is_host": False, "elo": 1780},
    {"team_id": "AUS", "team_name": "Australia",       "group": "D", "confederation": "AFC",      "is_host": False, "elo": 1839},
    {"team_id": "TUR", "team_name": "Turkiye",         "group": "D", "confederation": "UEFA",     "is_host": False, "elo": 1849},
    # Group E
    {"team_id": "CIV", "team_name": "Côte d'Ivoire",   "group": "E", "confederation": "CAF",      "is_host": False, "elo": 1743},
    {"team_id": "ECU", "team_name": "Ecuador",         "group": "E", "confederation": "CONMEBOL", "is_host": False, "elo": 1890},
    {"team_id": "GER", "team_name": "Germany",         "group": "E", "confederation": "UEFA",     "is_host": False, "elo": 1939},
    {"team_id": "CUW", "team_name": "Curaçao",         "group": "E", "confederation": "CONCACAF", "is_host": False, "elo": 1427},
    # Group F
    {"team_id": "NED", "team_name": "Netherlands",     "group": "F", "confederation": "UEFA",     "is_host": False, "elo": 1944},
    {"team_id": "JPN", "team_name": "Japan",           "group": "F", "confederation": "AFC",      "is_host": False, "elo": 1910},
    {"team_id": "SWE", "team_name": "Sweden",          "group": "F", "confederation": "UEFA",     "is_host": False, "elo": 1755},
    {"team_id": "TUN", "team_name": "Tunisia",         "group": "F", "confederation": "CAF",      "is_host": False, "elo": 1585},
    # Group G
    {"team_id": "IRN", "team_name": "Iran",            "group": "G", "confederation": "AFC",      "is_host": False, "elo": 1756},
    {"team_id": "NZL", "team_name": "New Zealand",     "group": "G", "confederation": "OFC",      "is_host": False, "elo": 1578},
    {"team_id": "BEL", "team_name": "Belgium",         "group": "G", "confederation": "UEFA",     "is_host": False, "elo": 1879},
    {"team_id": "EGY", "team_name": "Egypt",           "group": "G", "confederation": "CAF",      "is_host": False, "elo": 1711},
    # Group H
    {"team_id": "SAU", "team_name": "Saudi Arabia",    "group": "H", "confederation": "AFC",      "is_host": False, "elo": 1598},
    {"team_id": "URY", "team_name": "Uruguay",         "group": "H", "confederation": "CONMEBOL", "is_host": False, "elo": 1870},
    {"team_id": "ESP", "team_name": "Spain",           "group": "H", "confederation": "UEFA",     "is_host": False, "elo": 2129},
    {"team_id": "CPV", "team_name": "Cape Verde",      "group": "H", "confederation": "CAF",      "is_host": False, "elo": 1606},
    # Group I
    {"team_id": "FRA", "team_name": "France",          "group": "I", "confederation": "UEFA",     "is_host": False, "elo": 2084},
    {"team_id": "SEN", "team_name": "Senegal",         "group": "I", "confederation": "CAF",      "is_host": False, "elo": 1839},
    {"team_id": "IRQ", "team_name": "Iraq",            "group": "I", "confederation": "AFC",      "is_host": False, "elo": 1607},
    {"team_id": "NOR", "team_name": "Norway",          "group": "I", "confederation": "UEFA",     "is_host": False, "elo": 1914},
    # Group J
    {"team_id": "ARG", "team_name": "Argentina",       "group": "J", "confederation": "CONMEBOL", "is_host": False, "elo": 2115},
    {"team_id": "DZA", "team_name": "Algeria",         "group": "J", "confederation": "CAF",      "is_host": False, "elo": 1772},
    {"team_id": "AUT", "team_name": "Austria",         "group": "J", "confederation": "UEFA",     "is_host": False, "elo": 1830},
    {"team_id": "JOR", "team_name": "Jordan",          "group": "J", "confederation": "AFC",      "is_host": False, "elo": 1680},
    # Group K
    {"team_id": "COL", "team_name": "Colombia",        "group": "K", "confederation": "CONMEBOL", "is_host": False, "elo": 1982},
    {"team_id": "JAM", "team_name": "Jamaica",         "group": "K", "confederation": "CONCACAF", "is_host": False, "elo": 1527},
    {"team_id": "POR", "team_name": "Portugal",        "group": "K", "confederation": "UEFA",     "is_host": False, "elo": 1989},
    {"team_id": "UZB", "team_name": "Uzbekistan",      "group": "K", "confederation": "AFC",      "is_host": False, "elo": 1714},
    # Group L
    {"team_id": "CRO", "team_name": "Croatia",         "group": "L", "confederation": "UEFA",     "is_host": False, "elo": 1912},
    {"team_id": "ENG", "team_name": "England",         "group": "L", "confederation": "UEFA",     "is_host": False, "elo": 2024},
    {"team_id": "GHA", "team_name": "Ghana",           "group": "L", "confederation": "CAF",      "is_host": False, "elo": 1510},
    {"team_id": "PAN", "team_name": "Panama",          "group": "L", "confederation": "CONCACAF", "is_host": False, "elo": 1730},
]


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def confirm(prompt):
    answer = input(f"{prompt} [type 'yes' to proceed]: ").strip().lower()
    return answer == "yes"


def wipe_via_admin(dry_run: bool):
    """Call POST /api/admin/wipe-fermi-forecasts to nuke all
    fermi_forecast workspaces system-wide. See src/handlers/admin.rs.

    The platform has no DELETE /api/workspaces/:id endpoint by design;
    forecasts are durable artifacts. The admin wipe endpoint is the
    single sanctioned way to bulk-clear them.
    """
    body = {"confirm": "WIPE_ALL_FERMI_FORECASTS", "dry_run": dry_run}
    print(f"  POST /api/admin/wipe-fermi-forecasts (dry_run={dry_run})…")
    try:
        resp = requests.post(
            f"{API_URL}/api/admin/wipe-fermi-forecasts",
            headers=headers(),
            json=body,
            timeout=TIMEOUT * 3,
        )
    except requests.exceptions.RequestException as e:
        print(f"  ERROR: request failed: {e}")
        return None

    if resp.status_code != 200:
        print(f"  ERROR: HTTP {resp.status_code}: {resp.text[:400]}")
        return None

    data = resp.json()
    print(f"  workspaces:    {data.get('workspaces')}")
    print(f"  forecasts:     {data.get('forecasts')}")
    if dry_run:
        wd = data.get("would_delete", {})
    else:
        wd = data.get("deleted", {})
        print(f"  repos_deleted: {data.get('repos_deleted')}")
        repo_failures = data.get("repo_failures", [])
        if repo_failures:
            print(f"  repo_failures: {len(repo_failures)}")
            for f in repo_failures[:5]:
                print(f"    {f.get('slug')}: {f.get('error')}")
    print("  table counts:")
    for tbl, cnt in sorted(wd.items()):
        print(f"    {tbl:35s} {cnt}")
    return data


# Chunk size for batch spawn calls.
#
# Each instance triggers serial server-side work (git init, params
# commit, dependency wiring, auto-hire across ~12 agents) totalling
# ~3-5s per workspace. 48 in one batch = ~4 minutes server-side =
# client timeout. 8 per chunk = ~40s, comfortably inside the request
# timeout we set on `requests.post` below.
CHUNK_SIZE = 8


def batch_spawn_team_priors():
    """Spawn 48 team-prior workspaces in chunked batches."""
    instances = []
    for team in TEAMS:
        instances.append({
            "name": f"Team Prior — {team['team_name']} ({team['team_id']})",
            "description": (
                f"Tournament win probability prior for {team['team_name']}. "
                f"Group {team['group']}, {team['confederation']}, Elo {team['elo']}."
            ),
            "params": {
                "program_type": "TEAM_PRIOR",
                "team_id": team["team_id"],
                "team_name": team["team_name"],
                "group": team["group"],
                "confederation": team["confederation"],
                "is_host": team["is_host"],
                "elo_current": team["elo"],
                "elo_trend": 0.0,
                # Socio metrics are placeholders; macro_data_agent fills these
                # in once it runs against the workspace.
                "gdp_per_capita_log": 0.0,
                "population_log": 0.0,
                "hdi_logit": 0.0,
            },
        })

    ws_map = {}
    failed_total = []

    n_chunks = (len(instances) + CHUNK_SIZE - 1) // CHUNK_SIZE
    print(f"  Batch spawning {len(instances)} team-priors in {n_chunks} chunks of {CHUNK_SIZE}…")

    for chunk_idx in range(n_chunks):
        start = chunk_idx * CHUNK_SIZE
        chunk = instances[start:start + CHUNK_SIZE]
        names = [it["name"].split("—")[-1].strip() for it in chunk]
        print(f"  [chunk {chunk_idx + 1}/{n_chunks}] {len(chunk)} workspaces: "
              f"{', '.join(names[:3])}{'…' if len(names) > 3 else ''}", flush=True)

        try:
            resp = requests.post(
                f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
                headers=headers(),
                json={"instances": chunk},
                timeout=TIMEOUT * 3,  # 180s per chunk — comfortable margin
            )
        except requests.exceptions.RequestException as e:
            print(f"    FAIL: request error: {e}")
            continue

        if resp.status_code != 201:
            print(f"    FAIL: HTTP {resp.status_code}: {resp.text[:240]}")
            continue

        data = resp.json()
        chunk_spawned = 0
        for ws in data.get("workspaces", []):
            params = ws.get("params") or ws.get("provisioned", {}).get("params") or {}
            team_id = params.get("team_id")
            if not team_id:
                name = ws.get("name", "")
                if "(" in name and name.endswith(")"):
                    team_id = name.rsplit("(", 1)[-1].rstrip(")")
            if team_id:
                ws_map[team_id] = str(ws["workspace_id"])
                chunk_spawned += 1
        failed_total.extend(data.get("failed", []))
        print(f"    OK: {chunk_spawned}/{len(chunk)} spawned")
        time.sleep(0.5)  # be polite between chunks

    print(f"  Total spawned: {len(ws_map)} / {len(instances)}")
    if failed_total:
        print(f"  Failures: {len(failed_total)}")
        for err in failed_total[:10]:
            print(f"    {err}")
    return ws_map


def load_team_prior_template():
    """Read the FPL template once, return as a string for per-team substitution."""
    tmpl_path = os.path.join(
        os.path.dirname(__file__), "..", "..", "templates", "world_cup", "team_prior.fpl"
    )
    if not os.path.exists(tmpl_path):
        print(f"WARNING: team_prior.fpl template not found at {tmpl_path}")
        return None
    with open(tmpl_path) as f:
        return f.read()


def render_fpl_for_team(template: str, team: dict) -> str:
    """Render the FPL template for one team by substituting `{team_name}`
    in the `question` line. Param values are passed separately to the
    executor via params.json; we don't substitute them into the FPL text
    itself — the parser reads `param` declarations and the executor binds
    runtime values from the params file.
    """
    return template.replace("{team_name}", team["team_name"])


def create_forecasts_per_team(ws_map: dict, template: str | None) -> dict:
    """For each team-prior workspace, POST /api/forecasts with the per-team
    FPL and workspace_id link. Returns {team_id: forecast_id}.

    This is what makes the BayesOps refit hook, forecast spacetime, and
    Polymarket linkage all work: they all join through
    fermi_forecasts.workspace_id.
    """
    if not template:
        print("  No FPL template available; skipping forecast creation.")
        return {}

    forecast_map: dict[str, str] = {}
    print(f"  Creating fermi_forecasts rows for {len(ws_map)} team-prior workspaces…")

    for team in TEAMS:
        tid = team["team_id"]
        ws_id = ws_map.get(tid)
        if not ws_id:
            continue

        fpl_source = render_fpl_for_team(template, team)
        body = {
            "question_text": f"Will {team['team_name']} win the 2026 FIFA World Cup?",
            "predicted_probability": 0.02,  # cold-start prior; 1/48 ≈ 0.021
            "domain": "sports",
            "fpl_source": fpl_source,
            "status": "active",
            "visibility": "shared",
            "tags": [
                "wc2026",
                f"group-{team['group'].lower()}",
                team["confederation"].lower(),
            ],
            "workspace_id": ws_id,
        }

        try:
            resp = requests.post(
                f"{API_URL}/api/forecasts",
                headers=headers(),
                json=body,
                timeout=TIMEOUT,
            )
        except requests.exceptions.RequestException as e:
            print(f"    {tid}: FAIL (request error: {e})")
            continue

        if resp.status_code not in (200, 201):
            print(f"    {tid}: FAIL (HTTP {resp.status_code}: {resp.text[:200]})")
            continue

        data = resp.json()
        fid = data.get("forecast_id") or data.get("id")
        if fid:
            forecast_map[tid] = fid
        else:
            print(f"    {tid}: FAIL (no forecast id in response: {data})")
        time.sleep(0.05)

    print(f"  Forecasts created: {len(forecast_map)} / {len(ws_map)}")
    return forecast_map


def spawn_group_paths(ws_map):
    """Spawn 12 group_path workspaces with dependencies wired."""
    # Build group → team_ids index
    by_group = {}
    for team in TEAMS:
        by_group.setdefault(team["group"], []).append(team["team_id"])

    group_map = {}
    for group, team_ids in sorted(by_group.items()):
        depends_on = [ws_map[tid] for tid in team_ids if tid in ws_map]
        if len(depends_on) != 4:
            print(f"  Group {group}: only {len(depends_on)} dependencies wired (expected 4) — skipping")
            continue

        instance = {
            "name": f"Tournament Path — Group {group}",
            "description": f"Group {group} bracket simulation: {', '.join(team_ids)}",
            "params": {
                "program_type": "TOURNAMENT_PATH",
                "stage_id": f"GROUP_{group}",
                "team_ids": team_ids,
                "team_workspace_ids": {tid: ws_map.get(tid) for tid in team_ids},
                "n_simulations": 10000,
            },
            "depends_on": depends_on,
        }

        print(f"  Group {group} ({', '.join(team_ids)}) …", end=" ", flush=True)
        try:
            resp = requests.post(
                f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
                headers=headers(),
                json={"instances": [instance]},
                timeout=TIMEOUT,
            )
        except requests.exceptions.RequestException as e:
            print(f"FAIL ({e})")
            continue

        if resp.status_code != 201:
            print(f"FAIL ({resp.status_code}: {resp.text[:120]})")
            continue

        ws_id = resp.json().get("workspaces", [{}])[0].get("workspace_id")
        if ws_id:
            group_map[group] = str(ws_id)
            print(f"OK → {ws_id}")
        else:
            print("FAIL (no workspace_id in response)")
        time.sleep(0.05)

    return group_map


def main():
    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY")
        sys.exit(1)

    print(f"API: {API_URL}")
    print(f"Teams: {len(TEAMS)} across 12 groups")
    print()

    # Sanity check: 4 teams per group
    by_group = {}
    for t in TEAMS:
        by_group.setdefault(t["group"], []).append(t["team_id"])
    for g, ts in sorted(by_group.items()):
        if len(ts) != 4:
            print(f"ERROR: Group {g} has {len(ts)} teams (expected 4): {ts}")
            sys.exit(1)
    print(f"Sanity check: 12 groups × 4 teams = {len(TEAMS)} ✓")
    print()

    map_path = os.path.join(os.path.dirname(__file__), "workspace_map.json")

    # ── 1. Wipe via admin endpoint ────────────────────────────────
    #
    # Dry-run first to show the operator exactly what will be deleted,
    # require confirmation, then run the actual wipe. Idempotent on
    # repeat runs (if everything's already gone, counts are zero).
    print("─── Phase 1: wipe existing fermi_forecast state ───")
    print("  Step 1a: dry-run")
    dry = wipe_via_admin(dry_run=True)
    if dry is None:
        print("Aborted: dry-run failed.")
        sys.exit(1)
    print()
    if dry.get("workspaces", 0) == 0 and dry.get("forecasts", 0) == 0:
        print("  Nothing to wipe — skipping live wipe.")
    else:
        if not confirm(f"  Step 1b: live wipe. Destructive. Continue?"):
            print("Aborted.")
            sys.exit(0)
        live = wipe_via_admin(dry_run=False)
        if live is None:
            print("Aborted: live wipe failed.")
            sys.exit(1)
    print()

    # ── 2. Spawn team priors ──────────────────────────────────────
    print("─── Phase 2: spawn team-prior workspaces ───")
    ws_map = batch_spawn_team_priors()
    if len(ws_map) != len(TEAMS):
        print(f"WARNING: spawned {len(ws_map)}/{len(TEAMS)} team-priors. Stopping; review and rerun.")
        with open(map_path, "w") as f:
            json.dump(
                {"team_priors": ws_map, "group_paths": {}, "forecasts": {}},
                f, indent=2,
            )
        sys.exit(1)
    print()

    # ── 3. Create per-team fermi_forecasts rows ───────────────────
    print("─── Phase 3: create forecasts linked to team-prior workspaces ───")
    template = load_team_prior_template()
    forecast_map = create_forecasts_per_team(ws_map, template)
    print()

    # ── 4. Spawn group paths ──────────────────────────────────────
    print("─── Phase 4: spawn group-path workspaces ───")
    group_map = spawn_group_paths(ws_map)
    print()

    # ── 5. Persist map ────────────────────────────────────────────
    combined = {
        "team_priors": ws_map,
        "group_paths": group_map,
        "forecasts": forecast_map,
    }
    with open(map_path, "w") as f:
        json.dump(combined, f, indent=2)
    print(f"Workspace map written: {map_path}")
    print(f"  team_priors:  {len(ws_map)}")
    print(f"  forecasts:    {len(forecast_map)}")
    print(f"  group_paths:  {len(group_map)}")
    print(f"  total ws:     {len(ws_map) + len(group_map)}")
    print()
    print("Next steps:")
    print(f"  curl -X POST '{API_URL}/api/apps/fermi_forecast/sync-auto-hire' \\\\")
    print(f"    -H 'Authorization: Bearer $FERMI_API_KEY'")
    print(f"  python3 backfill_observations.py")


if __name__ == "__main__":
    main()
