# {{NAME}}

An ABW App.

## Quick reference

```bash
# Validate the manifest locally (no network)
abw app validate

# Deploy: register/update this App on ABW
abw app deploy

# Spawn a workspace from the deployed App
abw app spawn {{SLUG}}
```

## What's in this directory

- `manifest.json` — the App manifest. This is what gets registered.
- `.env.example` — env vars used by the CLI; copy to `.env` for local overrides.
- `README.md` — this file.

## Customising the App

Edit `manifest.json`:

- `name` / `tagline` / `description` — how your App appears in the catalogue
- `workspace_template.auto_hire` — agents auto-hired when a user spawns a workspace
- `workspace_template.initial_budget` — credits granted on spawn (typically 50–300)
- `workspace_template.initial_files` — files written into each new workspace
- `schema_json` — optional JSON Schema for the canonical document
- `composition_slug` — optional composition pattern to attach
- `visibility` — `private` (default), `unlisted`, or `public`

Run `abw app validate` after every change to catch issues early.

## Working with Xaman Ek

The platform's navigator agent (`xaman_ek`) reads this App's `description`,
`tagline`, and registered fleet. To improve discoverability, write a clear
description and pick agents whose tags match your domain.

```bash
# Ask Xaman Ek which agents fit
curl -X POST $ABW_BASE_URL/api/agents/xaman_ek/execute \
  -H "Authorization: Bearer $ABW_API_TOKEN" \
  -d '{"query": "What agents would suit a {{SLUG}} App?"}'
```

## Next steps

- Run `abw app validate` to see what fields the platform recommends filling in
- Run `abw app deploy` to register the App
- Visit `$ABW_BASE_URL/apps/{{SLUG}}` to see the catalogue page
