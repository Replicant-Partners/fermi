#!/usr/bin/env python3
"""
WC 2026 — Backfill match observations into team-prior workspaces.

For every fixture in FIXTURES below, this writes a synthetic resolution
upstream of both teams' team-prior workspaces. The Spec 23 R-1 refit
hook treats each resolution outcome as an observation source via the
declared extractors on the learnable drivers (won_rate,
form_signal).

Strategy:
  We don't have h2h match workspaces. So we write observations directly
  to the team-prior workspace's `observations` output. Per the R-1
  refit hook's data-collection logic (refit.rs collect_observations,
  Source 1), this is the preferred over walking upstream resolutions
  when present.

Each fixture produces TWO observation records, one for each team.

Run:
    python3 backfill_observations.py [--workspace TEAM_ID]
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

# ═══════════════════════════════════════════════════════════════════
# Played fixtures (from the provided fixture list, June 11–16 2026)
# Each entry: (date, group, home_team_id, away_team_id, home_goals, away_goals)
# ═══════════════════════════════════════════════════════════════════

FIXTURES = [
    # June 11
    ("2026-06-11", "A", "MEX", "ZAF", 2, 0),
    ("2026-06-11", "A", "KOR", "CZE", 2, 1),
    # June 12
    ("2026-06-12", "B", "CAN", "BIH", 1, 1),
    ("2026-06-12", "D", "USA", "PRY", 4, 1),
    # June 13
    ("2026-06-13", "B", "QAT", "CHE", 1, 1),
    ("2026-06-13", "C", "BRA", "MAR", 1, 1),
    ("2026-06-13", "C", "HAI", "SCO", 0, 1),
    ("2026-06-13", "D", "AUS", "TUR", 2, 0),
    # June 14
    ("2026-06-14", "E", "CIV", "ECU", 1, 0),
    ("2026-06-14", "E", "GER", "CUW", 7, 1),
    ("2026-06-14", "F", "NED", "JPN", 2, 2),
    ("2026-06-14", "F", "SWE", "TUN", 5, 1),
    # June 15
    ("2026-06-15", "H", "SAU", "URY", 1, 1),
    ("2026-06-15", "H", "ESP", "CPV", 0, 0),
    ("2026-06-15", "G", "IRN", "NZL", 2, 2),
    ("2026-06-15", "G", "BEL", "EGY", 1, 1),
    # June 16
    ("2026-06-16", "I", "FRA", "SEN", 3, 1),
    ("2026-06-16", "I", "IRQ", "NOR", 1, 4),
    ("2026-06-16", "J", "ARG", "DZA", 3, 0),
    ("2026-06-16", "J", "AUT", "JOR", 3, 1),
]


def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def per_team_observation(date, group, home, away, h_goals, a_goals, perspective):
    """Build a single observation outcome from this team's perspective.

    `perspective` is the team_id the observation belongs to. Returns the
    JSON object that will be appended to the team's observations array.
    """
    assert perspective in (home, away)
    if perspective == home:
        own_goals, opp_goals, opponent = h_goals, a_goals, away
    else:
        own_goals, opp_goals, opponent = a_goals, h_goals, home

    if own_goals > opp_goals:
        result = "won"
    elif own_goals < opp_goals:
        result = "lost"
    else:
        result = "drew"

    return {
        "date": date,
        "group": group,
        "match_id": f"{date}_{home}_{away}",
        "opponent_team_id": opponent,
        # The fields below are what the FPL's learnable-driver extractors
        # look up. binary_winner_id_match reads `winner_team_id`;
        # scalar_field_value with path "goal_differential" reads the
        # signed differential.
        "winner_team_id": home if h_goals > a_goals
                         else (away if a_goals > h_goals else None),
        "goal_differential": own_goals - opp_goals,
        "own_goals": own_goals,
        "opp_goals": opp_goals,
        "result": result,
        "venue_group": group,
    }


def get_existing_observations(workspace_id):
    """Read current observations output; return {} if not present."""
    try:
        resp = requests.get(
            f"{API_URL}/api/workspaces/{workspace_id}/outputs/observations",
            headers=headers(),
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException:
        return {}
    if resp.status_code == 404:
        return {}
    if resp.status_code != 200:
        print(f"    WARN: GET observations returned {resp.status_code}: {resp.text[:120]}")
        return {}
    data = resp.json()
    return data.get("value", {}) or {}


def write_observations(workspace_id, observations):
    """PUT the observations output (full overwrite of the merged structure)."""
    resp = requests.put(
        f"{API_URL}/api/workspaces/{workspace_id}/outputs/observations",
        headers=headers(),
        json={"value": observations},
        timeout=TIMEOUT,
    )
    if resp.status_code not in (200, 201):
        return False, f"HTTP {resp.status_code}: {resp.text[:160]}"
    return True, None


def backfill_team(team_id, workspace_id, dry_run=False):
    """For one team, append every fixture they played to their observations.

    The refit hook (src/handlers/workspace/refit.rs::read_observations_array)
    reads observations at the top level of `workspace_outputs[ws].observations`
    as `{ <driver_name>: [f64, ...] }` — a flat per-driver array.

    For the WC team_prior FPL we ship two learnable drivers:
      - won_rate     → 1.0 on win, 0.0 on loss/draw (binary winner match)
      - form_signal  → signed goal differential per match

    We also keep a `matches` array of full per-match records alongside, both
    for auditing and as raw material for any future driver extractors that
    want to read structured outcomes.
    """
    own_fixtures = [
        f for f in FIXTURES
        if f[2] == team_id or f[3] == team_id
    ]
    if not own_fixtures:
        print(f"  {team_id}: no fixtures played; skipping.")
        return 0

    # Build the per-team observation list, then merge into existing.
    existing = get_existing_observations(workspace_id) if not dry_run else {}
    matches_existing = existing.get("matches", []) or []

    new_matches = []
    seen_ids = {m.get("match_id") for m in matches_existing if isinstance(m, dict)}

    for fx in own_fixtures:
        obs = per_team_observation(*fx, perspective=team_id)
        if obs["match_id"] in seen_ids:
            continue
        new_matches.append(obs)

    if not new_matches:
        print(f"  {team_id}: all {len(own_fixtures)} fixtures already recorded.")
        return 0

    # Build the merged observations object:
    #   - top-level `won_rate` / `form_signal` flat arrays (what refit reads)
    #   - top-level `matches` array of full records (for audit / extractors)
    merged = dict(existing)
    all_matches = matches_existing + new_matches
    merged["matches"] = all_matches

    # Recompute per-driver arrays from the full matches list so re-runs
    # are idempotent and consistent even if a previous run left a
    # stale per-driver array behind.
    merged["won_rate"] = [
        1.0 if m.get("result") == "won" else 0.0
        for m in all_matches
    ]
    merged["form_signal"] = [
        float(m.get("goal_differential", 0))
        for m in all_matches
    ]

    if dry_run:
        print(f"  {team_id}: would append {len(new_matches)} obs (dry-run)")
        for m in new_matches:
            print(f"    {m['date']} vs {m['opponent_team_id']}: "
                  f"{m['result']} ({m['own_goals']}–{m['opp_goals']}, "
                  f"diff {m['goal_differential']:+d})")
        print(f"    → won_rate     = {merged['won_rate']}")
        print(f"    → form_signal  = {merged['form_signal']}")
        return len(new_matches)

    ok, err = write_observations(workspace_id, merged)
    if ok:
        print(
            f"  {team_id}: wrote {len(new_matches)} obs ({len(merged['matches'])} total, "
            f"won_rate={merged['won_rate']}, form_signal={merged['form_signal']})"
        )
    else:
        print(f"  {team_id}: FAILED — {err}")
    return len(new_matches) if ok else 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workspace",
        help="Limit to a single team (e.g. ARG)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be written without making changes",
    )
    args = parser.parse_args()

    if not API_KEY:
        print("ERROR: Set FERMI_API_KEY or ABW_API_KEY")
        sys.exit(1)

    map_path = os.path.join(os.path.dirname(__file__), "workspace_map.json")
    if not os.path.exists(map_path):
        print(f"ERROR: workspace_map.json not found at {map_path}")
        print("Run respawn_aligned.py first.")
        sys.exit(1)

    with open(map_path) as f:
        ws_map = json.load(f)
    team_priors = ws_map.get("team_priors", {})
    if not team_priors:
        print("ERROR: workspace_map.json has no team_priors entry.")
        sys.exit(1)

    if args.workspace:
        if args.workspace not in team_priors:
            print(f"ERROR: {args.workspace} not in workspace_map.")
            sys.exit(1)
        targets = [(args.workspace, team_priors[args.workspace])]
    else:
        targets = sorted(team_priors.items())

    print(f"Backfilling {len(targets)} workspace(s) with {len(FIXTURES)} fixtures.")
    if args.dry_run:
        print("(DRY RUN — no changes will be made)")
    print()

    total_written = 0
    teams_with_fixtures = 0
    for team_id, ws_id in targets:
        n = backfill_team(team_id, ws_id, dry_run=args.dry_run)
        if n > 0:
            teams_with_fixtures += 1
            total_written += n
        if not args.dry_run:
            time.sleep(0.05)

    print()
    print(f"Summary: {total_written} observations written across {teams_with_fixtures} teams.")
    if not args.dry_run and total_written > 0:
        print()
        print("Next: trigger refit on a team to see the loop fire:")
        first_team = next(iter(t for t, _ in targets if backfill_team), "ARG")
        print(f"  curl -X POST '{API_URL}/api/workspaces/{team_priors.get(first_team, '<id>')}/refit' \\\\")
        print(f"    -H 'Authorization: Bearer $FERMI_API_KEY'")


if __name__ == "__main__":
    main()
