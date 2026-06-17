#!/usr/bin/env python3
"""
WC 2026 — Simulate a matchday's worth of results, watch the BayesOps loop fire.

This is the demo-driver script. Given a slate of fixtures (a "matchday"),
it appends each result to BOTH teams' team-prior workspace observations,
then fires the manual refit endpoint on every affected workspace. The
expected effect, visible in the console without further intervention:

  1. workspace_outputs[ws].observations grows by one match per team.
  2. The won_rate / form_signal arrays now have n ≥ 2 (variance > 0), so
     fit_marginal succeeds where it previously raised "variance is zero".
  3. Each successful fit writes a bayesops_snapshot row and posts a
     bayesops_fit_{accepted|staged|hard_blocked} message to the workspace.
     Failed fits (still possible if n is small or all-identical) post
     bayesops_fit_failed events — both kinds show in the Trajectory tab.
  4. If the impact gate's delta_pp exceeds the auto-accept threshold but
     stays under the hard-block ceiling, the console's sparkline badge
     flips to PendingReview, ready for accept/reject from the UI.
  5. forecast_updates rows are written with revision_trigger='bayesops_refit'
     when the posterior actually shifts the predicted_probability — these
     are what the Trajectory tab plots as the spacetime track.

The result: the operator runs this one command, opens the console, and
watches the trajectory move on every team that played.

Matchdays:
  --matchday 1   (default) = June 11–16, group stage MD1 (already backfilled)
  --matchday 2             = June 17–22, group stage MD2 (this demo's payload)
  --matchday 3             = June 23–28, group stage MD3
  --custom path/to.json    = load fixtures from a JSON file

Run:
    python3 scripts/world_cup/simulate_matchday.py --matchday 2
    python3 scripts/world_cup/simulate_matchday.py --matchday 2 --dry-run
    python3 scripts/world_cup/simulate_matchday.py --matchday 2 --workspace ARG
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
TIMEOUT = 60

WORKSPACE_MAP = Path(__file__).parent / "workspace_map.json"

# ═══════════════════════════════════════════════════════════════════════════
# Fixture data
# ═══════════════════════════════════════════════════════════════════════════
#
# Matchday 1 is the slate already backfilled by backfill_observations.py —
# kept here for completeness so re-running this script after a wipe is one
# command. The script's append logic is idempotent on (date, home, away).
#
# Matchday 2 is the actual demo payload — invented results that give each
# team a second observation. Scores chosen to create a mix of wins / draws /
# upsets / blowouts so the fitted posteriors actually MOVE off the prior
# (uniform Triangular(0.2, 0.5, 0.8) for won_rate, Normal(0, 1) for
# form_signal) in different directions per team.
#
# Each entry: (date, group, home_team_id, away_team_id, home_goals, away_goals)

MATCHDAY_1 = [
    ("2026-06-11", "A", "MEX", "ZAF", 2, 0),
    ("2026-06-11", "A", "KOR", "CZE", 2, 1),
    ("2026-06-12", "B", "CAN", "BIH", 1, 1),
    ("2026-06-12", "D", "USA", "PRY", 4, 1),
    ("2026-06-13", "B", "QAT", "CHE", 1, 1),
    ("2026-06-13", "C", "BRA", "MAR", 1, 1),
    ("2026-06-13", "C", "HAI", "SCO", 0, 1),
    ("2026-06-13", "D", "AUS", "TUR", 2, 0),
    ("2026-06-14", "E", "CIV", "ECU", 1, 0),
    ("2026-06-14", "E", "GER", "CUW", 7, 1),
    ("2026-06-14", "F", "NED", "JPN", 2, 2),
    ("2026-06-14", "F", "SWE", "TUN", 5, 1),
    ("2026-06-15", "H", "SAU", "URY", 1, 1),
    ("2026-06-15", "H", "ESP", "CPV", 0, 0),
    ("2026-06-15", "G", "IRN", "NZL", 2, 2),
    ("2026-06-15", "G", "BEL", "EGY", 1, 1),
    ("2026-06-16", "I", "FRA", "SEN", 3, 1),
    ("2026-06-16", "I", "IRQ", "NOR", 1, 4),
    ("2026-06-16", "J", "ARG", "DZA", 3, 0),
    ("2026-06-16", "J", "AUT", "JOR", 3, 1),
]

# Matchday 2: each MD1 team plays again. Same 4-team groups (A–J × 2 = 20
# matches). Outcomes chosen to lift the posteriors off the prior — wins for
# the heavyweights, draws/losses for the chaff. Goal diffs span [-3, +4] so
# the form_signal distribution has reasonable spread.
MATCHDAY_2 = [
    # Group A: MEX 1-1, ZAF 0-2, KOR 2-0, CZE 0-1
    ("2026-06-17", "A", "MEX", "KOR", 1, 1),
    ("2026-06-17", "A", "ZAF", "CZE", 0, 1),
    # Group B: CAN 2-0, BIH 0-3, QAT 0-2, CHE 2-1
    ("2026-06-18", "B", "CAN", "QAT", 2, 0),
    ("2026-06-18", "B", "BIH", "CHE", 0, 3),
    # Group C: BRA 4-0, MAR 1-1, HAI 0-2, SCO 2-1
    ("2026-06-18", "C", "BRA", "HAI", 4, 0),
    ("2026-06-18", "C", "MAR", "SCO", 1, 1),
    # Group D: USA 2-1, PRY 1-2, AUS 1-1, TUR 0-1
    ("2026-06-19", "D", "USA", "AUS", 2, 1),
    ("2026-06-19", "D", "PRY", "TUR", 0, 1),
    # Group E: CIV 0-2, ECU 1-0, GER 3-1, CUW 0-4
    ("2026-06-19", "E", "CIV", "GER", 0, 2),
    ("2026-06-19", "E", "ECU", "CUW", 1, 0),
    # Group F: NED 3-0, JPN 1-1, SWE 1-2, TUN 0-1
    ("2026-06-20", "F", "NED", "SWE", 3, 1),
    ("2026-06-20", "F", "JPN", "TUN", 1, 0),
    # Group G: BEL 2-0, EGY 0-1, IRN 1-1, NZL 0-2
    ("2026-06-20", "G", "BEL", "IRN", 2, 1),
    ("2026-06-20", "G", "EGY", "NZL", 0, 0),
    # Group H: ESP 3-0, CPV 0-1, SAU 0-2, URY 2-0
    ("2026-06-21", "H", "ESP", "SAU", 3, 0),
    ("2026-06-21", "H", "CPV", "URY", 0, 2),
    # Group I: FRA 2-0, SEN 0-2, IRQ 0-3, NOR 1-1
    ("2026-06-21", "I", "FRA", "IRQ", 2, 0),
    ("2026-06-21", "I", "SEN", "NOR", 0, 1),
    # Group J: ARG 3-1, DZA 1-2, AUT 1-1, JOR 0-3
    ("2026-06-22", "J", "ARG", "AUT", 3, 1),
    ("2026-06-22", "J", "DZA", "JOR", 1, 0),
]

# Matchday 3 placeholder so --matchday 3 fails loudly with a clear message
# rather than silently doing nothing.
MATCHDAY_3: list = []

MATCHDAYS = {1: MATCHDAY_1, 2: MATCHDAY_2, 3: MATCHDAY_3}


# ═══════════════════════════════════════════════════════════════════════════
# HTTP helpers
# ═══════════════════════════════════════════════════════════════════════════

def headers():
    return {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }


def per_team_observation(date, group, home, away, h_goals, a_goals, perspective):
    """Build the per-match observation record from one team's perspective.

    Mirrors backfill_observations.per_team_observation so both scripts
    produce byte-identical records for the same fixture. Don't drift these
    — the refit hook reads top-level `won_rate` / `form_signal` arrays
    derived from these records, and the `matches` audit array is what the
    Trajectory tab cross-references for context.
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
        "winner_team_id": home if h_goals > a_goals
                         else (away if a_goals > h_goals else None),
        "goal_differential": own_goals - opp_goals,
        "own_goals": own_goals,
        "opp_goals": opp_goals,
        "result": result,
        "venue_group": group,
    }


