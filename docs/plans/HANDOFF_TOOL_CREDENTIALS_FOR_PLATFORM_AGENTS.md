# Handoff — a platform-tier agent cannot reach a tool credential

**Status:** open. Blocking nothing today; silently degrading one shipped
feature and structurally blocking any future one that needs a third-party
tool key on a curated or system agent.

**Found while:** wiring `POST /api/workspaces/:id/actions/evaluate_claims`
(the DPP Studio claim evaluator, commits `b8dffc71` and `3f663667`). That
endpoint needs `web_search`, `web_search` needs a Brave key, and
`regulatory_lens_translator` is `tier: curated`. The key could not be reached
through the credential model, so the endpoint reads env as a documented
compromise. This document is the work required to remove that compromise.

**Owner:** whoever owns the credential subsystem
(`docs/specs/AGENT_CREDENTIAL_MODEL.md`, mig-171, `fermi-auth/src/secrets.rs`).
Deliberately not done inside the DPP feature — see §7.

---

## 1. The defect in one paragraph

`docs/specs/AGENT_CREDENTIAL_MODEL.md` §2 says a credential is
`(owning_principal, provider, scope)` in an encrypted store, and — verbatim —
**"Never env vars."** For LLM provider keys that holds: mig-171 built
`agent_credentials`, `abw-system` is the owning principal for platform-service
agents, and `api_server.rs:2560` seeds the store from env once at startup as a
bootstrap. For **tool** credentials it does not hold. mig-171 explicitly left
tool secrets in `user_secrets`, and `user_secrets` is keyed by a human owner.
A curated or system agent has no human owner, so
`resolve_agent_owner_secrets` returns `None` for those tiers *by design*
(`api_server.rs:6510-6522`). The result: **there is no store path from which a
platform-tier agent can obtain a tool key**, so every tool that needs one
reads `std::env::var` at call time.

Two consequences visible in the tree today:

- `web_search` reads `BRAVE_SEARCH_API_KEY` from env
  (`src/agent_backend/tools/domains/platform.rs:1077`) and, when unset,
  returns its own error message *as the tool result* rather than raising. The
  model then answers from training data and the run succeeds.
- `do_fmp_api` ships a **hardcoded fallback API key** in source
  (`src/agent_backend/tools/domains/financial.rs:459`). That is the same gap
  with a worse mitigation, and it should be treated as part of this work.

---

## 2. The mechanism that already exists and cannot be used

`PlatformTool::required_credential() -> Option<&'static str>` plus the guard
at `src/agent_backend/tools/registry.rs:79-88`:

```rust
if let Some(key) = tool.required_credential() {
    if ctx.user_secrets.as_ref().and_then(|s| s.get(key)).is_none() {
        return Err(format!(
            "Tool `{tool_name}` requires credential `{key}` \
             which is not configured for this agent."
        ));
    }
}
```

This is the right shape: refuse before executing, name the missing key. Nine
tools declare it, all in `financial.rs`, all `FMP_API_KEY`.

It cannot be applied to `web_search`. The guard reads `ctx.user_secrets`, and
that map is `None` on every path a platform-tier agent executes on (§3).
Declaring `required_credential()` on `web_search` today would refuse the tool
for every curated agent that searches — which is most of the research fleet.

**Note the nine existing declarations are already only half-wired**: the guard
checks `user_secrets`, but `do_fmp_api` then reads the key from env and
ignores `ctx` entirely (its parameter is `_ctx`). So the guard gates on one
source while execution reads another. Fixing that is the same fix.

---

## 3. Verified inventory

13 `ToolContext` construction sites in `src/`, exhibiting **four** different
behaviours for the same field. Counted 2026-09-10.

| Site | `user_secrets` |
|---|---|
| `handlers/execution.rs:294` | `owner_secrets` |
| `handlers/execution_stream.rs:194` | `resolve_agent_owner_secrets(..)` |
| `handlers/mcp.rs:321` | `resolve_agent_owner_secrets(..)` |
| `handlers/a2a.rs:372` | `owner_secrets` |
| `handlers/a2a.rs:722` | `owner_secrets` |
| `handlers/workspace/messages.rs:506` | `get_secrets_for_agent(db, enc, **invoking user_id**, agent_name)` |
| `tools/domains/platform.rs:728` | `ctx.user_secrets.clone()` — propagates |
| `tools/domains/platform.rs:443` | `None` |
| `handlers/rabble_workspace.rs:322` | `None` |
| `handlers/creatures/mod.rs:370` | `None` |
| `handlers/eval.rs:570` | `None` |
| `handlers/mcp.rs:441` | `None` |
| `handlers/workspace/coherence.rs:653` | `None` |

