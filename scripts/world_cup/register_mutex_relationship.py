#!/usr/bin/env python3
"""
Register the WC sims mutually_exclusive relationship.

Ties the 48 team-prior forecasts together as "exactly one of these is
true." When the operator resolves any of them via the cockpit, the
Resolve sheet will surface a "Cascade to 47 forecasts" affordance —
clicking propagates the resolution across siblings:

  Resolve YES (team won): all 47 others drop to ~0.
  Resolve NO (team eliminated): the team's previous probability is
    redistributed across the other 47, proportional to their current
    probability.

Idempotent: if a relationship with kind='mutually_exclusive' and the
exact same forecast_ids already exists for the operator, this script
finds it and reports it instead of creating a duplicate.

Run:
    python3 scripts/world_cup/register_mutex_relationship.py [--dry-run]
"""

import argparse
import json
import os
import sys
from pathlib import Path

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
TIMEOUT = 30

WORKSPACE_MAP = Path(__file__).parent / "workspace_map.json"


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dry-run", action="store_true",
                        help="Print the relationship payload without registering")
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY", file=sys.stderr)
        sys.exit(1)

    with WORKSPACE_MAP.open() as f:
        ws_map = json.load(f)
    forecasts = ws_map.get("forecasts", {})
    if len(forecasts) < 2:
        print(f"ERROR: only {len(forecasts)} forecasts in workspace_map.json/forecasts; "
              "expected 48", file=sys.stderr)
        sys.exit(1)

    forecast_ids = sorted(forecasts.values())

    # Check for duplicate relationship (idempotency)
    pick = forecast_ids[0]
    r = requests.get(
        f"{API_URL}/api/forecast-relationships?forecast_id={pick}",
        headers=headers(),
        timeout=TIMEOUT,
    )
    if r.status_code == 200:
        existing = r.json().get("relationships", [])
        for rel in existing:
            if rel.get("kind") == "mutually_exclusive" \
               and sorted(rel.get("forecast_ids") or []) == forecast_ids:
                print(f"Relationship already registered: {rel.get('id')}")
                print(f"  kind: mutually_exclusive")
                print(f"  members: {len(forecast_ids)} forecasts")
                return

    payload = {
        "kind": "mutually_exclusive",
        "forecast_ids": forecast_ids,
        "parameters": {
            "tournament": "FIFA World Cup 2026",
            "constraint": "Exactly one team wins.",
        },
        "description": "FIFA World Cup 2026 — winner mutex (48 teams).",
    }

    if args.dry_run:
        print("DRY RUN — would POST:")
        print(json.dumps(payload, indent=2)[:600] + "...")
        print(f"  ({len(forecast_ids)} forecast_ids)")
        return

    r = requests.post(
        f"{API_URL}/api/forecast-relationships",
        headers=headers(),
        json=payload,
        timeout=TIMEOUT,
    )
    if r.status_code != 200:
        print(f"ERROR: HTTP {r.status_code}: {r.text[:300]}", file=sys.stderr)
        sys.exit(1)
    body = r.json()
    print(f"Created: {body.get('id')}")
    print(f"  kind: {body.get('kind')}")
    print(f"  members: {body.get('n_forecasts')} forecasts")
    print()
    print("Next: resolve any team in the cockpit; the Resolve sheet will")
    print("surface a 'Cascade to 47 forecasts' button.")


if __name__ == "__main__":
    main()
