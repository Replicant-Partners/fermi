#!/usr/bin/env python3
"""
Backfill the socio params (gdp_per_capita_log, population_log, hdi_logit)
on every WC team workspace.

The spawn script (respawn_aligned.py) hardcoded 0.0 for these three fields
with a comment promising "macro_data_agent fills these in once it runs."
That promise was never wired up — the agent puts the values in evidence
text but doesn't push them into workspace_outputs.params, so the FPL's
distribution expressions silently evaluate to 0.0 (the EvalError is
caught and substituted) and every team's socio_capital driver collapses
to ~triangular(-0.18, 0.07, 0.37). Net effect: every team's posterior
clamps to the cockpit's 1% floor regardless of strength.

This script writes plausible static values across all 48 workspaces so
the simulation produces sensible per-team rates while the longer-term
"agent writes to params" wire is being built.

Data shapes:
  - gdp_per_capita_log: log10(GDP per capita USD, World Bank 2023–2024).
    Range ~3.0 (HAI ~$1,800) to ~5.1 (CHE ~$104k).
  - population_log: log10(population). Range ~5.2 (CPV ~520k) to
    ~9.43 (USA ~340M).
  - hdi_logit: logit(HDI 2023, UNDP). Range ~0.5 (HAI 0.55) to
    ~3.5 (CHE 0.97).

The cockpit's load_workspace_params reads this into the executor's
EvaluationContext. Distribution expressions like
    triangular(0.6 + 0.4*(gdp+pop+hdi-7.8)/4.0, ...)
then evaluate to per-team values centered around the team's actual
socioeconomic capacity.

Run:
    python3 scripts/world_cup/backfill_socio_params.py [--dry-run]
"""

import argparse
import json
import math
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


def hdi_logit(hdi: float) -> float:
    """logit(HDI): (HDI bounded in (0, 1) → ℝ)."""
    h = max(0.001, min(0.999, hdi))
    return math.log(h / (1.0 - h))


# ─── Per-team socio data ──────────────────────────────────────────────
#
# Keyed by team_id. Each entry: (gdp_per_capita_usd, population, hdi).
# Sources: World Bank WDI (most recent year 2023 or 2024 where available),
# UNDP HDR 2023.
#
# These are deliberately approximate — within ±10% of the published
# values. The model is a relative-comparison engine; getting orders of
# magnitude right matters more than three decimal places.

SOCIO = {
    # CONMEBOL
    "ARG": (12_667, 45_700_000, 0.851),
    "BRA": (10_412, 215_000_000, 0.760),
    "URY": (22_565, 3_500_000, 0.830),
    "ECU": (6_531, 18_000_000, 0.740),
    "PRY": (6_466, 6_900_000, 0.731),
    "COL": (6_976, 52_000_000, 0.752),

    # UEFA
    "FRA": (44_460, 68_000_000, 0.910),
    "ESP": (32_677, 48_000_000, 0.911),
    "GER": (52_746, 84_000_000, 0.950),
    "ENG": (49_099, 56_500_000, 0.940),  # England: UK GDPpc, Eng pop
    "POR": (27_276, 10_300_000, 0.874),
    "NED": (62_536, 17_700_000, 0.946),
    "SWE": (56_303, 10_500_000, 0.952),
    "BEL": (53_376, 11_700_000, 0.942),
    "CHE": (104_894, 8_800_000, 0.967),
    "AUT": (56_034, 9_100_000, 0.926),
    "NOR": (87_961, 5_500_000, 0.966),
    "CRO": (22_220, 3_900_000, 0.878),
    "SCO": (37_300, 5_500_000, 0.925),  # Scotland: UK estimates
    "CZE": (30_427, 10_900_000, 0.895),
    "TUR": (13_383, 85_000_000, 0.838),
    "BIH": (8_429, 3_200_000, 0.779),

    # CONCACAF
    "USA": (82_769, 340_000_000, 0.927),
    "MEX": (13_926, 130_000_000, 0.781),
    "CAN": (53_372, 41_000_000, 0.935),
    "PAN": (19_516, 4_500_000, 0.820),
    "JAM": (6_786, 2_900_000, 0.706),
    "HAI": (1_745, 11_700_000, 0.552),
    "CPV": (4_323, 593_000, 0.661),  # Cabo Verde

    # CAF
    "MAR": (3_672, 37_500_000, 0.698),
    "EGY": (3_457, 110_000_000, 0.728),
    "ZAF": (6_023, 60_000_000, 0.717),
    "DZA": (5_260, 45_000_000, 0.745),
    "SEN": (1_736, 17_700_000, 0.517),
    "TUN": (4_124, 12_400_000, 0.732),
    "GHA": (2_304, 34_000_000, 0.602),
    "CIV": (2_729, 28_900_000, 0.534),  # Côte d'Ivoire

    # AFC
    "JPN": (33_950, 124_000_000, 0.920),
    "KOR": (32_423, 51_700_000, 0.929),
    "AUS": (64_711, 26_700_000, 0.946),
    "IRN": (4_503, 88_500_000, 0.774),
    "SAU": (32_586, 36_000_000, 0.875),
    "QAT": (84_426, 2_700_000, 0.875),
    "JOR": (4_204, 11_300_000, 0.736),
    "IRQ": (5_519, 44_500_000, 0.673),
    "UZB": (2_496, 36_000_000, 0.727),

    # OFC
    "NZL": (48_802, 5_200_000, 0.939),

    # ABW: also need CUW (Curaçao) — small Caribbean territory in our 48
    "CUW": (21_000, 152_000, 0.811),  # rough
}


