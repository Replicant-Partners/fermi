#!/usr/bin/env python3
"""Has the deployed agent drifted from its card on disk?

    python3 scripts/agent_card_drift.py carbon_accountant
    python3 scripts/agent_card_drift.py --all

Reads `DATABASE_URL` from the environment or `.env`. Read-only.

## Why this exists

Agent cards reach the database exactly once: `seed_agents_to_database` runs at
api-server **startup**, reads `agents/curated/` from whatever filesystem that
server has, and upserts. There is no reseed endpoint. So "is the deployed agent
running my card?" is a real question with no obvious way to ask it, and the
obvious improvisation is to grep the stored prompt for a word you expect.

That improvisation is what produced this script. Looking for `corroborat` in
`agents.system_prompt` returned false, and the conclusion drawn was "production
is running a stale card". The card was byte-identical. The word was never in
the prompt — the corroboration instruction lives in the handler's QUERY, where
output-shape wording has to live, because `structured_output_trigger` removes
every tool from an agent whose prompt talks like a JSON contract.

A probe for one token cannot tell "the field changed" from "the token was never
there". This compares the fields `MemoryStore::upsert_agent` actually refreshes,
which is the set that can drift, and says which.

## The one comparison that needs care

`model_ladder` round-trips through a struct with `params`, `eval_score` and
`benchmarked_at`, so the stored JSON has keys the card never wrote. Comparing
raw would report permanent drift on every agent. Null-valued keys absent from
the card are dropped before comparing — a real difference in a rung's model or
tier still shows.
"""
import json
import os
import subprocess
import sys

# (card path, db column). Mirrors the DO UPDATE list in upsert_agent; fields it
# does not refresh (user_id, fork_count, …) are deliberately absent, because
# drift there is not the card's to fix.
FIELDS = [
    (("version",),                        "version"),
    (("metadata", "description"),         "description"),
    (("capabilities", "model"),           "model"),
    (("capabilities", "temperature"),     "temperature"),
    (("capabilities", "min_tier"),        "min_tier"),
    (("prompt_template",),                "prompt_template"),
    (("system_prompt",),                  "system_prompt"),
    (("metadata", "tags"),                "tags"),
    (("accepts",),                        "accepts"),
    (("produces",),                       "produces"),
    (("metadata", "sample_queries"),      "sample_queries"),
    (("metadata", "valence"),             "valence"),
    (("capabilities", "output_contract"), "output_contract"),
    (("capabilities", "model_ladder"),    "model_ladder"),
    (("capabilities", "model_params"),    "model_params"),
    (("capabilities", "capability_gates"), "capability_gates"),
    (("requires_secrets",),               "requires_secrets"),
]


def dig(doc, path):
    for k in path:
        if not isinstance(doc, dict) or k not in doc:
            return None
        doc = doc[k]
    return doc


def canon(v):
    """Card value and stored value, reduced to what a difference would MEAN.

    Two normalisations, both of which reported drift on identical agents
    before they were added:

    - Null-valued keys the DB round-trip adds and the card never wrote.
      `model_ladder` gains `params`, `eval_score` and `benchmarked_at`, so
      every agent with a ladder looked permanently drifted.
    - Integer/float spelling. `temperature: 0.0` in a card comes back from
      `row_to_json` as `0`, and dumping both to JSON made "0.0" != "0" —
      `coherence_evaluator` reported as drifted on a value that is equal in
      every sense anyone cares about. Bools are excluded explicitly, because
      `isinstance(True, int)` is true in Python and `1.0` is not a tag.

    A drift report that fires on identical input is worse than none: it gets
    skimmed, and then the real one is skimmed with it.
    """
    if isinstance(v, dict):
        return {k: canon(x) for k, x in v.items() if x is not None}
    if isinstance(v, list):
        return [canon(x) for x in v]
    if isinstance(v, bool):
        return v
    if isinstance(v, int):
        return float(v)
    return v


def norm(v):
    if v is None or v == [] or v == {}:
        return None
    return json.dumps(canon(v), sort_keys=True, ensure_ascii=False)


def db_url():
    url = os.environ.get("DATABASE_URL")
    if url:
        return url
    for line in open(".env", encoding="utf-8"):
        if line.startswith("DATABASE_URL="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    sys.exit("no DATABASE_URL in the environment or .env")


def rows_for(url, names):
    """Every requested agent in ONE round trip, keyed by name.

    Was one connection per agent, which made `--all` take longer than anyone
    would wait against a serverless Postgres and so made the sweep — the mode
    that finds drift nobody is looking for — effectively unavailable. A tool
    that times out is a tool nobody runs.
    """
    cols = ", ".join(c for _, c in FIELDS)
    lit = ", ".join("'" + n.replace("'", "''") + "'" for n in names)
    out = subprocess.run(
        ["psql", url, "-Atq", "-c",
         f"SELECT row_to_json(t) FROM (SELECT agent_name, {cols} FROM agents "
         f"WHERE agent_name IN ({lit})) t;"],
        capture_output=True, text=True, timeout=300,
        env={**os.environ, "PGCONNECT_TIMEOUT": "25"},
    )
    if out.returncode != 0:
        sys.exit(f"psql: {out.stderr.strip()}")
    rows = {}
    for line in out.stdout.splitlines():
        if line.strip():
            r = json.loads(line)
            rows[r["agent_name"]] = r
    return rows


def check(rows, name):
    path = f"agents/curated/{name}/agent_card.json"
    if not os.path.exists(path):
        print(f"{name}: no card at {path}")
        return None
    card = json.load(open(path, encoding="utf-8"))
    row = rows.get(name)
    if row is None:
        print(f"{name}: NOT IN THE DATABASE — it has never been seeded. The "
              f"api-server seeds at startup; until it restarts with this card "
              f"on its filesystem, the agent does not exist at runtime.")
        return ["<absent>"]
    drift = [col for p, col in FIELDS if norm(dig(card, p)) != norm(row.get(col))]
    if drift:
        print(f"{name}: DRIFTED — {', '.join(drift)}")
        for col in drift:
            p = next(p for p, c in FIELDS if c == col)
            d, b = norm(dig(card, p)), norm(row.get(col))
            cut = lambda s: (s[:160] + "…") if s and len(s) > 160 else s
            print(f"    card: {cut(d)}")
            print(f"    db:   {cut(b)}")
    else:
        print(f"{name}: current — all {len(FIELDS)} seeded fields match the card")
    return drift


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    url = db_url()
    names = args
    if "--all" in sys.argv:
        names = sorted(
            d for d in os.listdir("agents/curated")
            if os.path.exists(f"agents/curated/{d}/agent_card.json")
        )
    if not names:
        sys.exit("usage: agent_card_drift.py <agent_name>... | --all")
    rows = rows_for(url, names)
    drifted = [n for n in names if check(rows, n)]
    if len(names) > 1:
        print(f"\n  {len(names) - len(drifted)} current, {len(drifted)} drifted"
              + (f": {', '.join(drifted)}" if drifted else ""))
    sys.exit(1 if drifted else 0)
