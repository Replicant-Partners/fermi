#!/usr/bin/env python3
"""
Batch-spawn World Cup 2026 team prior workspaces via the Fermi forecast app.

Usage:
    python3 spawn_team_priors.py [--api-url URL] [--api-key KEY]

Spawns 48 team prior workspaces (one per WC 2026 team), each parameterized
with Elo ratings, group assignment, and confederation. Uses the batch spawn
endpoint: POST /api/apps/fermi_forecast/workspaces/batch

After team priors are spawned, spawns 12 group path workspaces that depend
on their group's team priors.
"""

import json
import os
import sys
import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))

# ═══════════════════════════════════════════════════════════════════
# 2026 FIFA World Cup — 48 teams, 12 groups of 4
# Elo ratings from footballdatabase.com (approximate, June 2026)
# ═══════════════════════════════════════════════════════════════════

TEAMS = [
    # Group A (hosts: USA, Mexico, Canada share hosting)
    {"team_id": "USA", "team_name": "United States", "group": "A", "confederation": "CONCACAF", "is_host": True,  "elo": 1820},
    {"team_id": "CAN", "team_name": "Canada",        "group": "A", "confederation": "CONCACAF", "is_host": True,  "elo": 1680},
    {"team_id": "MEX", "team_name": "Mexico",         "group": "A", "confederation": "CONCACAF", "is_host": True,  "elo": 1760},
    {"team_id": "COL", "team_name": "Colombia",        "group": "A", "confederation": "CONMEBOL", "is_host": False, "elo": 1840},
    # Group B
    {"team_id": "ARG", "team_name": "Argentina",      "group": "B", "confederation": "CONMEBOL", "is_host": False, "elo": 2100},
    {"team_id": "PRY", "team_name": "Paraguay",       "group": "B", "confederation": "CONMEBOL", "is_host": False, "elo": 1620},
    {"team_id": "NZL", "team_name": "New Zealand",    "group": "B", "confederation": "OFC",      "is_host": False, "elo": 1480},
    {"team_id": "IRN", "team_name": "Iran",            "group": "B", "confederation": "AFC",      "is_host": False, "elo": 1560},
    # Group C
    {"team_id": "BRA", "team_name": "Brazil",          "group": "C", "confederation": "CONMEBOL", "is_host": False, "elo": 1980},
    {"team_id": "JPN", "team_name": "Japan",           "group": "C", "confederation": "AFC",      "is_host": False, "elo": 1780},
    {"team_id": "CIV", "team_name": "Côte d'Ivoire",  "group": "C", "confederation": "CAF",      "is_host": False, "elo": 1580},
    {"team_id": "TUN", "team_name": "Tunisia",         "group": "C", "confederation": "CAF",      "is_host": False, "elo": 1520},
    # Group D
    {"team_id": "FRA", "team_name": "France",          "group": "D", "confederation": "UEFA",     "is_host": False, "elo": 2050},
    {"team_id": "AUS", "team_name": "Australia",       "group": "D", "confederation": "AFC",      "is_host": False, "elo": 1620},
    {"team_id": "IDN", "team_name": "Indonesia",       "group": "D", "confederation": "AFC",      "is_host": False, "elo": 1280},
    {"team_id": "UAE", "team_name": "UAE",              "group": "D", "confederation": "AFC",      "is_host": False, "elo": 1380},
    # Group E
    {"team_id": "ESP", "team_name": "Spain",           "group": "E", "confederation": "UEFA",     "is_host": False, "elo": 2020},
    {"team_id": "NGA", "team_name": "Nigeria",         "group": "E", "confederation": "CAF",      "is_host": False, "elo": 1620},
    {"team_id": "BIH", "team_name": "Bosnia-Herzegovina","group": "E","confederation": "UEFA",    "is_host": False, "elo": 1540},
    {"team_id": "ALB", "team_name": "Albania",         "group": "E", "confederation": "UEFA",     "is_host": False, "elo": 1520},
    # Group F
    {"team_id": "ENG", "team_name": "England",         "group": "F", "confederation": "UEFA",     "is_host": False, "elo": 1970},
    {"team_id": "SEN", "team_name": "Senegal",         "group": "F", "confederation": "CAF",      "is_host": False, "elo": 1620},
    {"team_id": "POL", "team_name": "Poland",          "group": "F", "confederation": "UEFA",     "is_host": False, "elo": 1700},
    {"team_id": "PAN", "team_name": "Panama",          "group": "F", "confederation": "CONCACAF", "is_host": False, "elo": 1480},
    # Group G
    {"team_id": "GER", "team_name": "Germany",         "group": "G", "confederation": "UEFA",     "is_host": False, "elo": 1940},
    {"team_id": "URY", "team_name": "Uruguay",         "group": "G", "confederation": "CONMEBOL", "is_host": False, "elo": 1820},
    {"team_id": "KOR", "team_name": "South Korea",     "group": "G", "confederation": "AFC",      "is_host": False, "elo": 1700},
    {"team_id": "SVK", "team_name": "Slovakia",        "group": "G", "confederation": "UEFA",     "is_host": False, "elo": 1560},
    # Group H
    {"team_id": "POR", "team_name": "Portugal",        "group": "H", "confederation": "UEFA",     "is_host": False, "elo": 1990},
    {"team_id": "ECU", "team_name": "Ecuador",         "group": "H", "confederation": "CONMEBOL", "is_host": False, "elo": 1720},
    {"team_id": "SAU", "team_name": "Saudi Arabia",    "group": "H", "confederation": "AFC",      "is_host": False, "elo": 1500},
    {"team_id": "CHN", "team_name": "China",           "group": "H", "confederation": "AFC",      "is_host": False, "elo": 1320},
    # Group I
    {"team_id": "NED", "team_name": "Netherlands",     "group": "I", "confederation": "UEFA",     "is_host": False, "elo": 1920},
    {"team_id": "CHL", "team_name": "Chile",           "group": "I", "confederation": "CONMEBOL", "is_host": False, "elo": 1680},
    {"team_id": "CMR", "team_name": "Cameroon",        "group": "I", "confederation": "CAF",      "is_host": False, "elo": 1500},
    {"team_id": "BHR", "team_name": "Bahrain",         "group": "I", "confederation": "AFC",      "is_host": False, "elo": 1340},
    # Group J
    {"team_id": "ITA", "team_name": "Italy",           "group": "J", "confederation": "UEFA",     "is_host": False, "elo": 1900},
    {"team_id": "CRC", "team_name": "Costa Rica",      "group": "J", "confederation": "CONCACAF", "is_host": False, "elo": 1540},
    {"team_id": "SRB", "team_name": "Serbia",          "group": "J", "confederation": "UEFA",     "is_host": False, "elo": 1680},
    {"team_id": "MAR", "team_name": "Morocco",         "group": "J", "confederation": "CAF",      "is_host": False, "elo": 1760},
    # Group K
    {"team_id": "BEL", "team_name": "Belgium",         "group": "K", "confederation": "UEFA",     "is_host": False, "elo": 1860},
    {"team_id": "WAL", "team_name": "Wales",           "group": "K", "confederation": "UEFA",     "is_host": False, "elo": 1560},
    {"team_id": "BOL", "team_name": "Bolivia",         "group": "K", "confederation": "CONMEBOL", "is_host": False, "elo": 1400},
    {"team_id": "UZB", "team_name": "Uzbekistan",      "group": "K", "confederation": "AFC",      "is_host": False, "elo": 1460},
    # Group L
    {"team_id": "CRO", "team_name": "Croatia",         "group": "L", "confederation": "UEFA",     "is_host": False, "elo": 1880},
    {"team_id": "DEN", "team_name": "Denmark",         "group": "L", "confederation": "UEFA",     "is_host": False, "elo": 1800},
    {"team_id": "TTO", "team_name": "Trinidad & Tobago","group": "L","confederation": "CONCACAF", "is_host": False, "elo": 1340},
    {"team_id": "EGY", "team_name": "Egypt",           "group": "L", "confederation": "CAF",      "is_host": False, "elo": 1540},
]