def get_observations(workspace_id):
    """Read current observations output; return {} on 404 or transient error."""
    try:
        r = requests.get(
            f"{API_URL}/api/workspaces/{workspace_id}/outputs/observations",
            headers=headers(),
            timeout=TIMEOUT,
        )
    except requests.exceptions.RequestException as e:
        print(f"    WARN: GET observations failed: {e}")
        return {}
    if r.status_code == 404:
        return {}
    if r.status_code != 200:
        print(f"    WARN: GET observations returned {r.status_code}: {r.text[:120]}")
        return {}
    return r.json().get("value", {}) or {}


def put_observations(workspace_id, observations):
    r = requests.put(
        f"{API_URL}/api/workspaces/{workspace_id}/outputs/observations",
        headers=headers(),
        json={"value": observations},
        timeout=TIMEOUT,
    )
    if r.status_code not in (200, 201):
        return False, f"HTTP {r.status_code}: {r.text[:200]}"
    return True, None


def fire_refit(workspace_id):
    """POST /api/workspaces/:id/refit. Returns parsed RefitOutcome or error str."""
    r = requests.post(
        f"{API_URL}/api/workspaces/{workspace_id}/refit",
        headers=headers(),
        json={},
        timeout=TIMEOUT,
    )
    if r.status_code != 200:
        return None, f"HTTP {r.status_code}: {r.text[:240]}"
    return r.json(), None


