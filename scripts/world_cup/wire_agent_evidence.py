#!/usr/bin/env python3
"""
Wire agent evidence → workspace params for WC team priors.

Each agent's evidence summary in state.json contains structured
[MULTIPLIER] blocks with (p5, p50, p95) recommendations. This script
parses those and PUTs them to /api/workspaces/:id/outputs/params.

After writing params, it triggers a refit via POST /api/workspaces/:id/refit
so the FPL re-evaluates with the new driver distributions.

Mapping from agent driver_refs to param keys:

  macro_data_agent:          socio_capital  → socio_p5/socio_p50/socio_p95
  football_institution_agent: institutional_capacity → institutional_p5/...
  football_analyst:           dynamic_performance → dynamic_p5/...
                               squad_quality       → squad_p5/...
                               tactical_efficiency → tactical_p5/...
  fixture_context_agent:      fixture_context → fixture_p5/...

Run:
    python3 scripts/world_cup/wire_agent_evidence.py [--dry-run] [--team ARG]
"""

import argparse
import json
import os
import re
import sys
import time
from pathlib import Path

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
TIMEOUT = 30

WORKSPACE_MAP = Path(__file__).parent / "workspace_map.json"
FORECASTS_DIR = Path(__file__).resolve().parents[2] / "forecasts"

# ─── Agent → driver prefix mapping ──────────────────────────────────────
# Each agent's evidence produces a [MULTIPLIER] block for specific drivers.
# The key is the agent name prefix in the evidence id field.
AGENT_DRIVER_MAP = {
    "macro_data_agent": ["socio"],
    "football_institution_agent": ["institutional"],
    "football_analyst": ["dynamic", "squad", "tactical"],
    "fixture_context_agent": ["fixture"],
}

# Regex to parse agent evidence for multiplier recommendations.
# Matches: Suggested p50: 1.15 (p5: 1.05, p95: 1.28)
MULTIPLIER_RE = re.compile(
    r"Suggested\s+p50:\s+([\d.]+)\s*\(p5:\s+([\d.]+),\s*p95:\s+([\d.]+)\)"
)


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def load_forecast_state(team_id: str) -> dict | None:
    """Load the state.json for a team forecast.

    The filename convention uses the team name as it appears in the
    question text, lowercased with underscores. We scan the forecasts
    dir for filenames containing the team_id or team name.
    """
    # Try direct match by scanning
    team_map = {
        "ARG": "argentina",
        "BRA": "brazil",
        "ESP": "spain",
        "FRA": "france",
        "ENG": "england",
        "GER": "germany",
        "NED": "netherlands",
        "POR": "portugal",
        "BEL": "belgium",
        "CRO": "croatia",
        "ITA": "italy",
        "SUI": "switzerland",
        "USA": "united_states",
        "MEX": "mexico",
        "CAN": "canada",
        "URU": "uruguay",
        "COL": "colombia",
        "ECU": "ecuador",
        "JPN": "japan",
        "KOR": "south_korea",
        "AUS": "australia",
        "MAR": "morocco",
        "EGY": "egypt",
        "SEN": "senegal",
        "TUR": "turkiye",
        "SWE": "sweden",
        "NOR": "norway",
        "CZE": "czechia",
        "SCO": "scotland",
        "POL": "poland",
        "UKR": "ukraine",
        "DEN": "denmark",
        "AUT": "austria",
        "IRN": "iran",
        "IRQ": "iraq",
        "SAU": "saudi_arabia",
        "QAT": "qatar",
        "JOR": "jordan",
        "UZB": "uzbekistan",
        "NZL": "new_zealand",
        "PAN": "panama",
        "HAI": "haiti",
        "CIV": "cote_d_ivoire",
        "CMR": "cameroon",
        "GHA": "ghana",
        "TUN": "tunisia",
        "DZA": "algeria",
        "BIH": "bosnia",
        "PRY": "paraguay",
        "CUW": "curaçao",
        "CPV": "cape_verde",
    }
    team_name = team_map.get(team_id, team_id.lower())
    pattern = f"will_{team_name}_win"
    for fpath in FORECASTS_DIR.glob("*.state.json"):
        if pattern in fpath.name.replace(" ", "_").lower():
            with open(fpath) as f:
                return json.load(f)
    return None


def extract_multipliers(state: dict) -> dict:
    """Scan every evidence item in the forecast state and extract
    [MULTIPLIER] Suggested p50/p5/p95 values.

    Returns: { driver_prefix: (p5, p50, p95) }
    """
    multipliers: dict[str, tuple[float, float, float]] = {}
    evidence_list = state.get("evidence", [])

    for item in evidence_list:
        ev_id: str = item.get("id", "")
        summary: str = item.get("summary", "")

        # Determine which agent produced this evidence.
        agent_name = None
        for known_agent in AGENT_DRIVER_MAP:
            if ev_id.startswith(known_agent) or ev_id.startswith(
                known_agent.replace("_", "")
            ):
                agent_name = known_agent
                break
        if not agent_name and "Agent:" in item.get("source", ""):
            # Try from source string
            src = item.get("source", "")
            for known_agent in AGENT_DRIVER_MAP:
                if known_agent in src:
                    agent_name = known_agent
                    break
        if not agent_name:
            continue

        match = MULTIPLIER_RE.search(summary)
        if not match:
            continue

        p50 = float(match.group(1))
        p5 = float(match.group(2))
        p95 = float(match.group(3))

        # The multiplier applies to every driver this agent covers.
        driver_prefixes = AGENT_DRIVER_MAP.get(agent_name, [])
        for prefix in driver_prefixes:
            if prefix not in multipliers:
                multipliers[prefix] = (p5, p50, p95)

    return multipliers


