//! Billing and Stripe handlers.

use axum::{body::Bytes, extract::State, http::StatusCode, Json};
use fermi_auth::{credit_grant, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::{AppState, CREDIT_TIERS};
// ─── Billing / Stripe ──────────────────────────────────────────────

/// Return pricing tiers (public info, but behind auth so we can show personalized data)
pub async fn billing_tiers_handler(State(state): State<AppState>) -> Json<Value> {
    let tiers: Vec<Value> = CREDIT_TIERS
        .iter()
        .map(|t| {
            json!({
                "credits": t.credits,
                "price_cents": t.price_cents,
                "price_display": format!("${:.2}", t.price_cents as f64 / 100.0),
                "per_credit_cents": (t.price_cents as f64 / t.credits as f64 * 100.0).round() / 100.0,
                "label": t.label,
                "discount_pct": t.discount_pct,
            })
        })
        .collect();

    Json(json!({
        "tiers": tiers,
        "stripe_configured": state.stripe.is_configured(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    credits: i32,
}

/// Create a Stripe Checkout Session for credit purchase
pub async fn billing_checkout_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if !state.stripe.is_configured() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Stripe not configured".to_string(),
        ));
    }

    // Find matching tier
    let tier = CREDIT_TIERS
        .iter()
        .find(|t| t.credits == req.credits)
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid credit amount. Choose from: {}",
                CREDIT_TIERS
                    .iter()
                    .map(|t| t.credits.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ))?;

    let user_id = principal.user_id();
    let client = state.stripe.client();

    // Get user email for pre-fill
    let user_email: Option<String> = sqlx::query("SELECT email FROM users WHERE user_id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|row| row.try_get("email").ok())
        .flatten();

    // Build Checkout Session
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("user_id".to_string(), user_id.clone());
    metadata.insert("credits".to_string(), tier.credits.to_string());

    let mut params = stripe::CreateCheckoutSession::new();
    params.mode = Some(stripe::CheckoutSessionMode::Payment);
    params.success_url = Some("/dashboard?payment=success");
    params.cancel_url = Some("/dashboard?payment=cancelled");
    params.metadata = Some(metadata);
    params.customer_email = user_email.as_deref();

    params.line_items = Some(vec![stripe::CreateCheckoutSessionLineItems {
        price_data: Some(stripe::CreateCheckoutSessionLineItemsPriceData {
            currency: stripe::Currency::USD,
            product_data: Some(stripe::CreateCheckoutSessionLineItemsPriceDataProductData {
                name: format!("{} Credits — {}", tier.credits, tier.label),
                description: if tier.discount_pct > 0 {
                    Some(format!("{}% discount", tier.discount_pct))
                } else {
                    None
                },
                ..Default::default()
            }),
            unit_amount: Some(tier.price_cents),
            ..Default::default()
        }),
        quantity: Some(1),
        ..Default::default()
    }]);

    let session = stripe::CheckoutSession::create(&client, params)
        .await
        .map_err(|e| {
            eprintln!("Stripe checkout session creation failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create checkout session: {}", e),
            )
        })?;

    let checkout_url = session.url.ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "No checkout URL returned".to_string(),
    ))?;

    Ok(Json(json!({
        "checkout_url": checkout_url,
        "session_id": session.id.as_str(),
    })))
}

/// Dev/beta credit faucet — grants 500 credits when Stripe is not configured.
/// Auto-disables in production when STRIPE_SECRET_KEY is set.
pub async fn billing_dev_topup_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    if state.stripe.is_configured() {
        return Err((
            StatusCode::GONE,
            "Dev top-up disabled — use Stripe checkout to purchase credits.".to_string(),
        ));
    }

    let user_id = principal.user_id();
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let grant_amount = 500;
    credit_grant(
        &state.db,
        wallet.wallet_id,
        grant_amount,
        "Beta testing credit grant",
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Refresh balance
    let new_balance: i32 = sqlx::query("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet.wallet_id)
        .fetch_one(&state.db)
        .await
        .map(|row| sqlx::Row::get(&row, "balance"))
        .unwrap_or(grant_amount);

    Ok(Json(json!({
        "status": "granted",
        "credits": grant_amount,
        "new_balance": new_balance,
    })))
}

/// Stripe webhook handler — verifies signature, processes events.
/// This endpoint has NO auth middleware (Stripe calls it directly).
pub async fn stripe_webhook_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.stripe.is_configured() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Stripe not configured".to_string(),
        ));
    }

    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Missing stripe-signature header".to_string(),
        ))?;

    let payload = std::str::from_utf8(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid UTF-8 in request body".to_string(),
        )
    })?;

    let event = stripe::Webhook::construct_event(payload, signature, &state.stripe.webhook_secret)
        .map_err(|e| {
            eprintln!("Stripe webhook verification failed: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                "Invalid webhook signature".to_string(),
            )
        })?;

    match event.type_ {
        stripe::EventType::CheckoutSessionCompleted => {
            if let stripe::EventObject::CheckoutSession(session) = event.data.object {
                handle_checkout_completed(&state, session).await;
            }
        }
        other => {
            println!("Stripe webhook: unhandled event type {:?}", other);
        }
    }

    Ok(StatusCode::OK)
}

