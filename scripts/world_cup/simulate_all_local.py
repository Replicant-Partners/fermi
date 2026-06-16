#!/usr/bin/env python3
"""
Run the TEAM_PRIOR factor model for all 48 WC2026 teams locally — no API,
no workspaces, just the Rust binary. Prints a ranked table of tournament
strength and softmax-normalized win probabilities.

This is the development-loop equivalent of `publish_team_priors.py`:
verify the factor model produces sane rankings before pushing outputs
to the live workspace fleet.

Usage:
    python3 simulate_all_local.py [--iterations 10000] [--seed 42]
"""

import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_TEMPLATE = REPO_ROOT / "templates" / "world_cup" / "team_prior.fpl"
DEFAULT_BINARY = REPO_ROOT / "target" / "debug" / "initialize-workspace"

# Import the same canonical team list used by the spawn script.
sys.path.insert(0, str(Path(__file__).parent))
from spawn_team_priors import TEAMS  # noqa: E402


def args():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--template", default=str(DEFAULT_TEMPLATE))
    p.add_argument("--binary", default=str(DEFAULT_BINARY))
    p.add_argument("--iterations", type=int, default=10000)
    p.add_argument("--seed", type=int, default=42)
    # Softmax temperature is the only mapping from `tournament_strength`
    # (in factor-product space) to a normalized probability across teams.
    # It is NOT calibrated against any outside-view distribution — Polymarket
    # serves as a divergence reference inside the Fermi loop, not a fitting
    # target. Self-improvement (Phase 6 / BayesOps) updates the underlying
    # elasticities; T is just the scoring head.
    p.add_argument("--temperature", type=float, default=0.10,
                   help="Softmax temperature for win-prob normalization")
    return p.parse_args()


def derive_socio(team):
    """Stub socioeconomic params until World Bank fetch lands.

    Per-confederation defaults so the factor model has non-zero X1 input.
    These are placeholder log-units; real numbers swap in via the
    macro_data_agent (Step 2 in the next-session plan).
    """
    cf = team["confederation"]
    base = {
        "UEFA":     {"gdp": 4.55, "pop": 1.78, "hdi": 1.95},
        "CONMEBOL": {"gdp": 4.05, "pop": 1.60, "hdi": 1.60},
        "CONCACAF": {"gdp": 4.30, "pop": 1.70, "hdi": 1.80},
        "AFC":      {"gdp": 4.10, "pop": 2.10, "hdi": 1.50},
        "CAF":      {"gdp": 3.50, "pop": 1.80, "hdi": 1.10},
        "OFC":      {"gdp": 4.45, "pop": 0.70, "hdi": 1.85},
    }.get(cf, {"gdp": 4.0, "pop": 1.6, "hdi": 1.5})
    return base


def run_one(args_ns, team):
    socio = derive_socio(team)
    params = {
        "team_id": team["team_id"],
        "team_name": team["team_name"],
        "group": team["group"],
        "confederation": team["confederation"],
        "is_host": team["is_host"],
        "elo_current": team["elo"],
        "elo_trend": 0,
        "gdp_per_capita_log": socio["gdp"],
        "population_log": socio["pop"],
        "hdi_logit": socio["hdi"],
    }
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(params, f)
        path = f.name
    try:
        proc = subprocess.run(
            [args_ns.binary, "--template", args_ns.template,
             "--params", path,
             "--iterations", str(args_ns.iterations),
             "--seed", str(args_ns.seed),
             "--quiet"],
            check=True, capture_output=True, text=True,
        )
        return json.loads(proc.stdout)
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


def main():
    a = args()
    if not Path(a.binary).exists():
        print(f"ERROR: missing binary {a.binary}. Build: cargo build --bin initialize-workspace", file=sys.stderr)
        sys.exit(1)

    rows = []
    for i, t in enumerate(TEAMS, 1):
        try:
            out = run_one(a, t)
            ts = out["tournament_strength"]
            corr = (out.get("factor_orthogonality") or {}).get("max_abs_corr", 0)
            rows.append({
                "team_id": t["team_id"],
                "team_name": t["team_name"],
                "group": t["group"],
                "elo": t["elo"],
                "strength_mean": ts["mean"],
                "strength_p5": ts["p5"],
                "strength_p95": ts["p95"],
                "corr_max": corr,
            })
            print(f"  [{i:>2}/48] {t['team_id']:<4} strength={ts['mean']:.3f}  corr={corr:.4f}")
        except subprocess.CalledProcessError as e:
            print(f"  [{i:>2}/48] {t['team_id']:<4} FAILED: {e.stderr.strip()[:120]}")

    # Softmax-normalize.
    if rows:
        vals = [r["strength_mean"] / a.temperature for r in rows]
        m = max(vals)
        exps = [math.exp(v - m) for v in vals]
        z = sum(exps)
        for r, e in zip(rows, exps):
            r["predicted_prob"] = e / z

        rows.sort(key=lambda r: -r["strength_mean"])

        print()
        print("=" * 72)
        print(f"{'rank':>4}  {'team':<6}  {'grp':<3}  {'elo':>5}  "
              f"{'strength':>10}  {'p5..p95':>16}  {'p_win':>8}  {'corr_max':>9}")
        print("-" * 72)
        for rank, r in enumerate(rows, 1):
            band = f"{r['strength_p5']:.2f}..{r['strength_p95']:.2f}"
            print(f"{rank:>4}  {r['team_id']:<6}  {r['group']:<3}  {r['elo']:>5}  "
                  f"{r['strength_mean']:>10.3f}  {band:>16}  "
                  f"{r['predicted_prob']*100:>7.2f}%  {r['corr_max']:>9.4f}")
        print()
        top5 = sum(r["predicted_prob"] for r in rows[:5]) * 100
        top16 = sum(r["predicted_prob"] for r in rows[:16]) * 100
        favorite = rows[0]
        print(f"Favorite:  {favorite['team_id']} @ {favorite['predicted_prob']*100:.2f}%  (strength={favorite['strength_mean']:.3f})")
        print(f"Top 5 share:  {top5:>5.1f}%")
        print(f"Top 16 share: {top16:>5.1f}%")
        # Diagnostic: max-pairwise-correlation across all teams should be
        # uniformly low (the per-team noise is iid, so factor correlation
        # is purely structural — same for everyone). Drift across teams
        # would indicate the executor is leaking state.
        corrs = [r["corr_max"] for r in rows]
        print(f"factor_corr_max range: [{min(corrs):.4f}, {max(corrs):.4f}]  (target < 0.05 for near-orthogonal)")

if __name__ == "__main__":
    main()