Four behaviours:

1. **Owner-keyed** — `resolve_agent_owner_secrets`, which returns `None` for
   platform tiers. 5 sites.
2. **Invoker-keyed** — `messages.rs:432` resolves against the *calling user*,
   not the agent owner. A different security model from (1), on the main chat
   path, and the difference is not documented anywhere.
3. **`None`** — 6 sites, all genuine agent-execution paths: the delegation
   child context, creature dispatch, the eval loop, an MCP tool call, the
   coherence strategist, and the rabble/workspace dispatcher used by
   `evaluate_claims`.
4. **Propagate** — `platform.rs:728`.

**(3) and (4) are the sharpest finding.** `platform.rs:713/728` and
`platform.rs:428/443` are both delegation-child contexts, and one propagates
the parent's secrets while the other passes `None`. A delegated child's access
to tool credentials therefore depends on which delegation tool called it.
Whichever is correct, they should not disagree.

### 3.1 A second, unrelated "required secrets" system

`agents.requires_secrets` (mig-040, `AgentCard::requires_secrets`,
`agent_card.rs:66`) is checked at `messages.rs:443` to refuse an invocation
with a named missing secret. It is a **different mechanism** from
`PlatformTool::required_credential()`, keyed on the *agent card* rather than
the *tool*, enforced on *one* path rather than in the registry, and the two do
not consult each other. 18 references across `src/`.

Deciding whether these converge is part of this work. An agent card declaring
`requires_secrets: [BRAVE_SEARCH_API_KEY]` and a tool declaring
`required_credential("BRAVE_SEARCH_API_KEY")` are the same fact in two places,
and today only one of them is checked on any given path.

---

## 4. Two designs

### Option A — encryptor on `ToolContext`, lazy resolution in the tool

Add `secret_encryptor: Option<Arc<SecretEncryptor>>` to `ToolContext`,
populate it at all 13 sites, and have each tool resolve its own key at call
time.

The `fermi` lib already depends on `fermi-auth` (`Cargo.toml:144`; tools
already call `fermi_auth::get_or_create_wallet` from
`tools/domains/rabble.rs:361` and elsewhere), so this is **not** blocked by
dependency direction. An earlier version of this note claimed it was; that was
wrong.

- **Against:** a DB round-trip plus a decrypt on every tool call, or a new
  cache to avoid it. Contradicts SPEC_28, which pre-resolves credentials
  precisely so that no async credential work happens mid-execution. Puts
  decryption in the widest possible surface — 100+ tools.
- **For:** a tool never called costs nothing.

### Option B — eager resolution into `user_secrets` (recommended)

Extend the resolver so that a platform-tier agent's tool credentials come from
its funding principal's store, and keep resolution where it already happens:
once, up front, per execution.

- Mirrors `ResolvedCredentials` (SPEC_28) exactly, which solved this same
  problem for LLM provider keys.
- No `ToolContext` schema change; the lib stays ignorant of *which* secrets
  exist and only consumes a map.
- The 5 owner-keyed sites get it for free once the resolver changes.
- Unblocks `required_credential("BRAVE_SEARCH_API_KEY")` on `web_search`,
  which is the actual goal — a named refusal instead of a silent degrade.
- **Against:** resolves keys an execution may not use. Cheap: one indexed
  query per execution, already paid for LLM credentials.

**Recommendation: Option B.**

---

## 5. Requirements

**R1 — A platform-tier agent resolves tool credentials from its funding
principal's store.** Extend `resolve_agent_owner_secrets`
(`api_server.rs:6510`) or add a sibling so that when
`is_platform_funded(tier)`, tool credentials are read from
`(funding_principal_for(agent), provider, scope)` via
`fermi_auth::resolve_agent_credential`. Keys must be returned under the names
`required_credential()` returns (`BRAVE_SEARCH_API_KEY`, `FMP_API_KEY`), since
that is what the guard looks up.

Preserve the existing distinction: `None` means "no store applies, tools may
fall back"; `Some(empty)` means "a store applies and the key is absent", which
the guard must report as missing. Collapsing those re-introduces the v0.9.0
soft-fallback that v0.9.2 deliberately removed.

**R2 — Enumerate tool-credential providers in one place.** Both
`api_server.rs`'s bootstrap list and any resolver need to know which providers
are tool credentials rather than LLM providers. Today the bootstrap list is a
literal array and `("BRAVE_SEARCH_API_KEY", "brave_search")` was appended to
it by `3f663667`. One table, consumed by both.

**R3 — Every `ToolContext` site states `user_secrets` deliberately, and the
six `None`s are resolved.** For each: either populate it, or state in a
comment why this path has no agent whose credentials could be resolved. A bare
`None` with no reason is what made this invisible.

