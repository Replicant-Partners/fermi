#!/usr/bin/env python3
"""
Batch-spawn weather bucket-ladder workspaces from a pre-built plan.

Usage:
    cargo run --example weather_spawn_plan -- London,NYC 2026-08-16 > plan.json
    python3 scripts/weather/spawn_bucket_ladders.py plan.json [--dry-run]

Deliberately a thin poster. Every parameter is computed by the Rust example,
which calls the same tools the agents call (weather_settlement_spec,
weather_dispersion_fit, weather_ensemble_forecast, polymarket_weather_markets).
Reimplementing that logic here would give two sources of truth for the
calibration, and the calibration is the part that is easy to get quietly wrong.

Uses the same batch endpoint as scripts/world_cup/spawn_team_priors.py:
    POST /api/apps/fermi_forecast/workspaces/batch
"""

import argparse
import json
import os
import sys
import time

import requests

API_URL = os.environ.get("FERMI_API_URL", "https://agent-bestiary.world")
API_KEY = os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", ""))
BATCH_SIZE = 10


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def preflight(plan):
    """Refuse to spawn a plan that would produce miscalibrated workspaces.

    Each check corresponds to a failure that has actually happened during this
    template's development, so none of them is hypothetical.
    """
    problems, warnings = [], []

    if not plan.get("instances"):
        problems.append("plan contains no instances")

    for inst in plan.get("instances", []):
        p = inst.get("params", {})
        name = p.get("bucket_label", "?")
        station = p.get("station", "?")

        # A guessed sd is the thing the measured fit exists to replace.
        if not p.get("predictive_sd"):
            problems.append(f"{station}/{name}: no predictive_sd — dispersion was never fitted")

        # A bucket is an interval. A one-sided read was a 6x error in production.
        lo, hi = p.get("bucket_lo"), p.get("bucket_hi")
        if lo is None or hi is None or hi <= lo:
            problems.append(f"{station}/{name}: bad bucket edges ({lo}, {hi})")

        # The settlement station is the single largest error source.
        if not p.get("station") or not p.get("timezone"):
            problems.append(f"{name}: settlement station or timezone unresolved")

        if p.get("sd_is_upper_bound"):
            warnings.append(
                f"{station}/{name}: lead 0 — sd is an UPPER BOUND from lead 1. "
                "Tighten with the running maximum before sizing."
            )
        if p.get("market_mid") is None:
            warnings.append(f"{station}/{name}: no live market price; cannot compute edge yet")

    for note in plan.get("notes", []):
        if "synthetic ladder" in note:
            warnings.append("synthetic ladder in use — verify the real market slug before trading")

    return problems, warnings


def spawn(instances, dry_run):
    total, errors = 0, 0
    for start in range(0, len(instances), BATCH_SIZE):
        batch = instances[start : start + BATCH_SIZE]
        label = f"batch {start // BATCH_SIZE + 1} ({len(batch)} instances)"
        if dry_run:
            print(f"  [dry-run] would POST {label}")
            for b in batch:
                print(f"             {b['name']}")
            total += len(batch)
            continue
        try:
            resp = requests.post(
                f"{API_URL}/api/apps/fermi_forecast/workspaces/batch",
                headers=headers(),
                json={"instances": batch},
                timeout=120,
            )
        except requests.exceptions.RequestException as e:
            print(f"  ✗ {label}: {e}")
            errors += len(batch)
            continue
        if resp.status_code >= 300:
            print(f"  ✗ {label}: HTTP {resp.status_code} {resp.text[:200]}")
            errors += len(batch)
        else:
            print(f"  ✓ {label}")
            total += len(batch)
        time.sleep(1)
    return total, errors


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("plan", help="JSON plan from the weather_spawn_plan example")
    ap.add_argument("--dry-run", action="store_true", help="validate and print, do not POST")
    ap.add_argument("--force", action="store_true", help="spawn despite preflight problems")
    args = ap.parse_args()

    with open(args.plan) as fh:
        plan = json.load(fh)

    print(f"Plan generated {plan.get('generated_at', '?')}")
    print(f"Target date    {plan.get('target_date', '?')}")
    print(f"Cities         {', '.join(plan.get('cities', []))}")
    print(f"Instances      {plan.get('instance_count', 0)}"
          f"  ({len(plan.get('skipped', []))} buckets skipped as negligible)")
    print()

    problems, warnings = preflight(plan)
    for w in warnings:
        print(f"  ⚠ {w}")
    for p in problems:
        print(f"  ✗ {p}")
    if problems and not args.force:
        print("\nPreflight failed. Fix the plan or pass --force.")
        sys.exit(1)
    print()

    if not args.dry_run and not API_KEY:
        print("ERROR: set FERMI_API_KEY or ABW_API_KEY")
        sys.exit(1)

    total, errors = spawn(plan["instances"], args.dry_run)
    print()
    print(f"Spawned {total}, failed {errors}")

    # The sizing hazard these workspaces share. Stated at the end because it is
    # the thing most likely to lose money after everything above went right.
    stations = sorted({i["params"]["station"] for i in plan["instances"]})
    if len(plan["instances"]) > 1:
        print()
        print("─" * 68)
        print("SIZING: do NOT apply per-market Kelly across these positions.")
        print("Buckets within one ladder are mutually exclusive, and stations on")
        print("the same synoptic system share forecast error. Measure it first:")
        print()
        print(f"  cargo run --example weather_portfolio_risk -- {','.join(stations)}")
        print()
        print("Then apply the reported Kelly haircut.")
        print("─" * 68)


if __name__ == "__main__":
    main()
