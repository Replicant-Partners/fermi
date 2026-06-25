#!/usr/bin/env python3
"""
Configures Kilo Code with kilocode + anthropic profiles,
removes the dead OpenRouter key, and updates Zed's settings
with the new Anthropic key.
"""

import json
import os
import re

KILO_TOKEN = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJlbnYiOiJwcm9kdWN0aW9uIiwia2lsb1VzZXJJZCI6ImUzNTIwZjk5LTU3YWUtNGQwNi04NjIwLWEzYWZkMDExZThkNSIsImFwaVRva2VuUGVwcGVyIjpudWxsLCJ2ZXJzaW9uIjozLCJpYXQiOjE3NzgwMjAxNTAsImV4cCI6MTkzNTcwMDE1MH0.ZnFLCvxzJBgj6ePELAoZnlGHOtnJagW0qE9yytThLD8"
ANT_KEY = "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"

HOME = os.path.expanduser("~")
KILO_DIR = os.path.join(HOME, ".kilocode", "cli", "global")
SECRETS = os.path.join(KILO_DIR, "secrets.json")
STATE = os.path.join(KILO_DIR, "global-state.json")
CLI_CFG = os.path.join(HOME, ".kilocode", "cli", "config.json")
ZED_CFG = os.path.join(HOME, ".config", "zed", "settings.json")

# ── 1. secrets.json ──────────────────────────────────────────────────────────
inner = {
    "currentApiConfigName": "kilocode",
    "apiConfigs": {
        "kilocode": {
            "apiProvider": "kilocode",
            "kilocodeToken": KILO_TOKEN,
            "id": "kilo-001",
        },
        "anthropic": {
            "apiProvider": "anthropic",
            "apiKey": ANT_KEY,
            "apiModelId": "claude-sonnet-4-5",
            "id": "anth-001",
        },
    },
    "modeApiConfigs": {
        "architect": "kilo-001",
        "code": "kilo-001",
        "ask": "kilo-001",
        "debug": "kilo-001",
        "orchestrator": "kilo-001",
    },
    "migrations": {
        "rateLimitSecondsMigrated": True,
        "diffSettingsMigrated": True,
        "openAiHeadersMigrated": True,
        "consecutiveMistakeLimitMigrated": True,
        "todoListEnabledMigrated": True,
        "morphApiKeyMigrated": True,
    },
}

secrets = {
    "roo_cline_config_api_config": json.dumps(inner, indent=2),
    "kilocodeToken": KILO_TOKEN,
}

with open(SECRETS, "w") as f:
    json.dump(secrets, f, indent=2)
print(f"  [OK] {SECRETS}")

# ── 2. global-state.json ─────────────────────────────────────────────────────
with open(STATE) as f:
    state = json.load(f)

state["currentApiConfigName"] = "kilocode"
state["apiProvider"] = "kilocode"
state.pop("openRouterModelId", None)
state["listApiConfigMeta"] = [
    {"name": "kilocode", "id": "kilo-001", "apiProvider": "kilocode", "modelId": ""},
    {
        "name": "anthropic",
        "id": "anth-001",
        "apiProvider": "anthropic",
        "modelId": "claude-sonnet-4-5",
    },
]

with open(STATE, "w") as f:
    json.dump(state, f, indent=2)
print(f"  [OK] {STATE}")

# ── 3. config.json (CLI) ──────────────────────────────────────────────────────
with open(CLI_CFG) as f:
    cli = json.load(f)

cli["provider"] = "kilocode"
cli["providers"] = [
    {"id": "kilocode", "provider": "kilocode", "kilocodeToken": KILO_TOKEN},
    {
        "id": "anthropic",
        "provider": "anthropic",
        "apiKey": ANT_KEY,
        "apiModelId": "claude-sonnet-4-5",
    },
]

with open(CLI_CFG, "w") as f:
    json.dump(cli, f, indent=2)
print(f"  [OK] {CLI_CFG}")

# ── 4. settings.json (Zed) — swap the old Anthropic key ──────────────────────
OLD_ANT = "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"

with open(ZED_CFG) as f:
    zed_raw = f.read()

if OLD_ANT in zed_raw:
    zed_raw = zed_raw.replace(OLD_ANT, ANT_KEY)
    with open(ZED_CFG, "w") as f:
        f.write(zed_raw)
    print(f"  [OK] {ZED_CFG} (Anthropic key updated)")
else:
    print(f"  [--] {ZED_CFG} (old key not found — check manually)")

print("\nDone. Restart Zed for changes to take effect.")