**R4 — Reconcile the two delegation-child contexts.**
`platform.rs:443` and `platform.rs:728` must agree on whether a delegated
child inherits the parent's tool credentials. Note this is an authorization
boundary — the `remote_mcp` field's doc comment
(`tools/context.rs`) argues the opposite case for remote tools, and that
reasoning should be applied or explicitly rejected here.

**R5 — Decide whether the invoker-keyed path is intended.**
`messages.rs:432` resolves against the calling user; five other sites resolve
against the agent owner. Either is defensible; both silently is not. Document
the choice next to the resolver.

**R6 — `web_search` declares `required_credential("BRAVE_SEARCH_API_KEY")`
and reads the key from `ctx`, not env.** Only after R1 and R3, or every
curated agent that searches breaks. Same change for the nine `financial.rs`
tools, and **delete the hardcoded fallback key at `financial.rs:459`** —
that is a committed secret and should be rotated.

**R7 — Decide whether `agents.requires_secrets` and
`PlatformTool::required_credential()` converge** (§3.1), or document why two
mechanisms exist.

**R8 — Remove the env compromise in the DPP preflight.** Once R1 and R6 land,
`resolve_search_credential` in
`src/handlers/workspace/claim_evaluation.rs` should drop its env branch and
resolve from the store alone. Its docstring names this as the follow-up.

---

## 6. Tests

- **A ratchet on `None`.** A test enumerating `ToolContext` construction sites
  that fails when a new one passes `user_secrets: None` without being on a
  documented allowlist. Same shape as
  `no_curated_card_declares_a_phantom_tool` and
  `narrative_leak_coverage_only_shrinks`. Without this, R3 decays.
- **A refusal test.** With a store configured and the key absent, executing an
  agent whose tool declares `required_credential` must produce the guard's
  named error — not a successful run. Must be able to go red: assert the
  positive case too, or it passes by matching nothing.
- **A platform-tier resolution test.** A curated agent resolves a tool
  credential from `abw-system`. This is the case that has never worked.
- **An `empty` vs `None` test**, per R1.
- **A no-committed-secrets test** — grep the tool sources for high-entropy
  string literals assigned to anything named `*key*`. Would have caught
  `financial.rs:459`.

---

## 7. Why this was not done in the DPP feature

`evaluate_claims` needed one tool key. The fix touches the resolver that
funds every agent on the platform, 13 construction sites, an authorization
boundary between delegation tools, and a committed secret. That is a change to
shared credential plumbing, and a claims endpoint has no standing to make it —
the first attempt reached into `dispatch_rabble_action`, a shared executor, and
that coupling is what put the work in a file it had no business editing.

What the DPP feature does instead, and what to unwind later:

- `resolve_search_credential`
  (`src/handlers/workspace/claim_evaluation.rs`) resolves store-first,
  env-second, and refuses with a 503 if neither has the key. It returns which
  source satisfied it so the legacy path is visible rather than assumed.
- Its docstring states the asymmetry plainly: **a key present in the store but
  absent from env passes the preflight and then fails inside the tool**,
  because `web_search` reads env. That is the bug this document exists to
  close, and R8 is the line to delete.
- `3f663667` added `("BRAVE_SEARCH_API_KEY", "brave_search")` to the startup
  bootstrap so the key can live in the store at all. That is one line in an
  existing array and is the only shared-file change made.

---

## 8. Acceptance

1. A curated agent obtains a tool credential from the `abw-system` store with
   no environment variable set anywhere.
2. With the key absent, the run is **refused** by name, and no evaluation is
   written. In particular no `tool_no_match` stamp is produced, because that
   verdict asserts the corpus was searched.
3. `financial.rs:459`'s hardcoded key is deleted and rotated.
4. The `None` ratchet is green and every remaining `None` has a stated reason.
5. R8 is applied — `claim_evaluation.rs` no longer reads env.
6. `grep -rn 'std::env::var' src/agent_backend/tools/` returns nothing that is
   a credential.

---

## 9. Context

- `docs/specs/AGENT_CREDENTIAL_MODEL.md` — §2 the three axes, §3 the two
  classes, P5.3 (rename `user_secrets` → `tool_secrets`) is adjacent to R1
- mig-171 `171_agent_credentials.sql` — the store, and the sentence scoping it
  to LLM/embedding providers that created this gap
- mig-039 `039_user_secrets.sql`, mig-040 `040_agent_requires_secrets.sql`
- `api_server.rs:2560` — the sanctioned env→store bootstrap
- `tools/registry.rs:79-88` — the guard
- Commits `b8dffc71`, `3f663667`