def derived_params(team_id: str) -> dict | None:
    if team_id not in SOCIO:
        return None
    gdp, pop, hdi = SOCIO[team_id]
    return {
        "gdp_per_capita_log": round(math.log10(gdp), 4),
        "population_log": round(math.log10(pop), 4),
        "hdi_logit": round(hdi_logit(hdi), 4),
        # Bookkeeping so the evidence is traceable in workspace_outputs:
        "gdp_per_capita_usd": gdp,
        "population": pop,
        "hdi": hdi,
    }


def get_existing_params(workspace_id: str) -> dict:
    """Pull current params output; preserve fields we don't override."""
    try:
        r = requests.get(
            f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
            headers=headers(),
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException as e:
        print(f"  WARN: GET failed: {e}")
        return {}
    if r.status_code == 404:
        return {}
    if r.status_code != 200:
        print(f"  WARN: GET returned {r.status_code}: {r.text[:120]}")
        return {}
    return r.json().get("value", {}) or {}


def put_params(workspace_id: str, merged: dict) -> tuple[bool, str]:
    r = requests.put(
        f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
        headers=headers(),
        json={"value": merged},
        timeout=TIMEOUT,
    )
    if r.status_code in (200, 201):
        return True, ""
    return False, f"HTTP {r.status_code}: {r.text[:200]}"


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dry-run", action="store_true",
                        help="Print derived values without writing.")
    parser.add_argument("--team", type=str, default=None,
                        help="Restrict to one team_id (e.g. ARG).")
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY", file=sys.stderr)
        sys.exit(1)

    with WORKSPACE_MAP.open() as f:
        ws_map = json.load(f)
    team_priors = ws_map.get("team_priors", {})

    teams = sorted(team_priors.keys()) if not args.team else [args.team]
    missing = [t for t in teams if t not in SOCIO]
    if missing:
        print(f"WARN: no SOCIO entry for {missing} — they'll be skipped.\n")

    ok = 0
    fail = 0
    skipped = 0
    for team in teams:
        if team not in team_priors:
            print(f"  ✗ {team}: not in workspace_map")
            continue
        ws_id = team_priors[team]
        derived = derived_params(team)
        if not derived:
            print(f"  - {team}: no SOCIO data; skipping")
            skipped += 1
            continue

        if args.dry_run:
            print(f"  • {team} ({ws_id[:8]}): "
                  f"gdp_log={derived['gdp_per_capita_log']:.3f}, "
                  f"pop_log={derived['population_log']:.3f}, "
                  f"hdi_logit={derived['hdi_logit']:.3f} "
                  f"(GDP=${SOCIO[team][0]:,}, pop={SOCIO[team][1]:,}, HDI={SOCIO[team][2]})")
            continue

        existing = get_existing_params(ws_id)
        merged = dict(existing)
        merged.update(derived)
        ok_ok, err = put_params(ws_id, merged)
        if ok_ok:
            ok += 1
            print(f"  ✓ {team}: gdp_log={derived['gdp_per_capita_log']:.3f}, "
                  f"pop_log={derived['population_log']:.3f}, "
                  f"hdi_logit={derived['hdi_logit']:.3f}")
        else:
            fail += 1
            print(f"  ✗ {team}: {err}")

    if not args.dry_run:
        print()
        print(f"Done. ok={ok}, fail={fail}, skipped={skipped}")


if __name__ == "__main__":
    main()
