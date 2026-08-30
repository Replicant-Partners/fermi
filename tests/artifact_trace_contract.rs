//! Does the trace assemble against real episodes, and is it honest about them?
//!
//! # What only production can settle
//!
//! The module's own suite proves the composition: an empty trace is `unknown`, a
//! violation is a fault, the floor comes from `grounding_trust`. None of that
//! needs a database.
//!
//! Two things do. **The contract is re-run over retained bytes**, so whether a
//! historical episode can be traced at all depends on what `response_text`
//! actually holds — and the answer was surprising once already: 0 of 94 responses
//! from contracted agents are bare JSON and 64 carry a document embedded in
//! prose, which a bare `serde_json::from_str` refused for the life of the
//! feature. And **the distribution of readings** decides whether the endpoint is
//! worth building a screen against, which is a fact about the corpus rather than
//! about the code.
//!
//! # This suite reports, and asserts only what cannot be a matter of degree
//!
//! The reading distribution is **printed**. A threshold on it would be a target,
//! and it moves with every deploy. What is asserted is the invariant the UX team
//! will build against: **an episode with no graded fields is never `idle`**, and
//! **every trace names an owner**. Those cannot be true of some episodes and false
//! of others without the surface lying.

use fermi::artifact_trace;
use fermi::declaration_ladder::{self as dl, Owner};
use fermi::panel_absence::Reading;
use sqlx::{PgPool, Row};

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// Everything the handler needs, for one episode.
struct Traced {
    agent: String,
    graded: usize,
    violations: usize,
    reading: Reading,
    token: &'static str,
    owner: Owner,
    floor_strength: u8,
    /// Did the hashes independently agree that enforcement changed the document?
    grounding_changed: Option<bool>,
}

async fn trace_all(pool: &PgPool, limit: i64) -> Vec<Traced> {
    let rows = sqlx::query(
        "SELECT a.agent_name, e.response_text, a.accepts, a.produces, \
                a.output_contract \
           FROM episodes e JOIN agents a ON a.agent_id = e.agent_id \
          WHERE e.response_text IS NOT NULL AND e.response_text <> '' \
          ORDER BY e.timestamp_ref DESC \
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .expect("read episodes");

    let mut out = Vec::new();
    for r in &rows {
        let agent: String = r.try_get("agent_name").unwrap_or_default();
        let text: String = r.try_get("response_text").unwrap_or_default();

        // Exactly what the handler does: recover the document, keep the claimed
        // copy, enforce on a clone, grade from the report.
        let claimed = fermi::agent_backend::envelope::extract_json(&text);
        let mut enforced = claimed.clone();
        let report = match enforced.as_mut() {
            Some(d) => fermi::grounding_trust::enforce(&agent, d),
            None => fermi::grounding_trust::Report::default(),
        };
        let graded = match claimed.as_ref() {
            Some(d) => fermi::grounding_trust::graded_fields(&agent, d, &report),
            None => Vec::new(),
        };

        let mut declared: Vec<&'static str> = Vec::new();
        let accepts: Option<Vec<String>> = r.try_get("accepts").ok();
        let produces: Option<Vec<String>> = r.try_get("produces").ok();
        if accepts.as_ref().is_some_and(|v| !v.is_empty())
            && produces.as_ref().is_some_and(|v| !v.is_empty())
        {
            declared.push("ports");
        }
        if let Some(oc) = r
            .try_get::<Option<serde_json::Value>, _>("output_contract")
            .ok()
            .flatten()
        {
            if oc.get("produces_schema").is_some() {
                declared.push("output_type");
            }
            if oc.get("schema").is_some_and(|s| s.is_object()) {
                declared.push("output_schema");
            }
        }
        if dl::has_field_contract(&agent) {
            declared.push("field_contract");
        }
        let legibility = dl::legibility(&declared);

        let (reading, token, _silence, owner) =
            artifact_trace::reading(report.violations.len(), &graded, &legibility);
        let (_, floor) = artifact_trace::fields(&agent, &graded);
        // Computed the way the handler computes it: the claimed text against the
        // ENFORCED document, never the same value twice.
        let hashes = fermi::artifact_hash::of_episode(None, Some(&text), enforced.as_ref());

        out.push(Traced {
            agent,
            graded: graded.len(),
            violations: report.violations.len(),
            reading,
            token,
            owner,
            floor_strength: fermi::grounding_trust::strength(floor),
            grounding_changed: hashes.enforcement_changed_the_bytes,
        });
    }
    out
}

/// The two invariants the UX team builds against.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_trace_is_ever_green_on_nothing() {
    let pool = pool().await;
    let traced = trace_all(&pool, 400).await;
    assert!(
        !traced.is_empty(),
        "no episode retained a response, so this proves nothing"
    );

    let mut broken = Vec::new();
    for t in &traced {
        // The one that matters. An artifact with no graded fields has an empty
        // journey, and `idle` would colour it as a pass on evidence about none of
        // it. 3,571 of 3,576 episodes are in this state, so this is the DEFAULT
        // screen, not an edge case.
        if t.graded == 0 && t.reading == Reading::Idle {
            broken.push(format!(
                "{}: 0 graded fields and reading `idle` (token `{}`)",
                t.agent, t.token
            ));
        }
        // And the symmetric error: a graded, clean, fully declared artifact must
        // not be reported as unknowable, or the retrofit has no gradient and
        // there is no reason for an agent to declare anything.
        if t.graded > 0 && t.violations == 0 && t.reading == Reading::Fault {
            broken.push(format!(
                "{}: {} fields graded clean and reading `fault`",
                t.agent, t.graded
            ));
        }
        // Two independent computations of the same fact, and they must agree.
        //
        // `report.violations` is the contract's own count of fields it refused;
        // `enforcement_changed_the_bytes` is a SHA-256 comparison of the
        // document before and after enforcement. A violation means `enforce`
        // nulled a field, so the digests must differ -- and they are arrived at
        // by completely different routes, which is what makes the agreement worth
        // asserting. If it ever fails, one of the two is describing a document the
        // other is not.
        if t.violations > 0 && t.grounding_changed != Some(true) {
            broken.push(format!(
                "{}: {} violation(s) and the hashes say the document was \
                 {:?} changed by enforcement",
                t.agent, t.violations, t.grounding_changed
            ));
        }
        // The reverse is deliberately NOT asserted, and finding out why is
        // what this cross-check earned. `enforce` also stamps
        // `<block>_provenance` siblings onto the document as bookkeeping, so the
        // bytes change on every contracted response whether or not anything was
        // wrong -- 21 episodes here, with zero violations between them. An
        // assertion that no violations implies no change would fire on entirely
        // correct behaviour, and the field is now named
        // `enforcement_changed_the_bytes` precisely so nobody reads it as
        // "a claim was stripped".
        // Every trace names an owner. A finding with no owner is a backlog item
        // nobody can pick up.
        if t.reading != Reading::Idle && t.owner == Owner::NoOne {
            broken.push(format!(
                "{}: reading `{}` with owner `no_one` — nobody can act on it",
                t.agent,
                t.reading.label()
            ));
        }
    }

    assert!(
        broken.is_empty(),
        "\n  {}\n\n{} of {} traces break an invariant the UX surface depends on.\n",
        broken.join("\n  "),
        broken.len(),
        traced.len()
    );
    let hashed = traced
        .iter()
        .filter(|t| t.grounding_changed.is_some())
        .count();
    println!(
        "  {} trace(s) assembled; none green on nothing, all owned; {hashed} had \
         a document to hash before and after enforcement.",
        traced.len()
    );
}

