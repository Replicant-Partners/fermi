//! Backfill mutex cascades for WC teams that already resolved NO.
//!
//! Six teams (Turkiye, Tunisia, Qatar, Jordan, Panama, Haiti) were
//! resolved as eliminated, but no cascade ever fired — the queue path
//! errored on the (then-missing) relationship_groups column and returned
//! before redistributing. So survivors never absorbed the eliminated
//! teams' mass and trajectories never moved.
//!
//! This replays, in resolution order, the SAME mutex resolved-NO
//! redistribution that src/handlers/relationships/propagation.rs now
//! performs (survivors = members that are neither the trigger nor already
//! resolved; each survivor absorbs trigger_prev · pᵢ/Σp; trigger → 0.001).
//! It writes a fermi_forecast_updates row per delta with
//! revision_trigger='cascade' so the trajectory tab shows each step, and
//! updates fermi_forecasts + workspace_outputs.
//!
//! This is mass-conserving redistribution per Spec 25 §3.1 — NOT a global
//! renormalization to Σ=1. The independent per-team sims sum to ~1.5; that
//! calibration artifact is out of scope here.
//!
//! Run:  DATABASE_URL=… cargo run --bin apply_wc_cascades -- [--dry-run]

use std::collections::HashSet;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};

const GROUP_ID: &str = "wc_2026_winner";

