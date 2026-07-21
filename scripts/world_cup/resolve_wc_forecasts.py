#!/usr/bin/env python3
"""
Retroactively resolve all WC 2026 forecasts with their actual outcomes,
computing Brier scores and storing them in the database.

This is a one-time backfill — after this runs, the `forecast_timeline`
endpoint will show resolved outcomes, the leaderboard will include WC
forecasts, and the BrierEvaluator will have data to read.

Outcomes:
  Spain won the tournament → resolve: ESP = YES, all others = NO

Run:
    python3 scripts/world_cup/resolve_wc_forecasts.py [--dry-run] [--api-url URL]
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
TIMEOUT = 30

WORKSPACE_MAP = Path(__file__).parent / "workspace_map.json"

# The tournament winner
WINNER = "ESP"


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def resolve_forecast(forecast_id: str, actual_outcome: bool) -> dict:
    """Resolve a single forecast and return the response."""
    resp = requests.post(
        f"{API_URL}/api/forecasts/{forecast_id}/resolve",
        headers=headers(),
        json={
            "actual_outcome": actual_outcome,
            "resolution_notes": f"WC 2026 resolution: {'Winner' if actual_outcome else 'Eliminated'}",
        },
        timeout=TIMEOUT,
    )
    if resp.status_code in (200, 201):
        return resp.json()
    # 409 Conflict means already resolved — that's fine
    if resp.status_code == 409:
        return {"status": "already_resolved"}
    resp.raise_for_status()
    return {}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be resolved without making changes",
    )
    parser.add_argument(
        "--api-url",
        default=os.environ.get("FERMI_API_URL", "https://agent-bestiary.world"),
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

    forecasts = ws_map.get("forecasts", {})
    if not forecasts:
        print("ERROR: no 'forecasts' map in workspace_map.json")
        sys.exit(1)

    print(f"API: {args.api_url}")
    print(f"Forecasts to resolve: {len(forecasts)}")
    print(f"Winner: {WINNER}{'  (DRY RUN)' if args.dry_run else ''}")
    print()

    ok = 0
    already = 0
    failed = 0

    for team_id, forecast_id in sorted(forecasts.items()):
        outcome = team_id == WINNER
        outcome_label = "YES (winner)" if outcome else "NO"

        if args.dry_run:
            print(
                f"  {team_id}: would resolve {outcome_label} (forecast {forecast_id[:8]}…)"
            )
            ok += 1
            continue

        try:
            resp = resolve_forecast(forecast_id, outcome)
            if resp.get("status") == "already_resolved":
                print(f"  {team_id}: already resolved — skipping")
                already += 1
            else:
                brier = resp.get("brier_score", "?")
                print(f"  {team_id}: {outcome_label} → Brier = {brier}")
                ok += 1
        except requests.RequestException as e:
            print(f"  {team_id}: FAILED — {e}")
            failed += 1

        time.sleep(0.05)  # polite to the API

    print()
    print(f"Done: {ok} resolved, {already} already resolved, {failed} failed")
    if ok > 0:
        print("Leaderboard should now include WC forecasts.")
        print("The BrierEvaluator can now read resolved scores.")

    # Also refresh the leaderboard
    if not args.dry_run and ok > 0:
        try:
            resp = requests.post(
                f"{args.api_url}/api/leaderboard/refresh",
                headers=headers(),
                timeout=TIMEOUT,
            )
            if resp.status_code == 200:
                print("Leaderboard refreshed.")
        except requests.RequestException:
            print("Note: leaderboard refresh endpoint not available (non-fatal)")


if __name__ == "__main__":
    main()
