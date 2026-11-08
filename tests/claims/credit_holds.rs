//! SPEC 36 R1.3 / LLD G04 (#48): reserve before work, settle after.
//!
//! Before: `/execute` checked only `balance > 0`, ran the agent, then
//! charged; a failed fee charge became a free run and gas was refused with
//! 402 after the work [V: src/handlers/execution.rs:106, :784-805, :810 @
//! 59d76b00]. Now `credit_hold::open` reserves the estimate before any LLM
//! call and `credit_hold::settle` charges fee, gas and royalty and closes the
//! hold in one transaction.

use std::time::Duration;

use fermi::credit_hold::{self, HoldError, Settlement};

use crate::deployed::base_url;
use crate::support;

async fn balance(pool: &sqlx::PgPool, wallet: uuid::Uuid) -> i32 {
    sqlx::query_scalar("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn closure(pool: &sqlx::PgPool, hold: uuid::Uuid) -> Option<(String, i32, i32)> {
    sqlx::query_as("SELECT kind, charged, unpaid FROM credit_hold_closures WHERE hold_id = $1")
        .bind(hold)
        .fetch_optional(pool)
        .await
        .unwrap()
}

fn plain(exec: i32, gas: i32) -> Settlement {
    Settlement {
        execution_fee: exec,
        gas_fee: gas,
        royalty: None,
        description: "spec36 hold test".into(),
        related_id: None,
    }
}

#[tokio::test]
async fn holds_reserve_settle_and_refuse_before_work() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let mut cc = c.lock().await;
        let caller = support::make_user(&pool, &mut cc).await;
        let owner = support::make_user(&pool, &mut cc).await;
        let w = support::make_wallet(&pool, &mut cc, "user", &format!("{caller}_w"), 30).await;
        cc.wallet_owners.push(owner.clone());
        drop(cc);

        // 20 of 30 reserved; a second 20 is refused before any work.
        let h1 = credit_hold::open(&pool, w, 20, "test")
            .await
            .expect("first hold");
        let refused = credit_hold::open(&pool, w, 20, "test").await.unwrap_err();
        assert!(
            matches!(
                refused,
                HoldError::Insufficient {
                    available: 10,
                    needed: 20
                }
            ),
            "{refused:?}"
        );
        assert_eq!(balance(&pool, w).await, 30, "a hold charges nothing");

        // Settle: fee + gas + royalty in one transaction, hold closed.
        let id1 = h1.hold_id;
        let mut s = plain(10, 1);
        s.royalty = Some((owner.clone(), 8));
        let settled = credit_hold::settle(&pool, h1, &s).await.expect("settle");
        assert_eq!(
            (settled.charged, settled.unpaid, settled.royalty_paid),
            (11, 0, 8)
        );
        assert_eq!(balance(&pool, w).await, 19);
        assert_eq!(closure(&pool, id1).await, Some(("settle".into(), 11, 0)));
        let owner_w: i32 = sqlx::query_scalar(
            "SELECT balance FROM wallets WHERE owner_type = 'user' AND owner_id = $1",
        )
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .expect("owner wallet created in the settle transaction");
        assert_eq!(owner_w, 8);

        // Overrun: charged up to the balance, the rest recorded unpaid.
        let h2 = credit_hold::open(&pool, w, 19, "test").await.expect("hold");
        let id2 = h2.hold_id;
        let over = credit_hold::settle(&pool, h2, &plain(22, 2))
            .await
            .expect("settle");
        assert_eq!((over.charged, over.unpaid), (19, 5));
        assert_eq!(balance(&pool, w).await, 0);
        assert_eq!(closure(&pool, id2).await, Some(("settle".into(), 19, 5)));

        // In debt: no new hold until it can be repaid; then it is repaid first.
        let e = credit_hold::open(&pool, w, 1, "test").await.unwrap_err();
        assert!(matches!(e, HoldError::InDebt { owed: 5, .. }), "{e:?}");
        fermi_auth::credit_deposit(&pool, w, 10, "spec36 top-up")
            .await
            .unwrap();
        let h3 = credit_hold::open(&pool, w, 3, "test")
            .await
            .expect("hold after top-up");
        assert_eq!(
            balance(&pool, w).await,
            5,
            "the 5 unpaid were charged first"
        );
        let id3 = h3.hold_id;
        credit_hold::release(&pool, h3, "test").await.unwrap();
        assert_eq!(closure(&pool, id3).await, Some(("release".into(), 0, 0)));
        assert_eq!(balance(&pool, w).await, 5, "release charges nothing");

        // A dropped hold is released.
        let h4 = credit_hold::open(&pool, w, 2, "test").await.unwrap();
        let id4 = h4.hold_id;
        drop(h4);
        let mut released = false;
        for _ in 0..40 {
            if closure(&pool, id4).await.is_some() {
                released = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        assert!(released, "a dropped hold must be released");

        // Concurrent opens on one wallet: the row lock admits exactly one.
        let (a, b) = tokio::join!(
            credit_hold::open(&pool, w, 4, "race"),
            credit_hold::open(&pool, w, 4, "race")
        );
        assert_eq!(
            [a.is_ok(), b.is_ok()].iter().filter(|x| **x).count(),
            1,
            "exactly one of two 4-credit holds fits 5"
        );
        for h in [a, b].into_iter().flatten() {
            credit_hold::release(&pool, h, "test").await.unwrap();
        }
    })
    .await;
}

/// Live: a caller with 1 credit is refused (402) before the run, and no
/// episode is reserved. On the old code `balance > 0` admitted the run, which
/// then failed after reserving an episode (500).
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_execute_is_refused_before_work_when_the_hold_does_not_fit() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let mut cc = c.lock().await;
        let owner = support::make_user(&pool, &mut cc).await;
        let w = support::make_wallet(&pool, &mut cc, "user", &owner, 1).await;
        let agent: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model,
                                 temperature, author, status, visibility, user_id, llm_provider)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'spec36-model', 0.5,
                     'spec36', 'draft', 'private', $2, 'spec36_no_such_provider')
             RETURNING agent_id",
        )
        .bind(support::tag("hold_agent"))
        .bind(&owner)
        .fetch_one(&pool)
        .await
        .expect("agent");
        cc.agents.push(agent);
        let (key, info) = fermi_auth::api_keys::create_api_key(
            &pool,
            &owner,
            "spec36 claims",
            &["write".to_string()],
        )
        .await
        .expect("api key");
        cc.api_keys.push(info.key_id);
        drop(cc);

        let r = reqwest::Client::new()
            .post(format!("{base}/api/agents/{agent}/execute"))
            .bearer_auth(&key)
            .json(&serde_json::json!({ "query": "spec36 hold probe" }))
            .send()
            .await
            .expect("request");
        let status = r.status().as_u16();
        let body = r.text().await.unwrap_or_default();

        let eps: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT episode_id FROM episodes WHERE agent_id = $1")
                .bind(agent)
                .fetch_all(&pool)
                .await
                .unwrap();
        c.lock().await.episodes.extend(eps.iter().copied());
        assert_eq!(status, 402, "refused before work: {body}");
        assert!(eps.is_empty(), "no episode reserved for a refused run");
        assert_eq!(balance(&pool, w).await, 1, "nothing charged");
    })
    .await;
}