pub async fn handle_checkout_completed(state: &AppState, session: stripe::CheckoutSession) {
    let metadata = match &session.metadata {
        Some(m) => m,
        None => {
            eprintln!("Stripe checkout: no metadata on session {}", session.id);
            return;
        }
    };

    let user_id = match metadata.get("user_id") {
        Some(id) => id.clone(),
        None => {
            eprintln!("Stripe checkout: missing user_id in metadata");
            return;
        }
    };

    let credits: i32 = match metadata.get("credits").and_then(|c| c.parse().ok()) {
        Some(c) => c,
        None => {
            eprintln!("Stripe checkout: missing or invalid credits in metadata");
            return;
        }
    };

    // ── Idempotency: CLAIM the session before crediting ───────────────
    //
    // The previous implementation did `if let Ok(Some(_)) = existing`, which
    // treats a database error as "not yet processed" and credits again, and
    // wrote the marker AFTER the deposit via a swallowed `let _ = UPDATE`.
    // Read-then-write is not idempotency; it is a race with money in it.
    //
    // Now the database decides. INSERT … ON CONFLICT DO NOTHING RETURNING
    // yields a row only for the caller that actually won the claim, so
    // concurrent webhook deliveries cannot both proceed. See migration 182.
    let session_id_str = session.id.as_str().to_string();

    let claim = sqlx::query(
        "INSERT INTO stripe_sessions_processed (session_id, user_id, credits) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (session_id) DO NOTHING \
         RETURNING session_id",
    )
    .bind(&session_id_str)
    .bind(&user_id)
    .bind(credits)
    .fetch_optional(&state.db)
    .await;

    match claim {
        Ok(Some(_)) => { /* we own this session — proceed to credit */ }
        Ok(None) => {
            println!(
                "Stripe checkout: session {} already claimed, skipping",
                session_id_str
            );
            return;
        }
        Err(e) => {
            // FAIL CLOSED. We cannot prove this session is unprocessed, so we
            // must not credit. Stripe retries failed webhooks, so declining
            // here loses nothing; guessing could double-credit real money.
            eprintln!(
                "Stripe checkout: CANNOT VERIFY idempotency for session {} ({}). \
                 Refusing to credit — Stripe will retry. If this persists, check \
                 that migration 182 applied.",
                session_id_str, e
            );
            return;
        }
    }

    // Release the claim so a Stripe retry can legitimately re-attempt.
    // Without this, a transient failure below would strand the session as
    // permanently "claimed" and the user would never receive their credits.
    async fn release_claim(db: &sqlx::PgPool, session_id: &str) {
        if let Err(e) = sqlx::query("DELETE FROM stripe_sessions_processed WHERE session_id = $1")
            .bind(session_id)
            .execute(db)
            .await
        {
            eprintln!(
                "Stripe checkout: FAILED TO RELEASE claim for session {} ({}). \
                 This session will not be retried automatically — credits may be owed.",
                session_id, e
            );
        }
    }

    // Credit the user's wallet
    match get_or_create_wallet(&state.db, "user", &user_id).await {
        Ok(wallet) => {
            match fermi_auth::credit_deposit(
                &state.db,
                wallet.wallet_id,
                credits,
                &format!("Stripe purchase: {} credits", credits),
            )
            .await
            {
                Ok(tx) => {
                    // Record stripe_session_id on the ledger entry. Still
                    // best-effort, but no longer load-bearing for idempotency
                    // — the claim above is. A failure here is now a
                    // reporting gap, not a double-credit.
                    if let Err(e) = sqlx::query(
                        "UPDATE credit_ledger SET stripe_session_id = $1 WHERE tx_id = $2",
                    )
                    .bind(&session_id_str)
                    .bind(tx.tx_id)
                    .execute(&state.db)
                    .await
                    {
                        eprintln!(
                            "Stripe checkout: credited session {} but failed to stamp the \
                             ledger row ({}). Credits ARE applied; the ledger link is missing.",
                            session_id_str, e
                        );
                    }

                    // Settle the claim: records that the deposit completed, so
                    // a NULL settled_at is a reliable "claimed but never
                    // credited" signal for the integrity audit.
                    if let Err(e) = sqlx::query(
                        "UPDATE stripe_sessions_processed \
                            SET settled_at = NOW(), tx_id = $2 WHERE session_id = $1",
                    )
                    .bind(&session_id_str)
                    .bind(tx.tx_id)
                    .execute(&state.db)
                    .await
                    {
                        eprintln!(
                            "Stripe checkout: could not settle claim for session {} ({})",
                            session_id_str, e
                        );
                    }

                    println!(
                        "Stripe checkout: credited {} credits to user {} (session {})",
                        credits, user_id, session_id_str
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Stripe checkout: failed to deposit credits for user {}: {}",
                        user_id, e
                    );
                    release_claim(&state.db, &session_id_str).await;
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Stripe checkout: failed to get/create wallet for user {}: {}",
                user_id, e
            );
            release_claim(&state.db, &session_id_str).await;
        }
    }
}
