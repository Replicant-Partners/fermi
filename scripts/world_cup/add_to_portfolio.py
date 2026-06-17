#!/usr/bin/env python3
"""
Add the 48 WC team-prior forecasts to a portfolio (default: "WC sims").

Reads `forecasts` map from workspace_map.json, resolves the target
portfolio by title (or accepts an explicit --portfolio-id), and POSTs
each forecast to /api/portfolios/:id/forecasts. The endpoint is
idempotent (ON CONFLICT DO NOTHING) so re-runs are safe.

Why a script and not a workspace_id-based auto-link: portfolios are
forecaster-scoped, not App-scoped — the demo's narrative is "look at
*my* WC portfolio", which means the operator picks which portfolio.
Adding it inline at forecast-create time would couple App spawning to
portfolio choice and break the abstraction.

Run:
    python3 scripts/world_cup/add_to_portfolio.py [--portfolio "WC sims"] [--dry-run]
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


def resolve_portfolio(title_or_id: str) -> tuple[str, str, int]:
    """Return (portfolio_id, title, current_forecast_count). Accepts UUID or title."""
    # If it looks like a UUID, fetch directly.
    if len(title_or_id) == 36 and title_or_id.count("-") == 4:
        r = requests.get(
            f"{API_URL}/api/portfolios/{title_or_id}", headers=headers(), timeout=TIMEOUT
        )
        r.raise_for_status()
        p = r.json()
        return p["id"], p.get("title", "?"), p.get("forecast_count", 0) or 0

    # Otherwise list and match by title.
    r = requests.get(f"{API_URL}/api/portfolios", headers=headers(), timeout=TIMEOUT)
    r.raise_for_status()
    portfolios = r.json().get("portfolios", [])
    matches = [p for p in portfolios if p.get("title") == title_or_id]
    if not matches:
        titles = [p.get("title") for p in portfolios]
        raise SystemExit(
            f"No portfolio titled {title_or_id!r}. Available: {titles}"
        )
    if len(matches) > 1:
        ids = [p["id"] for p in matches]
        raise SystemExit(
            f"Multiple portfolios titled {title_or_id!r}: {ids}. Pass --portfolio <id>."
        )
    p = matches[0]
    return p["id"], p["title"], p.get("forecast_count", 0) or 0


def add_forecast(portfolio_id: str, forecast_id: str) -> tuple[bool, str]:
    """POST to add. Returns (ok, message). Idempotent server-side."""
    r = requests.post(
        f"{API_URL}/api/portfolios/{portfolio_id}/forecasts",
        headers=headers(),
        data=json.dumps({"forecast_id": forecast_id}),
        timeout=TIMEOUT,
    )
    if r.status_code == 200:
        return True, r.json().get("status", "added")
    return False, f"HTTP {r.status_code}: {r.text[:200]}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--portfolio",
        default="WC sims",
        help="Portfolio title or UUID (default: 'WC sims')",
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="Show what would be added without POSTing"
    )
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY", file=sys.stderr)
        sys.exit(1)

    if not WORKSPACE_MAP.exists():
        print(f"ERROR: {WORKSPACE_MAP} not found", file=sys.stderr)
        sys.exit(1)

    with WORKSPACE_MAP.open() as f:
        ws_map = json.load(f)

    forecasts = ws_map.get("forecasts", {})
    if not forecasts:
        print("ERROR: workspace_map.json has no 'forecasts' map", file=sys.stderr)
        sys.exit(1)

    portfolio_id, portfolio_title, current_count = resolve_portfolio(args.portfolio)
    print(f"Portfolio: {portfolio_title} ({portfolio_id})")
    print(f"  Current forecast count: {current_count}")
    print(f"  Forecasts to add:       {len(forecasts)}")
    print()

    if args.dry_run:
        print("DRY RUN — would add:")
        for team, fid in sorted(forecasts.items()):
            print(f"  {team}: {fid}")
        return

    added = 0
    failed: list[tuple[str, str]] = []
    for team, fid in sorted(forecasts.items()):
        ok, msg = add_forecast(portfolio_id, fid)
        if ok:
            added += 1
            print(f"  ✓ {team} ({fid[:8]}): {msg}")
        else:
            failed.append((team, msg))
            print(f"  ✗ {team} ({fid[:8]}): {msg}")

    print()
    print(f"Done. Added {added}/{len(forecasts)}.")
    if failed:
        print("Failures:")
        for team, msg in failed:
            print(f"  {team}: {msg}")
        sys.exit(1)


if __name__ == "__main__":
    main()