#[tokio::main]
async fn main() {
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connect_options = PgConnectOptions::from_str(&database_url)
        .expect("parse DATABASE_URL")
        .statement_cache_capacity(0);
    let db: PgPool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(connect_options)
        .await
        .expect("connect");

    // Group members.
    let member_rows = sqlx::query(
        "SELECT id, question_text, predicted_probability, actual_outcome, resolved_at
           FROM public.fermi_forecasts
          WHERE relationship_groups @> ARRAY[$1]",
    )
    .bind(GROUP_ID)
    .fetch_all(&db)
    .await
    .expect("fetch members");

    // Working copy of current probabilities.
    let mut prob: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut resolved: HashSet<String> = HashSet::new();
    // Triggers = resolved-NO teams, applied oldest-resolution first.
    let mut triggers: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> = Vec::new();

    for r in &member_rows {
        let id: String = r.get("id");
        let p: f32 = r.get("predicted_probability");
        let q: String = r.get("question_text");
        let outcome: Option<bool> = r.try_get("actual_outcome").ok().flatten();
        prob.insert(id.clone(), p as f64);
        name.insert(id.clone(), q.clone());
        if outcome.is_some() {
            resolved.insert(id.clone());
        }
        if outcome == Some(false) {
            let at: Option<chrono::DateTime<chrono::Utc>> = r.try_get("resolved_at").ok().flatten();
            triggers.push((id.clone(), at));
        }
    }
    triggers.sort_by(|a, b| a.1.cmp(&b.1));

    let member_ids: Vec<String> = member_rows
        .iter()
        .map(|r| r.get::<String, _>("id"))
        .collect();

    println!(
        "Group '{}': {} members, {} resolved-NO triggers to replay{}\n",
        GROUP_ID,
        member_ids.len(),
        triggers.len(),
        if dry_run { " (DRY RUN)" } else { "" }
    );

    let arg_id = name
        .iter()
        .find(|(_, q)| q.contains("Argentina"))
        .map(|(id, _)| id.clone());

    // Idempotency ledger, read from the append-only update log.
    //
    // This used to be inferred from the trigger's own probability: the
    // run pinned each eliminated forecast to 0.001 and treated "already
    // at the floor" as "already cascaded". That marker was destructive —
    // the trigger is by definition a *resolved* forecast, so writing
    // 0.001 into it overwrote the very probability its stored
    // brier_score had been computed against. That is what corrupted all
    // 47 Polymarket-resolved forecasts (see mig-174).
    //
    // The cascade reason string is already unique per trigger and is
    // already written to fermi_forecast_updates, so the log is a
    // sufficient and non-destructive signal.
    let applied: HashSet<String> = sqlx::query(
        "SELECT DISTINCT reason
           FROM public.fermi_forecast_updates
          WHERE revision_trigger = 'cascade'
            AND reason LIKE 'cascade from %'",
    )
    .fetch_all(&db)
    .await
    .expect("fetch applied cascades")
    .iter()
    .map(|r| r.get::<String, _>("reason"))
    .collect();

    for (trigger, _) in &triggers {
        let trigger_prev = *prob.get(trigger).unwrap_or(&0.0);
        let reason = format!("cascade from {} (resolved)", trigger);

        // Idempotency: skip if this trigger's cascade is already in the
        // update log, so re-runs don't double-redistribute.
        if applied.contains(&reason) {
            println!(
                "  {} already cascaded, skipping",
                name[trigger]
                    .trim_start_matches("Will ")
                    .trim_end_matches(" win the 2026 FIFA World Cup?")
            );
            continue;
        }

        // survivors: not the trigger, not already resolved.
        let survivors: Vec<String> = member_ids
            .iter()
            .filter(|id| *id != trigger && !resolved.contains(*id))
            .cloned()
            .collect();
        let survivor_total: f64 = survivors.iter().map(|id| prob[id]).sum();

        let trig_name = name[trigger]
            .trim_start_matches("Will ")
            .trim_end_matches(" win the 2026 FIFA World Cup?")
            .to_string();

        if survivor_total < 1e-9 {
            println!("  {trig_name}: survivors sum ~0, skipping");
            continue;
        }

        let mut deltas: Vec<(String, f64, f64)> = Vec::new();
        for id in &survivors {
            let prev = prob[id];
            let absorbed = trigger_prev * (prev / survivor_total);
            let new_p = (prev + absorbed).clamp(0.001, 0.999);
            if (new_p - prev).abs() > 1e-5 {
                deltas.push((id.clone(), prev, new_p));
            }
        }

        // The trigger deliberately gets NO delta. It is a resolved
        // forecast: its elimination is already recorded by
        // actual_outcome = false, and its probability is the historical
        // record its Brier score was scored against. Pinning it to 0.001
        // added no information and destroyed the audit trail. Its mass is
        // still redistributed to survivors above — only the destructive
        // self-write is gone.
        //
        // Keep the in-memory map consistent for the rest of this run
        // without touching the database.
        prob.insert(trigger.clone(), 0.001);

        let arg_line = arg_id.as_ref().and_then(|aid| {
            deltas
                .iter()
                .find(|(id, _, _)| id == aid)
                .map(|(_, prev, new)| {
                    format!("   Argentina {:.3}% → {:.3}%", prev * 100.0, new * 100.0)
                })
        });

        println!(
            "  {} eliminated (was {:.2}%) → redistributed across {} survivors.{}",
            trig_name,
            trigger_prev * 100.0,
            survivors.len(),
            arg_line.map(|s| format!("\n{s}")).unwrap_or_default()
        );

        if !dry_run {
            // Batch via UNNEST: 2 round-trips per cascade instead of ~86.
            let ids: Vec<String> = deltas.iter().map(|(id, _, _)| id.clone()).collect();
            let prevs: Vec<f32> = deltas.iter().map(|(_, p, _)| *p as f32).collect();
            let news: Vec<f32> = deltas.iter().map(|(_, _, n)| *n as f32).collect();

            sqlx::query(
                "INSERT INTO public.fermi_forecast_updates
                      (id, forecast_id, previous_probability, new_probability,
                       reason, revision_trigger, created_at)
                 SELECT gen_random_uuid()::text, t.fid, t.prev, t.newp, $4, 'cascade', NOW()
                   FROM UNNEST($1::text[], $2::real[], $3::real[]) AS t(fid, prev, newp)",
            )
            .bind(&ids)
            .bind(&prevs)
            .bind(&news)
            .bind(&reason)
            .execute(&db)
            .await
            .expect("batch insert updates");

            // `AND f.status = 'active'` is load-bearing. Without it this
            // UPDATE rewrote the probability of already-resolved
            // forecasts — including pinning every eliminated one to 0.001
            // via the delta pushed at :139-141 below — which destroyed the
            // input that their stored brier_score had been computed
            // against. All 47 Polymarket-resolved forecasts were corrupted
            // this way (91 post-resolution revisions) before mig-174.
            //
            // mig-174 also installs a BEFORE UPDATE trigger that pins the
            // scoring tuple on resolved rows, so this is now defence in
            // depth rather than the sole guard — but filtering here keeps
            // the warning log quiet and makes the intent explicit.
            sqlx::query(
                "UPDATE public.fermi_forecasts f
                    SET predicted_probability = t.newp, updated_at = NOW()
                   FROM UNNEST($1::text[], $2::real[]) AS t(fid, newp)
                  WHERE f.id = t.fid
                    AND f.status = 'active'",
            )
            .bind(&ids)
            .bind(&news)
            .execute(&db)
            .await
            .expect("batch update forecasts");
        }

        // Apply to the in-memory working copy so the next trigger sees
        // post-cascade probabilities.
        for (id, _prev, new_p) in &deltas {
            prob.insert(id.clone(), *new_p);
        }
    }

    let live_sum: f64 = member_ids
        .iter()
        .filter(|id| !resolved.contains(*id))
        .map(|id| prob[id])
        .sum();
    if let Some(aid) = &arg_id {
        println!("\nArgentina final: {:.3}%", prob[aid] * 100.0);
    }
    println!(
        "Live (unresolved) field sum after cascades: {:.3}  \
         (mass-conserving redistribution; not renormalized to 1.0)",
        live_sum
    );
}