def get_existing_params(workspace_id: str) -> dict:
    """Pull current params output from the workspace."""
    try:
        r = requests.get(
            f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
            headers=headers(),
            timeout=TIMEOUT,
        )
    except requests.RequestException:
        return {}
    if r.status_code == 404:
        return {}
    if r.status_code != 200:
        return {}
    body = r.json()
    if isinstance(body, dict) and "value" in body:
        return body["value"] if isinstance(body["value"], dict) else {}
    return {}


def put_params(workspace_id: str, merged: dict) -> tuple[bool, str]:
    """Write merged params to the workspace."""
    r = requests.put(
        f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
        headers=headers(),
        json={"value": merged},
        timeout=TIMEOUT,
    )
    if r.status_code in (200, 201):
        return True, ""
    return False, f"HTTP {r.status_code}: {r.text[:200]}"


def trigger_refit(workspace_id: str) -> tuple[bool, str]:
    """POST /refit to re-evaluate the FPL with updated params."""
    try:
        r = requests.post(
            f"{API_URL}/api/workspaces/{workspace_id}/refit",
            headers=headers(),
            json={},
            timeout=TIMEOUT,
        )
    except requests.RequestException as e:
        return False, str(e)
    if r.status_code in (200, 201, 202):
        return True, ""
    return False, f"HTTP {r.status_code}: {r.text[:200]}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be written without making changes",
    )
    parser.add_argument(
        "--team", type=str, default=None, help="Restrict to one team_id (e.g. ESP)"
    )
    parser.add_argument(
        "--no-refit",
        action="store_true",
        help="Skip the POST /refit after writing params",
    )
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY", file=sys.stderr)
        sys.exit(1)

    if not WORKSPACE_MAP.exists():
        print(f"ERROR: {WORKSPACE_MAP} not found", file=sys.stderr)
        sys.exit(1)

    with open(WORKSPACE_MAP) as f:
        ws_map = json.load(f)
    team_priors = ws_map.get("team_priors", {})

    teams = sorted(team_priors.keys()) if not args.team else [args.team]

    print(f"API: {API_URL}")
    print(f"Teams: {len(teams)}{'  (DRY RUN)' if args.dry_run else ''}")
    print()

    ok = 0
    fail = 0
    skipped = 0
    refit_ok = 0
    refit_fail = 0

    for team in teams:
        if team not in team_priors:
            print(f"  {team}: not in workspace_map; skipping")
            skipped += 1
            continue

        ws_id = team_priors[team]

        # Load the forecast state from disk to get agent evidence.
        state = load_forecast_state(team)
        if not state:
            print(f"  {team}: no state.json found in {FORECASTS_DIR}")
            skipped += 1
            continue

        multipliers = extract_multipliers(state)
        if not multipliers:
            print(f"  {team}: no [MULTIPLIER] blocks found in agent evidence")
            skipped += 1
            continue

        # Build the params update: each driver's (p5, p50, p95) triple.
        update = {}
        for prefix, (p5, p50, p95) in multipliers.items():
            update[f"{prefix}_p5"] = p5
            update[f"{prefix}_p50"] = p50
            update[f"{prefix}_p95"] = p95

        if args.dry_run:
            print(f"  {team} ({ws_id[:8]}): would write:")
            for k, v in sorted(update.items()):
                print(f"      {k} = {v}")
            refit_note = " [would refit]" if not args.no_refit else ""
            print(f"      → {len(update)} params written{refit_note}")
            continue

        # Merge with existing params.
        existing = get_existing_params(ws_id)
        merged = dict(existing)
        merged.update(update)

        ok_ok, err = put_params(ws_id, merged)
        if not ok_ok:
            print(f"  {team}: FAILED to write params — {err}")
            fail += 1
            continue

        print(f"  {team}: wrote {len(update)} params from agent evidence", end="")

        if not args.no_refit:
            refit_ok_flag, refit_err = trigger_refit(ws_id)
            if refit_ok_flag:
                print(f" + refit OK")
                refit_ok += 1
            else:
                print(f" + refit FAILED — {refit_err}")
                refit_fail += 1
        else:
            print()

        ok += 1

    print()
    print(f"Done: {ok} written, {fail} failed, {skipped} skipped")
    if not args.no_refit and not args.dry_run:
        print(f"Refits: {refit_ok} OK, {refit_fail} failed")


if __name__ == "__main__":
    main()
