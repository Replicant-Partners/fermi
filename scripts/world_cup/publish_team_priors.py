#!/usr/bin/env python3
"""
Batch-initialize World Cup 2026 team prior workspaces.

For each workspace in workspace_map.json:
  1. Fetch params from /api/workspaces/:id/outputs/params (written at spawn time)
  2. Write to a tempfile, invoke `initialize-workspace` Rust binary
  3. Take the returned outputs JSON and PUT each key to
     /api/workspaces/:id/outputs/:key

After all team priors have published `tournament_strength`, we compute a
softmax-normalized `predicted_probability` across all 48 teams and PUT that
back as a second-pass output. This is the actual tournament win prior used
by group-path workspaces.

Usage:
    python3 publish_team_priors.py \
        [--api-url URL] [--api-key KEY] \
        [--template templates/world_cup/team_prior.fpl] \
        [--binary ./target/debug/initialize-workspace] \
        [--map scripts/world_cup/workspace_map.json] \
        [--only ARG,BRA,FRA]    # publish only specific teams
"""

import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import requests

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_TEMPLATE = REPO_ROOT / "templates" / "world_cup" / "team_prior.fpl"
DEFAULT_BINARY = REPO_ROOT / "target" / "debug" / "initialize-workspace"
DEFAULT_MAP = REPO_ROOT / "scripts" / "world_cup" / "workspace_map.json"


def parse_args():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--api-url", default=os.environ.get("FERMI_API_URL", "https://agent-bestiary.world"))
    p.add_argument("--api-key", default=os.environ.get("FERMI_API_KEY", os.environ.get("ABW_API_KEY", "")))
    p.add_argument("--template", default=str(DEFAULT_TEMPLATE))
    p.add_argument("--binary", default=str(DEFAULT_BINARY))
    p.add_argument("--map", default=str(DEFAULT_MAP))
    p.add_argument("--iterations", type=int, default=10000)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--only", default="", help="Comma-separated team_ids to publish (default: all)")
    p.add_argument("--dry-run", action="store_true", help="Run simulations but do not PUT outputs")
    return p.parse_args()


def auth_headers(api_key):
    return {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }


def get_params(api_url, api_key, workspace_id):
    """Fetch the `params` output written at spawn time."""
    url = f"{api_url}/api/workspaces/{workspace_id}/outputs/params"
    r = requests.get(url, headers=auth_headers(api_key), timeout=30)
    if r.status_code == 404:
        return None
    r.raise_for_status()
    body = r.json()
    # GET output handler returns { "value": ... } or the value directly,
    # depending on the schema. Handle both shapes defensively.
    if isinstance(body, dict) and "value" in body and isinstance(body["value"], dict):
        return body["value"]
    return body


def run_simulation(binary, template, params, iterations, seed):
    """Invoke the Rust initialize-workspace binary and return the outputs JSON."""
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(params, f)
        params_path = f.name
    try:
        result = subprocess.run(
            [
                binary,
                "--template", template,
                "--params", params_path,
                "--iterations", str(iterations),
                "--seed", str(seed),
                "--quiet",
            ],
            check=True, capture_output=True, text=True,
        )
        return json.loads(result.stdout)
    finally:
        try:
            os.unlink(params_path)
        except OSError:
            pass


def put_output(api_url, api_key, workspace_id, key, value):
    url = f"{api_url}/api/workspaces/{workspace_id}/outputs/{key}"
    r = requests.put(
        url,
        headers=auth_headers(api_key),
        json={"value": value},
        timeout=30,
    )
    r.raise_for_status()
    return r.json()


def publish_outputs(api_url, api_key, workspace_id, outputs, dry_run=False):
    """PUT each top-level key from outputs as a separate workspace output.

    Keys: tournament_strength, factor_means, factor_std_devs,
          factor_orthogonality, params, estimate_name, n_iterations
    """
    published = []
    failed = []
    for key, value in outputs.items():
        if key == "params":
            # Don't overwrite the spawn-time params output.
            continue
        if dry_run:
            published.append(key)
            continue
        try:
            put_output(api_url, api_key, workspace_id, key, value)
            published.append(key)
        except requests.HTTPError as e:
            failed.append((key, f"HTTP {e.response.status_code}: {e.response.text[:120]}"))
        except requests.RequestException as e:
            failed.append((key, str(e)))
    return published, failed


def softmax_normalize(strengths, temperature=1.0):
    """Convert tournament_strength values into a probability distribution.

    Uses a temperature-scaled softmax. Temperature=1.0 is the raw softmax;
    higher T flattens, lower T sharpens. Returns dict of team_id -> prob.
    """
    if not strengths:
        return {}
    vals = [s / temperature for s in strengths.values()]
    m = max(vals)
    exps = [math.exp(v - m) for v in vals]
    z = sum(exps)
    return {tid: e / z for tid, e in zip(strengths.keys(), exps)}


