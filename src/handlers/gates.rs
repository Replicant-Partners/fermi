//! Gates — what the platform refused, and what it cannot refuse.
//!
//! # The surface that did not exist
//!
//! `docs/AUDIT_loops_and_gates.md` §2.2 states the gap this closes:
//!
//! > **The platform has a record of every request it served and none of any it
//! > refused.**
//!
//! That is why the Γ arithmetic bug survived: the coherence gate rejected 100%
//! of agent-wide interventions for reasons unrelated to their content, and
//! nothing anywhere could show that it had ever been asked. Loop 2's only
//! structural control was unreachable and invisible at the same time.
//!
//! # Three questions, and only the first is about counts
//!
//! | block | question | source |
//! |---|---|---|
//! | Register | what has each gate decided, and what would silence mean? | [`crate::gate_trust`] |
//! | Enforcement | which verbs can a gate actually refuse? | [`crate::command_registry`] |
//! | Receipts | what did it refuse, specifically? | `gate_decisions` |
//!
//! The **middle one is the novel one**. A gate that runs and has its verdict
//! discarded is, from the caller's side, indistinguishable from no gate at all —
//! and the audit found three such verbs, including the two general-purpose
//! execute endpoints a third party actually calls. Counting decisions cannot
//! reveal that, because the count looks healthy: the gate *is* being asked, its
//! answer is simply thrown away. Only a declaration of intended enforcement,
//! compared against the call site, can say so. That is what `command_registry`
//! is for and this is where it surfaces.
//!
//! # Two honesty constraints on the numbers
//!
//! **Counters are per-process and reset on restart.** `gate_trust`'s totals are
//! `AtomicU64` statics, deliberately — *"a ledger that is itself a fallible
//! database write is most silent when it is most needed."* Every count served
//! here is therefore *since boot*, and the response says so rather than leaving
//! the page to imply a longer history.
//!
//! **Only two gates are durable.** `Retention::Recorded` covers coherence and
//! admission; the other five are counted in memory only and never reach
//! `gate_decisions` at all. A receipts list that did not say which gates can
//! appear in it would read as a complete record of refusals when it is a record
//! of two gates' refusals.

use axum::{extract::State, http::StatusCode, Json};
use fermi_auth::AuthPrincipal;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

/// `GET /api/gates`
pub async fn gates_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;
    let is_admin = principal.can_admin();

    // ── Register ─────────────────────────────────────────────────────────
    //
    // `reading` and `if_never_refuses` come straight from `gate_trust`: the
    // sentence explaining what silence would mean is authored next to the gate
    // itself, so the page cannot invent a softer one.
    let register: Vec<Value> = fermi::gate_trust::accounts()
        .into_iter()
        .map(|a| {
            json!({
                "id": a.id,
                "clock": a.clock,
                "retention": a.retention,
                "site": a.site,
                "refuses": fermi::gate_trust::GATES
                    .iter().find(|g| g.id == a.id).map(|g| g.refuses),
                "approved": a.approved,
                "refused": a.refused,
                "undetermined": a.undetermined,
                "asked": a.asked(),
                "reading": a.reading,
                "if_never_refuses": a.if_never_refuses,
                "last_refusal": a.last_refusal,
            })
        })
        .collect();

    // ── Enforcement map ──────────────────────────────────────────────────
    let commands: Vec<Value> = fermi::command_registry::COMMANDS
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "label": c.label,
                "does": c.does,
                "effect": c.effect,
                "route": c.route,
                "governed": c.is_governed(),
                "ungated_because": c.ungated_because,
                "gates": c.gates.iter().map(|g| json!({
                    "gate": g.gate.id(),
                    "enforcement": g.enforcement,
                    "site": g.site,
                    "why_not_control": g.why_not_control,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    // ── Receipts ─────────────────────────────────────────────────────────
    //
    // Scoped, because `subject` names agents and workspaces. An admin sees the
    // platform's refusals; anyone else sees refusals about subjects they own,
    // and the response says which of the two they are looking at.
    let receipts: Vec<Value> = if is_admin {
        sqlx::query(
            "SELECT gate, decision, reason, subject, decided_at
               FROM gate_decisions
              ORDER BY decided_at DESC
              LIMIT 100",
        )
        .fetch_all(db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            "SELECT gd.gate, gd.decision, gd.reason, gd.subject, gd.decided_at
               FROM gate_decisions gd
               JOIN agents a
                 ON a.agent_name = gd.subject OR a.agent_id::text = gd.subject
              WHERE a.user_id = $1
              ORDER BY gd.decided_at DESC
              LIMIT 100",
        )
        .bind(principal.user_id())
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }
    .iter()
    .map(|r| {
        json!({
            "gate": r.get::<String, _>("gate"),
            "decision": r.get::<String, _>("decision"),
            "reason": r.try_get::<Option<String>, _>("reason").ok().flatten(),
            "subject": r.try_get::<Option<String>, _>("subject").ok().flatten(),
            "decided_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("decided_at")
                .ok().map(|t| t.to_rfc3339()),
        })
    })
    .collect();

    let ledger = fermi::gate_trust::ledger_status();

    Ok(Json(json!({
        "register": register,
        "commands": commands,
        "discarded": fermi::command_registry::gates_computed_and_discarded()
            .into_iter()
            .map(|(cmd, gate)| json!({ "command": cmd, "gate": gate }))
            .collect::<Vec<_>>(),
        "ungoverned": fermi::command_registry::ungoverned_writes(),
        "receipts": receipts,
        "ledger": {
            "pending": ledger.pending,
            "dropped": ledger.dropped,
            "recorded_gates": ledger.recorded_gates,
            "counted_only_gates": ledger.counted_only_gates,
        },
        // Both stated rather than implied. A page that shows counters without
        // saying they reset, or receipts without saying which gates can produce
        // one, reads as a complete record and is not.
        "counters_since_boot": true,
        "receipts_scope": if is_admin { "platform" } else { "your agents" },
    })))
}
