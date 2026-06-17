#!/usr/bin/env python3
"""
Patch each WC team-prior forecast's `fpl_source` with the current template.

Useful when the template changes (e.g. a syntax fix to the learnable
driver declarations) and we don't want to re-run the full respawn just
to update FPL text on already-spawned workspaces.

Renders the template per team (substituting `{team_name}`) and PUTs to
/api/forecasts/:id.

Run:
    python3 scripts/world_cup/refresh_fpl.py [--workspace TEAM_ID]
"""

import argparse
import json
import os
import sys
import time

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
TIMEOUT = 30


# Team-name lookup from team_id (mirrors respawn_aligned.py's TEAMS).
# Required because workspace_map.json only has team_id → workspace_id,
# but the template uses {team_name} for substitution.
TEAM_NAMES = {
    "MEX": "Mexico", "ZAF": "South Africa", "KOR": "South Korea", "CZE": "Czechia",
    "CAN": "Canada", "BIH": "Bosnia & Herzegovina", "QAT": "Qatar", "CHE": "Switzerland",
    "BRA": "Brazil", "MAR": "Morocco", "HAI": "Haiti", "SCO": "Scotland",
    "USA": "United States", "PRY": "Paraguay", "AUS": "Australia", "TUR": "Turkiye",
    "CIV": "Côte d'Ivoire", "ECU": "Ecuador", "GER": "Germany", "CUW": "Curaçao",
    "NED": "Netherlands", "JPN": "Japan", "SWE": "Sweden", "TUN": "Tunisia",
    "IRN": "Iran", "NZL": "New Zealand", "BEL": "Belgium", "EGY": "Egypt",
    "SAU": "Saudi Arabia", "URY": "Uruguay", "ESP": "Spain", "CPV": "Cape Verde",
    "FRA": "France", "SEN": "Senegal", "IRQ": "Iraq", "NOR": "Norway",
    "ARG": "Argentina", "DZA": "Algeria", "AUT": "Austria", "JOR": "Jordan",
    "COL": "Colombia", "JAM": "Jamaica", "POR": "Portugal", "UZB": "Uzbekistan",
    "CRO": "Croatia", "ENG": "England", "GHA": "Ghana", "PAN": "Panama",
}


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def load_template():
    path = os.path.join(
        os.path.dirname(__file__), "..", "..", "templates", "world_cup", "team_prior.fpl"
    )
    with open(path) as f:
        return f.read()


def render(template: str, team_id: str) -> str:
    return template.replace("{team_name}", TEAM_NAMES.get(team_id, team_id))


def update_forecast(forecast_id: str, fpl_source: str) -> tuple[bool, str | None]:
    body = {"fpl_source": fpl_source}
    try:
        resp = requests.put(
            f"{API_URL}/api/forecasts/{forecast_id}",
            headers=headers(),
            json=body,
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException as e:
        return False, str(e)
    if resp.status_code not in (200, 201):
        return False, f"HTTP {resp.status_code}: {resp.text[:240]}"
    return True, None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", help="Limit to a single team (e.g. ARG)")
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY")
        sys.exit(1)

    map_path = os.path.join(os.path.dirname(__file__), "workspace_map.json")
    with open(map_path) as f:
        ws_map = json.load(f)
    forecasts = ws_map.get("forecasts", {})
    if not forecasts:
        print("ERROR: no 'forecasts' map in workspace_map.json")
        sys.exit(1)

    template = load_template()

    if args.workspace:
        if args.workspace not in forecasts:
            print(f"ERROR: {args.workspace} not in forecasts map")
            sys.exit(1)
        targets = [(args.workspace, forecasts[args.workspace])]
    else:
        targets = sorted(forecasts.items())

    print(f"Refreshing FPL on {len(targets)} forecast(s)…")
    print()
    ok_count = 0
    fail_count = 0
    for team_id, forecast_id in targets:
        fpl = render(template, team_id)
        ok, err = update_forecast(forecast_id, fpl)
        if ok:
            print(f"  {team_id}: OK ({forecast_id})")
            ok_count += 1
        else:
            print(f"  {team_id}: FAIL — {err}")
            fail_count += 1
        time.sleep(0.05)

    print()
    print(f"Summary: {ok_count} updated, {fail_count} failed.")
    if ok_count > 0:
        print()
        print("Next: re-fire the refit on a team to verify FPL parses now:")
        sample = next(iter(t for t, _ in targets), "ARG")
        ws_id = ws_map.get("team_priors", {}).get(sample, "<id>")
        print(f"  curl -X POST '{API_URL}/api/workspaces/{ws_id}/refit' \\\\")
        print(f"    -H 'Authorization: Bearer $FERMI_API_KEY' \\\\")
        print(f"    -H 'Content-Type: application/json' \\\\")
        print(f"    -d '{{}}' | jq")


if __name__ == "__main__":
    main()