def main():
    args = parse_args()
    if not args.api_key:
        print("ERROR: set FERMI_API_KEY or ABW_API_KEY", file=sys.stderr)
        sys.exit(1)
    if not Path(args.binary).exists():
        print(f"ERROR: binary not found: {args.binary}\n"
              f"Build with: cargo build --bin initialize-workspace", file=sys.stderr)
        sys.exit(1)
    if not Path(args.template).exists():
        print(f"ERROR: template not found: {args.template}", file=sys.stderr)
        sys.exit(1)

    with open(args.map) as f:
        ws_map = json.load(f)
    team_priors = ws_map.get("team_priors", {})

    only = set(s.strip() for s in args.only.split(",") if s.strip()) if args.only else None
    if only:
        team_priors = {tid: wid for tid, wid in team_priors.items() if tid in only}

    print(f"API: {args.api_url}")
    print(f"Template: {args.template}")
    print(f"Binary: {args.binary}")
    print(f"Teams to initialize: {len(team_priors)}{'  (dry-run)' if args.dry_run else ''}")
    print()

    # ── Pass 1: per-team simulation + publish ───────────────────────
    strengths = {}  # team_id -> tournament_strength.mean
    summary_rows = []

    for i, (team_id, ws_id) in enumerate(sorted(team_priors.items()), 1):
        prefix = f"[{i:>2}/{len(team_priors)}] {team_id}"
        try:
            params = get_params(args.api_url, args.api_key, ws_id)
            if params is None:
                print(f"{prefix}  → no params output found, skipping")
                continue

            outputs = run_simulation(
                args.binary, args.template, params,
                args.iterations, args.seed,
            )
            ts = outputs.get("tournament_strength", {})
            mean = ts.get("mean")
            corr = (outputs.get("factor_orthogonality") or {}).get("max_abs_corr")

            published, failed = publish_outputs(
                args.api_url, args.api_key, ws_id, outputs,
                dry_run=args.dry_run,
            )
            if mean is not None:
                strengths[team_id] = mean
            summary_rows.append((team_id, mean, corr, len(published), len(failed)))

            msg = f"{prefix}  mean={mean:.3f}  corr={corr:.4f}  pub={len(published)}"
            if failed:
                msg += f"  FAIL={len(failed)}"
            print(msg)
            for key, err in failed:
                print(f"    × {key}: {err}")
        except subprocess.CalledProcessError as e:
            print(f"{prefix}  SIM-FAILED: {e.stderr.strip()[:200]}")
        except requests.RequestException as e:
            print(f"{prefix}  HTTP-FAILED: {e}")
        except Exception as e:
            print(f"{prefix}  ERROR: {e}")

    # ── Pass 2: softmax-normalized win probabilities ────────────────
    if strengths and not args.dry_run:
        print()
        print(f"Pass 2: normalizing {len(strengths)} strengths → softmax win probabilities")
        # Softmax temperature is the mapping from tournament_strength →
        # normalized cross-team probability. It is NOT fitted to any outside
        # distribution; Polymarket is a divergence reference (separate
        # `pm_*` outputs), not a target. T is a frozen scoring-head constant
        # — BayesOps updates the underlying elasticities, not T.
        TEMP = 0.10
        probs = softmax_normalize(strengths, temperature=TEMP)
        top = sorted(probs.items(), key=lambda kv: -kv[1])[:10]
        print(f"  Top 10:  " + ", ".join(f"{t}={p*100:.1f}%" for t, p in top))

        published = 0
        for team_id, prob in probs.items():
            ws_id = team_priors[team_id]
            value = {
                "value": prob,
                "method": "softmax(tournament_strength)",
                "temperature": TEMP,
                "n_teams": len(strengths),
            }
            try:
                put_output(args.api_url, args.api_key, ws_id, "predicted_probability", value)
                published += 1
            except requests.RequestException as e:
                print(f"  × {team_id}: {e}")
        print(f"  Published predicted_probability for {published}/{len(probs)} teams")

    # ── Summary ─────────────────────────────────────────────────────
    print()
    print("=" * 60)
    if summary_rows:
        sorted_summary = sorted(summary_rows, key=lambda r: -(r[1] or 0))
        print(f"{'rank':>4}  {'team':<6}  {'strength':>10}  {'corr_max':>10}  {'published':>9}")
        for rank, (team_id, mean, corr, npub, nfail) in enumerate(sorted_summary, 1):
            mean_s = f"{mean:.3f}" if mean is not None else "—"
            corr_s = f"{corr:.4f}" if corr is not None else "—"
            print(f"{rank:>4}  {team_id:<6}  {mean_s:>10}  {corr_s:>10}  {npub:>9}")
    print()
    print(f"Done. {len(strengths)} teams initialized.")


if __name__ == "__main__":
    main()