# ═══════════════════════════════════════════════════════════════════════════
# Per-team logic
# ═══════════════════════════════════════════════════════════════════════════

def append_matches_for_team(team_id, workspace_id, team_fixtures, dry_run):
    """Append the team's matches from this slate to its observations output.

    Idempotent on `match_id` — re-running the same matchday produces no
    duplicate observations. Per-driver arrays are always recomputed from
    the full matches list so the shape upgrades on re-run if the schema
    evolves.

    Returns: (n_new_matches, merged_observations) — merged is None on no-op
    or on dry-run.
    """
    existing = get_observations(workspace_id) if not dry_run else {}
    matches_existing = existing.get("matches", []) or []
    seen_ids = {m.get("match_id") for m in matches_existing if isinstance(m, dict)}

    new_matches = []
    for fx in team_fixtures:
        obs = per_team_observation(*fx, perspective=team_id)
        if obs["match_id"] in seen_ids:
            continue
        new_matches.append(obs)

    if not new_matches:
        return 0, None

    merged = dict(existing)
    all_matches = matches_existing + new_matches
    merged["matches"] = all_matches
    merged["won_rate"] = [
        1.0 if m.get("result") == "won" else 0.0 for m in all_matches
    ]
    merged["form_signal"] = [
        float(m.get("goal_differential", 0)) for m in all_matches
    ]

    if dry_run:
        print(f"  {team_id}: would append {len(new_matches)} match(es)")
        for m in new_matches:
            print(
                f"    {m['date']} vs {m['opponent_team_id']}: "
                f"{m['result']} ({m['own_goals']}–{m['opp_goals']}, "
                f"diff {m['goal_differential']:+d})"
            )
        print(f"    → won_rate    n={len(merged['won_rate'])}: {merged['won_rate']}")
        print(f"    → form_signal n={len(merged['form_signal'])}: {merged['form_signal']}")
        return len(new_matches), None

    ok, err = put_observations(workspace_id, merged)
    if not ok:
        print(f"  {team_id}: PUT observations FAILED — {err}")
        return 0, None

    return len(new_matches), merged


def format_refit_outcome(outcome: dict) -> str:
    """One-line summary of a RefitOutcome JSON blob."""
    if not isinstance(outcome, dict):
        return "?"
    parts = [
        f"considered={outcome.get('drivers_considered', '?')}",
        f"accepted={outcome.get('auto_accepted', 0)}",
        f"staged={outcome.get('staged', 0)}",
        f"blocked={outcome.get('hard_blocked', 0)}",
        f"skipped={outcome.get('skipped', 0)}",
    ]
    summary = ", ".join(parts)

    per_driver = outcome.get("per_driver", []) or []
    if not per_driver:
        return summary

    driver_lines = []
    for d in per_driver:
        name = d.get("driver_name", "?")
        n_obs = d.get("n_observations", 0)
        decision = d.get("decision", {})
        if isinstance(decision, dict):
            # decision is { "auto_accepted": { "delta_pp": x } } etc.
            kind = next(iter(decision.keys()), "?")
            inner = decision.get(kind, {})
            if isinstance(inner, dict) and "delta_pp" in inner:
                tag = f"{kind} Δ={inner['delta_pp']:+.2f}pp"
            elif isinstance(inner, dict) and "reason" in inner:
                tag = f"{kind} ({inner['reason'][:60]})"
            else:
                tag = kind
        else:
            tag = str(decision)
        driver_lines.append(f"      • {name} (n={n_obs}): {tag}")

    return summary + "\n" + "\n".join(driver_lines)


# ═══════════════════════════════════════════════════════════════════════════
# Main driver
# ═══════════════════════════════════════════════════════════════════════════

