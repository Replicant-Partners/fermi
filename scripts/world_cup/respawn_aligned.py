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


def delete_workspace(ws_id):
    """Hard delete via DELETE /api/workspaces/:id."""
    try:
        resp = requests.delete(
            f"{API_URL}/api/workspaces/{ws_id}",
            headers=headers(),
            timeout=TIMEOUT,
        )
        if resp.status_code in (200, 204, 404):
            return True, None
        return False, f"HTTP {resp.status_code}: {resp.text[:160]}"
    except requests.exceptions.RequestException as e:
        return False, str(e)


def hard_delete_existing(map_path):
    """Read workspace_map.json and DELETE every UUID listed."""
    if not os.path.exists(map_path):
        print(f"  No existing workspace_map.json at {map_path}; nothing to delete.")
        return

    with open(map_path) as f:
        old = json.load(f)

    targets = []
    for team_id, ws_id in old.get("team_priors", {}).items():
        targets.append(("team_prior", team_id, ws_id))
    for group, ws_id in old.get("group_paths", {}).items():
        targets.append(("group_path", group, ws_id))

    print(f"  Hard deleting {len(targets)} existing workspaces…")
    failures = []
    for kind, key, ws_id in targets:
        ok, err = delete_workspace(ws_id)
        marker = "OK" if ok else f"FAIL ({err})"
        print(f"    [{kind:11s}] {key:5s} {ws_id} … {marker}")
        if not ok:
            failures.append((kind, key, ws_id, err))
        time.sleep(0.05)
    print(f"  Deleted: {len(targets) - len(failures)}; Failed: {len(failures)}")
    return failures


def batch_spawn_team_priors():
    """Spawn 48 team-prior workspaces via batch endpoint."""
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

    print(f"  Batch spawning {len(instances)} team-prior workspaces…")
    resp = requests.post(
        f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
        headers=headers(),
        json={"instances": instances},
        timeout=TIMEOUT * 2,
    )
    if resp.status_code != 201:
        print(f"  ERROR {resp.status_code}: {resp.text[:300]}")
        return {}

    data = resp.json()
    ws_map = {}
    for ws in data.get("workspaces", []):
        params = ws.get("params") or ws.get("provisioned", {}).get("params") or {}
        # Some endpoints echo params in the response; others don't. Fall back
        # to matching on slug or workspace_name.
        team_id = params.get("team_id")
        if not team_id:
            name = ws.get("name", "")
            # "Team Prior — Argentina (ARG)" → "ARG"
            if "(" in name and name.endswith(")"):
                team_id = name.rsplit("(", 1)[-1].rstrip(")")
        if team_id:
            ws_map[team_id] = str(ws["workspace_id"])
    print(f"  Spawned: {len(ws_map)}")
    failed = data.get("failed", [])
    if failed:
        print(f"  Failures: {len(failed)}")
        for err in failed:
            print(f"    {err}")
    return ws_map


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

    # ── 1. Confirm + delete ──────────────────────────────────────
    if os.path.exists(map_path):
        with open(map_path) as f:
            old = json.load(f)
        n_old = len(old.get("team_priors", {})) + len(old.get("group_paths", {}))
        print(f"Existing workspaces to HARD DELETE: {n_old}")
        if not confirm("This is destructive. Continue?"):
            print("Aborted.")
            sys.exit(0)
        print()
        print("─── Phase 1: hard delete ───")
        hard_delete_existing(map_path)
        print()

    # ── 2. Spawn team priors ──────────────────────────────────────
    print("─── Phase 2: spawn team-prior workspaces ───")
    ws_map = batch_spawn_team_priors()
    if len(ws_map) != len(TEAMS):
        print(f"WARNING: spawned {len(ws_map)}/{len(TEAMS)} team-priors. Stopping; review and rerun.")
        with open(map_path, "w") as f:
            json.dump({"team_priors": ws_map, "group_paths": {}}, f, indent=2)
        sys.exit(1)
    print()

    # ── 3. Spawn group paths ──────────────────────────────────────
    print("─── Phase 3: spawn group-path workspaces ───")
    group_map = spawn_group_paths(ws_map)
    print()

    # ── 4. Persist map ────────────────────────────────────────────
    combined = {"team_priors": ws_map, "group_paths": group_map}
    with open(map_path, "w") as f:
        json.dump(combined, f, indent=2)
    print(f"Workspace map written: {map_path}")
    print(f"  team_priors:  {len(ws_map)}")
    print(f"  group_paths:  {len(group_map)}")
    print(f"  total:        {len(ws_map) + len(group_map)}")
    print()
    print("Next steps:")
    print(f"  curl -X POST '{API_URL}/api/apps/fermi_forecast/sync-auto-hire' \\\\")
    print(f"    -H 'Authorization: Bearer $FERMI_API_KEY'")
    print(f"  python3 backfill_observations.py")


if __name__ == "__main__":
    main()
