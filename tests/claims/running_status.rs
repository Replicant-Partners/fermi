//! SPEC 36 R0.4 / LLD G05 (#40): `running` is a real status.
//!
//! An episode is reserved as `execution_status = 'running'` before its run
//! finishes (`MemoryStore::reserve_episode`). Before this change every store
//! reader decoded the column with `.parse().unwrap()` against an enum that had
//! no `Running`, so one reserved row panicked the reader. The dev DB had 8
//! such rows when this test was written, so any poll of a reserved A2A task,
//! metrics lookup, or consolidation pass over those agents panicked.
//!
//! Done when: a `running` row passes through consolidation's reader and the
//! paginated listing without panicking, and is excluded from the readers that
//! want outcomes.

use agent_bestiary_memory::{ExecutionStatus, MemoryStore};

use crate::support;

#[tokio::test]
async fn a_running_row_does_not_panic_any_reader() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, created| async move {
        let mut c = created.lock().await;
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                 temperature, author, status, visibility)
             VALUES ($1, 'research', '1.0.0', 'curated', 'llm', 'test', 0.5, 'spec36',
                     'published', 'private')
             RETURNING agent_id",
        )
        .bind(support::tag("running_agent"))
        .fetch_one(&pool)
        .await
        .expect("insert agent");
        c.agents.push(agent);

        let store = MemoryStore::from_pool(pool.clone());
        let running = uuid::Uuid::new_v4();
        c.episodes.push(running);
        store
            .reserve_episode(running, agent, "spec36 running fixture")
            .await
            .expect("reserve");
        let done = uuid::Uuid::new_v4();
        c.episodes.push(done);
        sqlx::query(
            "INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context,
                                   execution_status, execution_time_ms)
             VALUES ($1, $2, now() - interval '1 minute', 'spec36 done fixture',
                     '{}'::jsonb, 'success', 1)",
        )
        .bind(done)
        .bind(agent)
        .execute(&pool)
        .await
        .expect("insert finished episode");
        drop(c);

        // Point read: the A2A task poll, metrics and observatory path.
        let ep = store.get_episode(running).await.expect("get_episode");
        assert_eq!(ep.execution_status, ExecutionStatus::Running);

        // Paginated listing returns both, and decodes the running one.
        let (page, total) = store
            .get_episodes_paginated(agent, 10, 0)
            .await
            .expect("paginated listing");
        assert_eq!(total, 2);
        assert!(page
            .iter()
            .any(|e| e.episode_id == running && e.execution_status == ExecutionStatus::Running));

        // Consolidation's reader wants outcomes only: the reserved row is
        // excluded, so it is never marked consolidated before it finishes.
        let unconsolidated = store
            .get_unconsolidated_episodes(agent)
            .await
            .expect("get_unconsolidated_episodes");
        let ids: Vec<_> = unconsolidated.iter().map(|e| e.episode_id).collect();
        assert_eq!(
            ids,
            vec![done],
            "running row excluded from consolidation input"
        );
    })
    .await;
}

/// The status round-trips through its text form, and an unknown value is a
/// typed error, not a panic.
#[test]
fn execution_status_parses_every_stored_value() {
    for s in ["success", "failure", "partial", "running"] {
        let v: ExecutionStatus = s.parse().expect(s);
        assert_eq!(v.to_string(), s);
    }
    assert!("bogus".parse::<ExecutionStatus>().is_err());
}

/// No DB-sourced value is decoded with `.parse().unwrap()` or
/// `row.get(..).unwrap()` in the memory crate or the server
/// (SPEC 36 R0.4.2). Comments and `#[cfg(test)] mod` blocks are ignored.
#[test]
fn no_unwrap_on_db_decode() {
    let root = env!("CARGO_MANIFEST_DIR");
    let parse_unwrap = regex::Regex::new(
        r#"try_get::<String,\s*_>\([^)]*\)\?\s*\.parse\(\)\s*\.(unwrap|expect)\("#,
    )
    .unwrap();
    let row_get_unwrap =
        regex::Regex::new(r#"\b(row|r)\.(try_)?get\([^)]*\)\s*\.(unwrap|expect)\("#).unwrap();
    let mut hits = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = ["agent-bestiary/memory/src", "src"]
        .iter()
        .map(|d| format!("{root}/{d}").into())
        .collect();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|x| x != "rs") || p.ends_with("tests.rs") {
                continue;
            }
            let src = production_only(&std::fs::read_to_string(&p).unwrap());
            for rx in [&parse_unwrap, &row_get_unwrap] {
                for m in rx.find_iter(&src) {
                    let rel = p.strip_prefix(root).unwrap().display().to_string();
                    hits.push(format!("{rel}: {}", m.as_str()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "unwrap on a DB decode:\n{}",
        hits.join("\n")
    );
}

/// Source with `//` comments and `#[cfg(test)] mod … { … }` blocks removed.
fn production_only(src: &str) -> String {
    let no_comments: String = src
        .lines()
        .map(|l| match l.find("//") {
            Some(i) if !l[..i].contains('"') => &l[..i],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::new();
    let mut rest = no_comments.as_str();
    while let Some(i) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..i]);
        let after = &rest[i + "#[cfg(test)]".len()..];
        if !after.trim_start().starts_with("mod ") {
            rest = after;
            continue;
        }
        let Some(brace) = after.find('{') else {
            rest = "";
            break;
        };
        let mut depth = 0usize;
        let mut end = after.len();
        for (j, ch) in after[brace..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = brace + j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}
