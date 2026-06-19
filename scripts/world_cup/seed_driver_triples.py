#!/usr/bin/env python3
"""
Build per-team (p5, p50, p95) triples for all 6 drivers and write them to
each WC workspace's params output.

Aligned with the new team_prior template (Option 2 redesign): each driver's
distribution is `triangular(<driver>_p5, <driver>_p50, <driver>_p95)`. The
triples encode both the operator's best estimate (p50) and the operator's
uncertainty (p95 - p5). Higher data quality → tighter spread.

Per-driver derivation — all values normalized so 1.0 ≈ "average team":

  socio_capital      ← gdp_per_capita_log + population_log + hdi_logit.
                        Tight spread (high data quality, World Bank current).

  institutional_     ← confederation coefficient × log10(league_revenue).
    capacity            Wider spread for nations with weak domestic leagues
                        but historic football pedigree (e.g. ARG).

  dynamic_           ← Elo rating, anchored at 1700 = 1.0.
    performance         Slope: each 100 Elo points = +0.10 in p50.
                        Tight spread (Elo is current, well-sourced).
                        LEARNABLE — refits from match goal_differential.

  squad_quality      ← Transfermarkt total squad market value, anchored
                        log10($M) where 1.0 = $200M (mid-tier).
                        LEARNABLE — refits from match wins.

  tactical_          ← Recent-cycle xG diff + tournament form. Approximated
    efficiency         from Elo trend + championship pedigree since
                        per-match xG isn't a single source for all 48 teams.
                        LEARNABLE — refits from shot_conversion observations.

  fixture_context    ← Group strength + venue host advantage. Per-fixture
                        volatility caps spread tightness.

The "spread" comes from a per-driver base half-width that's wider when
data quality is lower (qualitative metrics get ±0.30, hard data gets
±0.20).

Run:
    python3 scripts/world_cup/seed_driver_triples.py [--dry-run] [--team ARG]
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

# ─── Per-team raw inputs ──────────────────────────────────────────────
#
# Sources:
#   GDP/Pop/HDI    — World Bank WDI 2023-2024, UNDP HDR 2023.
#   Elo            — World Football Elo Ratings, mid-2026 cycle.
#   Squad value    — Transfermarkt June 2026 (where reported); else
#                    estimated from Big-5 league penetration.
#   Confederation  — FIFA confederation strength coefficient (UEFA/CONMEBOL=1.0,
#                    AFC/CAF≈0.7, CONCACAF≈0.65, OFC≈0.4).
#   League revenue — log10(annual revenue USD) for top domestic league.
#                    EPL ~9.8 (revenue ~$6B), La Liga ~9.6, Liga Profesional ~8.0.

TEAMS = {
    # CONMEBOL — UEFA/CONMEBOL=1.0
    "ARG": dict(elo=2115, gdp=12667, pop=45_700_000, hdi=0.851, squad_m=807, confed=1.0,  league_rev_log=8.0,  conmebol=True),
    "BRA": dict(elo=1978, gdp=10412, pop=215_000_000, hdi=0.760, squad_m=1100, confed=1.0, league_rev_log=8.4,  conmebol=True),
    "URY": dict(elo=1870, gdp=22565, pop=3_500_000, hdi=0.830, squad_m=290, confed=1.0,   league_rev_log=7.5,  conmebol=True),
    "ECU": dict(elo=1890, gdp=6531, pop=18_000_000, hdi=0.740, squad_m=200, confed=1.0,   league_rev_log=7.6,  conmebol=True),
    "PRY": dict(elo=1780, gdp=6466, pop=6_900_000, hdi=0.731, squad_m=80,  confed=1.0,    league_rev_log=7.4,  conmebol=True),
    "COL": dict(elo=1982, gdp=6976, pop=52_000_000, hdi=0.752, squad_m=350, confed=1.0,   league_rev_log=7.8,  conmebol=True),
    # UEFA — 1.0
    "FRA": dict(elo=2084, gdp=44460, pop=68_000_000, hdi=0.910, squad_m=1300, confed=1.0, league_rev_log=9.6,  uefa=True),
    "ESP": dict(elo=2129, gdp=32677, pop=48_000_000, hdi=0.911, squad_m=1100, confed=1.0, league_rev_log=9.6,  uefa=True),
    "GER": dict(elo=1939, gdp=52746, pop=84_000_000, hdi=0.950, squad_m=1050, confed=1.0, league_rev_log=9.5,  uefa=True),
    "ENG": dict(elo=2024, gdp=49099, pop=56_500_000, hdi=0.940, squad_m=1400, confed=1.0, league_rev_log=9.8,  uefa=True),
    "POR": dict(elo=1989, gdp=27276, pop=10_300_000, hdi=0.874, squad_m=850,  confed=1.0, league_rev_log=8.6,  uefa=True),
    "NED": dict(elo=1944, gdp=62536, pop=17_700_000, hdi=0.946, squad_m=600,  confed=1.0, league_rev_log=8.8,  uefa=True),
    "SWE": dict(elo=1755, gdp=56303, pop=10_500_000, hdi=0.952, squad_m=180,  confed=1.0, league_rev_log=8.0,  uefa=True),
    "BEL": dict(elo=1879, gdp=53376, pop=11_700_000, hdi=0.942, squad_m=550,  confed=1.0, league_rev_log=8.2,  uefa=True),
    "CHE": dict(elo=1865, gdp=104894, pop=8_800_000, hdi=0.967, squad_m=350,  confed=1.0, league_rev_log=8.0,  uefa=True),
    "AUT": dict(elo=1830, gdp=56034, pop=9_100_000, hdi=0.926, squad_m=350,  confed=1.0,  league_rev_log=8.0,  uefa=True),
    "NOR": dict(elo=1914, gdp=87961, pop=5_500_000, hdi=0.966, squad_m=600,  confed=1.0,  league_rev_log=7.8,  uefa=True),
    "CRO": dict(elo=1912, gdp=22220, pop=3_900_000, hdi=0.878, squad_m=350,  confed=1.0,  league_rev_log=7.8,  uefa=True),
    "SCO": dict(elo=1794, gdp=37300, pop=5_500_000, hdi=0.925, squad_m=200,  confed=1.0,  league_rev_log=8.4,  uefa=True),
    "CZE": dict(elo=1712, gdp=30427, pop=10_900_000, hdi=0.895, squad_m=180, confed=1.0,  league_rev_log=8.0,  uefa=True),
    "TUR": dict(elo=1849, gdp=13383, pop=85_000_000, hdi=0.838, squad_m=400, confed=1.0,  league_rev_log=8.6,  uefa=True),
    "BIH": dict(elo=1616, gdp=8429, pop=3_200_000, hdi=0.779, squad_m=80,    confed=1.0,  league_rev_log=7.0,  uefa=True),
    # CONCACAF — 0.65
    "USA": dict(elo=1780, gdp=82769, pop=340_000_000, hdi=0.927, squad_m=350, confed=0.75, league_rev_log=8.4, host=True),
    "MEX": dict(elo=1881, gdp=13926, pop=130_000_000, hdi=0.781, squad_m=180, confed=0.75, league_rev_log=8.4, host=True),
    "CAN": dict(elo=1767, gdp=53372, pop=41_000_000, hdi=0.935, squad_m=200,  confed=0.75, league_rev_log=7.9, host=True),
    "PAN": dict(elo=1730, gdp=19516, pop=4_500_000, hdi=0.820, squad_m=40,    confed=0.65, league_rev_log=7.0),
    "JAM": dict(elo=1527, gdp=6786, pop=2_900_000, hdi=0.706, squad_m=70,     confed=0.65, league_rev_log=6.5),
    "HAI": dict(elo=1536, gdp=1745, pop=11_700_000, hdi=0.552, squad_m=20,    confed=0.65, league_rev_log=6.0),
    "CPV": dict(elo=1606, gdp=4323, pop=593_000, hdi=0.661, squad_m=15,       confed=0.55, league_rev_log=5.5),
    # CAF — 0.7
    "MAR": dict(elo=1760, gdp=3672, pop=37_500_000, hdi=0.698, squad_m=180, confed=0.75,  league_rev_log=7.2),
    "EGY": dict(elo=1711, gdp=3457, pop=110_000_000, hdi=0.728, squad_m=110, confed=0.7,  league_rev_log=7.6),
    "ZAF": dict(elo=1511, gdp=6023, pop=60_000_000, hdi=0.717, squad_m=50,   confed=0.7,  league_rev_log=7.4),
    "DZA": dict(elo=1772, gdp=5260, pop=45_000_000, hdi=0.745, squad_m=180,  confed=0.75, league_rev_log=7.2),
    "SEN": dict(elo=1839, gdp=1736, pop=17_700_000, hdi=0.517, squad_m=350,  confed=0.75, league_rev_log=7.0),
    "TUN": dict(elo=1585, gdp=4124, pop=12_400_000, hdi=0.732, squad_m=70,   confed=0.7,  league_rev_log=7.0),
    "GHA": dict(elo=1510, gdp=2304, pop=34_000_000, hdi=0.602, squad_m=80,   confed=0.7,  league_rev_log=6.8),
    "CIV": dict(elo=1743, gdp=2729, pop=28_900_000, hdi=0.534, squad_m=200,  confed=0.7,  league_rev_log=6.8),
    # AFC — 0.7
    "JPN": dict(elo=1910, gdp=33950, pop=124_000_000, hdi=0.920, squad_m=350, confed=0.85, league_rev_log=8.6),
    "KOR": dict(elo=1786, gdp=32423, pop=51_700_000, hdi=0.929, squad_m=210, confed=0.85, league_rev_log=8.4),
    "AUS": dict(elo=1839, gdp=64711, pop=26_700_000, hdi=0.946, squad_m=180, confed=0.75, league_rev_log=8.4),
    "IRN": dict(elo=1756, gdp=4503, pop=88_500_000, hdi=0.774, squad_m=80,   confed=0.7,  league_rev_log=7.4),
    "SAU": dict(elo=1598, gdp=32586, pop=36_000_000, hdi=0.875, squad_m=100, confed=0.85, league_rev_log=8.6),  # Saudi Pro League cash injection lifts revenue
    "QAT": dict(elo=1447, gdp=84426, pop=2_700_000, hdi=0.875, squad_m=40,   confed=0.7,  league_rev_log=7.5),
    "JOR": dict(elo=1680, gdp=4204, pop=11_300_000, hdi=0.736, squad_m=20,   confed=0.65, league_rev_log=6.5),
    "IRQ": dict(elo=1607, gdp=5519, pop=44_500_000, hdi=0.673, squad_m=20,   confed=0.65, league_rev_log=6.5),
    "UZB": dict(elo=1714, gdp=2496, pop=36_000_000, hdi=0.727, squad_m=30,   confed=0.7,  league_rev_log=6.5),
    # OFC — 0.4
    "NZL": dict(elo=1578, gdp=48802, pop=5_200_000, hdi=0.939, squad_m=80,   confed=0.5,  league_rev_log=6.8),
    # ABW oddballs
    "CUW": dict(elo=1427, gdp=21000, pop=152_000, hdi=0.811, squad_m=15,     confed=0.55, league_rev_log=5.0),
}


def hdi_logit(hdi: float) -> float:
    h = max(0.001, min(0.999, hdi))
    return math.log(h / (1.0 - h))


def triple(p50: float, half_width: float) -> tuple[float, float, float]:
    """Build a (p5, p50, p95) triangular triple from center + half-width.
    half_width is the symmetric gap to p5/p95 (e.g. 0.20 → spread 0.40).
    Clamped to non-negative everywhere — distributions over a strength
    multiplier should never go negative.
    """
    p5 = max(0.05, p50 - half_width)
    p95 = max(p5 + 0.05, p50 + half_width)
    return (round(p5, 3), round(p50, 3), round(p95, 3))


# ─── Per-driver derivation ────────────────────────────────────────────

def derive_socio(team: dict) -> tuple[float, float, float]:
    """Macro capacity. Anchored to the existing centered-around-7.8 formula
    so we stay calibrated against world averages."""
    s = math.log10(team["gdp"]) + math.log10(team["pop"]) + hdi_logit(team["hdi"])
    p50 = 0.85 + 0.4 * (s - 7.8) / 4.0
    p50 = max(0.4, min(1.7, p50))
    # Tight: World Bank / UNDP data is current and authoritative.
    return triple(p50, 0.20)


def derive_institutional(team: dict) -> tuple[float, float, float]:
    """Federation strength + league depth.
    Confederation coefficient (0.4–1.0) × normalized league revenue.
    Anchor calibrated so a typical UEFA/CONMEBOL team lands around
    p50 ≈ 1.0 and EPL/La Liga teams reach ~1.3-1.4."""
    confed = team["confed"]
    league = team["league_rev_log"]
    # League factor: 7.0 (basement) → 0.7; 8.0 (Liga MX) → 1.0;
    # 9.0 (Bundesliga) → 1.3; 9.8 (EPL) → 1.55.
    league_factor = 1.0 + 0.30 * (league - 8.0)
    league_factor = max(0.55, min(1.55, league_factor))
    # Confed coefficient pulls minnows down without sinking them.
    # confed=1.0 → 1.0 multiplier; confed=0.65 → 0.85; confed=0.4 → 0.7.
    confed_factor = 0.55 + 0.45 * confed
    p50 = league_factor * confed_factor
    p50 = max(0.45, min(1.55, p50))
    # Wider — institutional metrics are qualitative and slow-moving.
    return triple(p50, 0.30)


def derive_dynamic(team: dict) -> tuple[float, float, float]:
    """Recent form. Anchored at Elo 1700 = 1.0; +0.10 per 100 Elo pts.
    Elo is current and authoritative → tight spread.
    Learnable, so this is just the prior; BayesOps refits over time."""
    elo = team["elo"]
    p50 = 1.0 + 0.10 * (elo - 1700) / 100.0
    p50 = max(0.4, min(1.6, p50))
    return triple(p50, 0.18)


def derive_squad(team: dict) -> tuple[float, float, float]:
    """Squad market value. Anchored at $80M = 1.0 (mid-tier WC squad);
    log-scale penalty/reward. Top WC squads (€1B+) → ~1.6; minnows
    ($10M) → 0.5.
    Transfermarkt is canonical → tight spread.

    Calibration sanity check (June 2026 Transfermarkt):
      ENG ($1.4B, log≈3.15) → p50 = 0.6 + 0.4 * (3.15 - 1.9) = 1.10
      ARG ($807M, log≈2.91) → p50 = 0.6 + 0.4 * (2.91 - 1.9) = 1.00
                                                              (??)
    The intent is for top squads to land ~1.3-1.5 not ~1.0. Boost
    coefficient to 0.55 per log10 and shift anchor."""
    sq = max(1, team["squad_m"])
    p50 = 0.7 + 0.55 * (math.log10(sq) - 1.9)  # log10(80) ≈ 1.9
    p50 = max(0.4, min(1.7, p50))
    return triple(p50, 0.20)


def derive_tactical(team: dict) -> tuple[float, float, float]:
    """Recent xG diff + championship pedigree. Approximated from Elo
    trend (which captures recent form) + a small bump for top-confederation
    teams that get harder qualifying tests.
    Wider spread because per-tournament tactical performance is volatile."""
    elo = team["elo"]
    confed = team["confed"]
    # Elo-driven base, scaled less aggressively than dynamic_performance
    # since tactical is more about coaching + style than outright strength.
    p50 = 0.85 + 0.08 * (elo - 1700) / 100.0
    p50 *= (0.85 + 0.20 * confed)
    p50 = max(0.4, min(1.5, p50))
    return triple(p50, 0.25)


def derive_fixture(team: dict) -> tuple[float, float, float]:
    """Per-fixture context. Host nations get a small boost; everyone else
    is centered at 1.0 with mild spread."""
    host = team.get("host", False)
    p50 = 1.05 if host else 1.0
    return triple(p50, 0.15)


def derive_all(team_id: str, team: dict) -> dict:
    socio = derive_socio(team)
    institutional = derive_institutional(team)
    dynamic = derive_dynamic(team)
    squad = derive_squad(team)
    tactical = derive_tactical(team)
    fixture = derive_fixture(team)
    return {
        "socio_p5":          socio[0],
        "socio_p50":         socio[1],
        "socio_p95":         socio[2],
        "institutional_p5":  institutional[0],
        "institutional_p50": institutional[1],
        "institutional_p95": institutional[2],
        "dynamic_p5":        dynamic[0],
        "dynamic_p50":       dynamic[1],
        "dynamic_p95":       dynamic[2],
        "squad_p5":          squad[0],
        "squad_p50":         squad[1],
        "squad_p95":         squad[2],
        "tactical_p5":       tactical[0],
        "tactical_p50":      tactical[1],
        "tactical_p95":      tactical[2],
        "fixture_p5":        fixture[0],
        "fixture_p50":       fixture[1],
        "fixture_p95":       fixture[2],
    }


# ─── HTTP helpers ─────────────────────────────────────────────────────

def headers():
    return {"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"}


def get_existing(workspace_id: str) -> dict:
    try:
        r = requests.get(f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
                         headers=headers(), timeout=TIMEOUT)
    except requests.exceptions.RequestException as e:
        print(f"  WARN: GET failed: {e}")
        return {}
    if r.status_code == 404:
        return {}
    if r.status_code != 200:
        return {}
    return r.json().get("value", {}) or {}


def put_params(workspace_id: str, merged: dict) -> tuple[bool, str]:
    r = requests.put(f"{API_URL}/api/workspaces/{workspace_id}/outputs/params",
                     headers=headers(),
                     json={"value": merged},
                     timeout=TIMEOUT)
    if r.status_code in (200, 201):
        return True, ""
    return False, f"HTTP {r.status_code}: {r.text[:200]}"


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--team", type=str, default=None)
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY", file=sys.stderr); sys.exit(1)

    with WORKSPACE_MAP.open() as f:
        ws_map = json.load(f)
    team_priors = ws_map.get("team_priors", {})

    teams = sorted(team_priors.keys()) if not args.team else [args.team]
    missing = [t for t in teams if t not in TEAMS]
    if missing:
        print(f"WARN: no TEAMS entry for {missing}\n")

    ok = 0; fail = 0; skipped = 0
    for team in teams:
        if team not in team_priors:
            continue
        if team not in TEAMS:
            print(f"  - {team}: no input data; skipping")
            skipped += 1; continue

        ws_id = team_priors[team]
        derived = derive_all(team, TEAMS[team])

        if args.dry_run:
            print(f"  • {team:>4}: socio p50={derived['socio_p50']:.2f} "
                  f"inst p50={derived['institutional_p50']:.2f} "
                  f"dyn p50={derived['dynamic_p50']:.2f} "
                  f"squad p50={derived['squad_p50']:.2f} "
                  f"tact p50={derived['tactical_p50']:.2f} "
                  f"fix p50={derived['fixture_p50']:.2f}")
            continue

        existing = get_existing(ws_id)
        merged = dict(existing)
        merged.update(derived)
        ok_ok, err = put_params(ws_id, merged)
        if ok_ok:
            ok += 1
            print(f"  ✓ {team}: triples written")
        else:
            fail += 1
            print(f"  ✗ {team}: {err}")

    if not args.dry_run:
        print()
        print(f"Done. ok={ok}, fail={fail}, skipped={skipped}")


if __name__ == "__main__":
    main()