def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--matchday", type=int, default=2,
        help="Which matchday to play (1, 2, or 3). Default: 2.",
    )
    parser.add_argument(
        "--custom", type=str, default=None,
        help="Path to a JSON file of fixtures (overrides --matchday).",
    )
    parser.add_argument(
        "--workspace", type=str, default=None,
        help="Limit to a single team_id (e.g. ARG) — applies to both append and refit.",
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="Show what would be appended without writing or refitting.",
    )
    parser.add_argument(
        "--no-refit", action="store_true",
        help="Append observations but skip the refit step.",
    )
    parser.add_argument(
        "--sleep", type=float, default=0.1,
        help="Seconds between team operations (default: 0.1).",
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
    team_priors = ws_map.get("team_priors", {})

    # Pick fixtures.
    if args.custom:
        with open(args.custom) as f:
            raw = json.load(f)
        fixtures = [tuple(fx) for fx in raw]
        label = f"custom ({args.custom})"
    else:
        fixtures = MATCHDAYS.get(args.matchday)
        if fixtures is None:
            print(f"ERROR: unknown matchday {args.matchday}", file=sys.stderr)
            sys.exit(1)
        if not fixtures:
            print(
                f"ERROR: matchday {args.matchday} fixtures not defined yet — "
                f"add them to MATCHDAY_{args.matchday} or pass --custom.",
                file=sys.stderr,
            )
            sys.exit(1)
        label = f"matchday {args.matchday}"

    # Figure out which teams play.
    teams_playing = set()
    for fx in fixtures:
        teams_playing.add(fx[2])
        teams_playing.add(fx[3])

    if args.workspace:
        if args.workspace not in team_priors:
            print(
                f"ERROR: {args.workspace} not in workspace_map.json/team_priors.",
                file=sys.stderr,
            )
            sys.exit(1)
        if args.workspace not in teams_playing:
            print(
                f"ERROR: {args.workspace} does not play in {label}.",
                file=sys.stderr,
            )
            sys.exit(1)
        teams_playing = {args.workspace}

    print(f"Playing {label}: {len(fixtures)} fixture(s), {len(teams_playing)} team(s).")
    if args.dry_run:
        print("(DRY RUN — no observations written, no refits fired.)")
    print()

    # ── Phase 1: append observations ─────────────────────────────────────
    print("─── Appending observations ───")
    appended_per_team: dict[str, int] = {}
    for team_id in sorted(teams_playing):
        ws_id = team_priors.get(team_id)
        if not ws_id:
            print(f"  {team_id}: not in workspace_map — skipping")
            continue
        team_fx = [fx for fx in fixtures if fx[2] == team_id or fx[3] == team_id]
        n_new, _ = append_matches_for_team(team_id, ws_id, team_fx, args.dry_run)
        appended_per_team[team_id] = n_new
        if n_new == 0 and not args.dry_run:
            print(f"  {team_id}: already had every match (idempotent skip)")
        elif not args.dry_run:
            print(f"  {team_id}: +{n_new} match(es)")
        if not args.dry_run:
            time.sleep(args.sleep)
    print()

    if args.dry_run or args.no_refit:
        print("Done (no refits fired).")
        return

    # ── Phase 2: fire refit on every team that received an observation ──
    print("─── Firing BayesOps refit ───")
    refit_results: dict[str, dict] = {}
    refit_errors: dict[str, str] = {}
    for team_id in sorted(teams_playing):
        if appended_per_team.get(team_id, 0) == 0:
            print(f"  {team_id}: no new observations → skipping refit")
            continue
        ws_id = team_priors.get(team_id)
        outcome, err = fire_refit(ws_id)
        if err:
            refit_errors[team_id] = err
            print(f"  {team_id}: REFIT FAILED — {err}")
        else:
            refit_results[team_id] = outcome
            print(f"  {team_id}: {format_refit_outcome(outcome)}")
        time.sleep(args.sleep)
    print()

    # ── Summary ──────────────────────────────────────────────────────────
    n_teams_touched = sum(1 for v in appended_per_team.values() if v > 0)
    n_accepted = sum(r.get("auto_accepted", 0) for r in refit_results.values())
    n_staged = sum(r.get("staged", 0) for r in refit_results.values())
    n_blocked = sum(r.get("hard_blocked", 0) for r in refit_results.values())
    n_skipped = sum(r.get("skipped", 0) for r in refit_results.values())

    print("─── Summary ───")
    print(f"  Teams updated:         {n_teams_touched}")
    print(f"  Refits succeeded:      {len(refit_results)}")
    print(f"  Refits failed:         {len(refit_errors)}")
    print(f"  Drivers auto-accepted: {n_accepted}")
    print(f"  Drivers staged:        {n_staged}  ← shows as PendingReview badge")
    print(f"  Drivers hard-blocked:  {n_blocked}")
    print(f"  Drivers skipped:       {n_skipped}  (typically n<2 or zero-variance)")
    print()
    print("Open the console, navigate to any team that played, and check:")
    print("  • Cockpit Trajectory tab → new bayesops_refit event(s)")
    print("  • Sparkline badge color   → green = accepted, yellow = pending")
    print("  • Polymarket delta        → may shift if predicted_probability moved")


if __name__ == "__main__":
    main()
