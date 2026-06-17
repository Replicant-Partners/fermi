#!/usr/bin/env python3
"""
Link each WC team-prior forecast to its Polymarket tournament-winner market.

Pulls the `world-cup-winner` event from Polymarket's Gamma API, parses
the 48 nested markets, matches each market to one of our 48 forecasts
by team identifier, and POSTs to /api/polymarket/link.

After linking, the forecast's `metadata.polymarket` field is populated
with pm_event_id / pm_market_id / current price, and
fermi_market_observations starts accumulating snapshots over time.

This is what makes the cockpit's Polymarket delta display work for WC
forecasts and unlocks the spacetime view's market trace.

Run:
    python3 scripts/world_cup/link_polymarket.py [--dry-run] [--workspace TEAM_ID]
"""

import argparse
import json
import os
import sys
import time

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
GAMMA_URL = "https://gamma-api.polymarket.com"
TIMEOUT = 30

# Map: Polymarket team-name spelling → our team_id.
#
# Polymarket's `groupItemTitle` is a free-form display string ("United
# States", "South Korea", "Côte d'Ivoire"). Our team_ids are FIFA 3-letter
# codes. This alias map handles the cases where they don't match by
# trivial uppercase/strip — anything that needs disambiguation, special
# characters, or known variants.
#
# Keys: exact strings as Polymarket displays them. Values: our team_id.
TEAM_NAME_TO_ID = {
    # Group A
    "Mexico": "MEX", "South Africa": "ZAF",
    "South Korea": "KOR", "Korea Republic": "KOR",
    "Czechia": "CZE", "Czech Republic": "CZE",
    # Group B
    "Canada": "CAN",
    "Bosnia & Herzegovina": "BIH", "Bosnia and Herzegovina": "BIH", "Bosnia-Herzegovina": "BIH",
    "Qatar": "QAT",
    "Switzerland": "CHE",
    # Group C
    "Brazil": "BRA", "Morocco": "MAR",
    "Haiti": "HAI", "Scotland": "SCO",
    # Group D
    "United States": "USA", "USA": "USA",
    "Paraguay": "PRY", "Australia": "AUS",
    "Turkey": "TUR", "Turkiye": "TUR", "Türkiye": "TUR",
    # Group E
    "Ivory Coast": "CIV", "Côte d'Ivoire": "CIV", "Cote d'Ivoire": "CIV",
    "Ecuador": "ECU", "Germany": "GER",
    "Curaçao": "CUW", "Curacao": "CUW",
    # Group F
    "Netherlands": "NED", "Japan": "JPN",
    "Sweden": "SWE", "Tunisia": "TUN",
    # Group G
    "Iran": "IRN", "I.R. Iran": "IRN",
    "New Zealand": "NZL",
    "Belgium": "BEL", "Egypt": "EGY",
    # Group H
    "Saudi Arabia": "SAU",
    "Uruguay": "URY",
    "Spain": "ESP", "Cape Verde": "CPV",
    # Group I
    "France": "FRA", "Senegal": "SEN",
    "Iraq": "IRQ", "Norway": "NOR",
    # Group J
    "Argentina": "ARG",
    "Algeria": "DZA",
    "Austria": "AUT", "Jordan": "JOR",
    # Group K
    "Colombia": "COL", "Jamaica": "JAM",
    "Portugal": "POR",
    "Uzbekistan": "UZB",
    # Group L
    "Croatia": "CRO", "England": "ENG",
    "Ghana": "GHA", "Panama": "PAN",
}


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def fetch_wc_event(slug: str = "world-cup-winner"):
    """Fetch a Polymarket event by slug from the Gamma API.

    Gamma returns a list of events matching the slug filter, even though
    slugs are unique. We take the first.
    """
    try:
        resp = requests.get(
            f"{GAMMA_URL}/events",
            params={"slug": slug},
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException as e:
        print(f"  ERROR: Gamma API fetch failed: {e}")
        return None
    if resp.status_code != 200:
        print(f"  ERROR: Gamma returned HTTP {resp.status_code}: {resp.text[:240]}")
        return None
    events = resp.json()
    if not events or not isinstance(events, list):
        print(f"  ERROR: Gamma response not a list: {resp.text[:240]}")
        return None
    return events[0]


def extract_team_from_market(market: dict) -> str | None:
    """Try several heuristics to identify which team a market is for.

    1. `groupItemTitle` is the cleanest signal — Polymarket sets it to
       the team name for grouped markets like world-cup-winner.
    2. Fallback: parse the question text for known team names.
    """
    # 1. groupItemTitle direct lookup
    gt = market.get("groupItemTitle") or ""
    if gt in TEAM_NAME_TO_ID:
        return TEAM_NAME_TO_ID[gt]

    # 2. Question text scan
    q = market.get("question") or ""
    for name, tid in TEAM_NAME_TO_ID.items():
        if name in q:
            return tid

    return None


def link_one(forecast_id: str, pm_event_id: str, pm_market_id: str):
    body = {
        "forecast_id": forecast_id,
        "pm_event_id": pm_event_id,
        "pm_market_id": pm_market_id,
    }
    try:
        resp = requests.post(
            f"{API_URL}/api/polymarket/link",
            headers=headers(),
            json=body,
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException as e:
        return False, str(e)
    if resp.status_code not in (200, 201):
        return False, f"HTTP {resp.status_code}: {resp.text[:240]}"
    return True, resp.json()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would be linked, don't actually link")
    parser.add_argument("--workspace",
                        help="Limit to a single team (e.g. ARG)")
    parser.add_argument("--slug", default="world-cup-winner",
                        help="Polymarket event slug (default: world-cup-winner)")
    args = parser.parse_args()

    if not API_KEY and not args.dry_run:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY")
        sys.exit(1)

    # Load workspace + forecast map
    map_path = os.path.join(os.path.dirname(__file__), "workspace_map.json")
    with open(map_path) as f:
        ws_map = json.load(f)
    forecasts = ws_map.get("forecasts", {})
    if not forecasts:
        print("ERROR: no 'forecasts' map in workspace_map.json")
        sys.exit(1)

    # Fetch the Polymarket event
    print(f"Fetching Polymarket event '{args.slug}' from Gamma…")
    event = fetch_wc_event(args.slug)
    if not event:
        sys.exit(1)
    pm_event_id = event.get("id")
    markets = event.get("markets") or []
    print(f"  event_id: {pm_event_id}")
    print(f"  title:    {event.get('title')}")
    print(f"  markets:  {len(markets)}")
    print()

    # Build market_id by team_id
    market_by_team = {}
    unmatched_markets = []
    for m in markets:
        tid = extract_team_from_market(m)
        if tid:
            market_by_team[tid] = m
        else:
            unmatched_markets.append({
                "id": m.get("id"),
                "groupItemTitle": m.get("groupItemTitle"),
                "question": (m.get("question") or "")[:100],
            })

    if unmatched_markets:
        print(f"WARNING: {len(unmatched_markets)} Polymarket markets couldn't be matched to a team:")
        for um in unmatched_markets:
            print(f"  - {um}")
        print()

    # Now link each forecast to its market
    if args.workspace:
        targets = [(args.workspace, forecasts.get(args.workspace))]
    else:
        targets = sorted(forecasts.items())

    print(f"Linking {len(targets)} forecast(s)…")
    print()

    ok_count = 0
    fail_count = 0
    skip_count = 0
    for team_id, forecast_id in targets:
        if not forecast_id:
            print(f"  {team_id}: SKIP — no forecast_id in map")
            skip_count += 1
            continue
        market = market_by_team.get(team_id)
        if not market:
            print(f"  {team_id}: SKIP — no matching Polymarket market")
            skip_count += 1
            continue

        pm_market_id = market.get("id")
        price = market.get("lastTradePrice")

        if args.dry_run:
            print(f"  {team_id}: would link forecast {forecast_id[:8]}… "
                  f"→ market {pm_market_id} "
                  f"(price={price}, question={market.get('groupItemTitle')!r})")
            ok_count += 1
            continue

        ok, result = link_one(forecast_id, pm_event_id, pm_market_id)
        if ok:
            print(f"  {team_id}: OK — market {pm_market_id} "
                  f"(PM price ≈ {price})")
            ok_count += 1
        else:
            print(f"  {team_id}: FAIL — {result}")
            fail_count += 1
        time.sleep(0.05)  # polite to the API

    print()
    print(f"Summary: ok={ok_count}, failed={fail_count}, skipped={skip_count}")
    if args.dry_run:
        print("(DRY RUN — no links were created. Re-run without --dry-run to commit.)")


if __name__ == "__main__":
    main()