/// What the trace surface will actually look like on arrival.
///
/// Reported, never asserted: a distribution is a fact about the corpus and
/// asserting one would assert that traced episodes must exist. This is the number
/// the UX team needs before designing the screen, because the majority state is
/// the one most likely to be treated as an error page.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_what_the_trace_surface_will_show() {
    let pool = pool().await;
    let traced = trace_all(&pool, 400).await;

    let mut by_token: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut by_owner: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut graded_any = 0usize;
    let mut reliable = 0usize;
    for t in &traced {
        *by_token.entry(t.token).or_default() += 1;
        *by_owner
            .entry(match t.owner {
                Owner::Platform => "platform",
                Owner::AgentAuthor => "agent_author",
                Owner::NoOne => "no_one",
            })
            .or_default() += 1;
        if t.graded > 0 {
            graded_any += 1;
        }
        if t.floor_strength >= 2 {
            reliable += 1;
        }
    }

    println!(
        "\n  Over the {} most recent episodes with a retained response:",
        traced.len()
    );
    println!("\n    reading token:");
    for (k, n) in &by_token {
        println!("      {k:<20} {n}");
    }
    println!("\n    whose work:");
    for (k, n) in &by_owner {
        println!("      {k:<20} {n}");
    }
    println!(
        "\n    {graded_any} had any field graded at all; {reliable} floor at \
         strength 2 (reproducible)."
    );
    println!(
        "\n  The majority token is the DEFAULT SCREEN. If it cannot be made to \
         look like a real answer rather than a loading failure, that is a \
         conversation to have before the screen is built, not after."
    );

    // The violations, named. This is the payoff of migration 199's retention and
    // it is worth printing rather than counting: re-running the contract over
    // retained bytes finds violations that were NEVER RECORDED, because the
    // contract did not exist -- or was not wired to that path -- when the episode
    // ran. `episodes.tags` carries `grounding:violations` on 0 rows.
    let mut offenders: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for t in traced.iter().filter(|t| t.violations > 0) {
        *offenders.entry(t.agent.as_str()).or_default() += 1;
    }
    if offenders.is_empty() {
        println!(
            "\n  No retained response violates its agent's contract. Reported, \
             never asserted: a clean corpus is a legitimate outcome and \
             asserting otherwise would assert that fabrication must exist.\n"
        );
    } else {
        println!("\n  Contract violations found in retained responses:");
        for (agent, n) in &offenders {
            println!("      {agent:<24} {n}");
        }
        println!(
            "\n  These are correctable anomalies with a named agent, a named \
             field and the claimed value retained. None of them was recorded when \
             it happened -- `episodes.tags` has `grounding:violations` on 0 rows \
             -- so every one is a finding the platform could not previously see.\n"
        );
    }
}