def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }

def batch_spawn_team_priors():
    """Spawn 48 team prior workspaces via batch endpoint."""
    instances = []
    for team in TEAMS:
        instances.append({
            "name": f"Team Prior — {team['team_name']} ({team['team_id']})",
            "description": f"WC 2026 tournament win probability prior for {team['team_name']}. Group {team['group']}, {team['confederation']}.",
            "params": {
                "program_type": "TEAM_PRIOR",
                "team_id": team["team_id"],
                "team_name": team["team_name"],
                "group": team["group"],
                "confederation": team["confederation"],
                "is_host": team["is_host"],
                "elo_current": team["elo"],
                "elo_trend": 0,  # neutral at start
            },
        })

    print(f"Spawning {len(instances)} team prior workspaces...")
    resp = requests.post(
        f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
        headers=headers(),
        json={"instances": instances},
        timeout=120,
    )

    if resp.status_code != 201:
        print(f"ERROR {resp.status_code}: {resp.text[:500]}")
        sys.exit(1)

    data = resp.json()
    print(f"Spawned: {data['spawned']}, Errors: {data['errors']}")

    # Build team_id → workspace_id map
    ws_map = {}
    for ws in data["workspaces"]:
        idx = ws["index"]
        team_id = TEAMS[idx]["team_id"]
        ws_map[team_id] = str(ws["workspace_id"])
        print(f"  {team_id}: {ws['workspace_id']} ({ws['name']})")

    if data["failed"]:
        print(f"\nFailed ({len(data['failed'])}):")
        for err in data["failed"]:
            print(f"  [{err['index']}] {err['name']}: {err['error']}")

    return ws_map

def spawn_group_paths(ws_map):
    """Spawn 12 group path workspaces, each depending on its 4 team priors."""
    groups = {}
    for team in TEAMS:
        groups.setdefault(team["group"], []).append(team["team_id"])

    instances = []
    for group, team_ids in sorted(groups.items()):
        depends_on = [ws_map[tid] for tid in team_ids if tid in ws_map]
        instances.append({
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
        })

    print(f"\nSpawning {len(instances)} group path workspaces...")
    resp = requests.post(
        f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
        headers=headers(),
        json={"instances": instances},
        timeout=120,
    )

    if resp.status_code != 201:
        print(f"ERROR {resp.status_code}: {resp.text[:500]}")
        return {}

    data = resp.json()
    print(f"Spawned: {data['spawned']}, Errors: {data['errors']}")

    group_ws_map = {}
    for ws in data["workspaces"]:
        idx = ws["index"]
        group = sorted(groups.keys())[idx]
        group_ws_map[group] = str(ws["workspace_id"])
        print(f"  Group {group}: {ws['workspace_id']}")

    return group_ws_map


if __name__ == "__main__":
    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY environment variable")
        sys.exit(1)

    print(f"API: {API_URL}")
    print(f"App: fermi_forecast")
    print(f"Teams: {len(TEAMS)}")
    print()

    # Phase 1: Team priors
    ws_map = batch_spawn_team_priors()

    # Save workspace map for later use
    map_path = os.path.join(os.path.dirname(__file__), "workspace_map.json")
    with open(map_path, "w") as f:
        json.dump(ws_map, f, indent=2)
    print(f"\nWorkspace map saved to {map_path}")

    # Phase 2: Group paths
    group_ws_map = spawn_group_paths(ws_map)

    # Save combined map
    combined = {"team_priors": ws_map, "group_paths": group_ws_map}
    with open(map_path, "w") as f:
        json.dump(combined, f, indent=2)
    print(f"\nFull workspace map saved to {map_path}")
    print(f"\nTotal workspaces: {len(ws_map) + len(group_ws_map)}")
