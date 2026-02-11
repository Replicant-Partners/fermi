# Agent Bestiary World - Wallet System Architecture

## Overview

The AWB wallet system provides a **rock-solid, audit-trail-complete** credit management system for all applications built on the platform. This document defines the canonical patterns that MUST be followed.

## Core Principles

1. **Append-only ledger**: Every transaction is immutable
2. **Atomic operations**: No partial states, no race conditions
3. **PgBouncer compatible**: No explicit BEGIN/COMMIT transactions
4. **Single source of truth**: The `wallets` table balance is authoritative
5. **Audit trail**: Every credit movement is traceable via `credit_ledger`

## Database Schema

### Wallets Table
```sql
CREATE TABLE wallets (
    wallet_id UUID PRIMARY KEY,
    owner_type TEXT CHECK (owner_type IN ('user', 'workspace')),
    owner_id TEXT NOT NULL UNIQUE,
    balance INTEGER NOT NULL DEFAULT 0,
    total_deposited INTEGER NOT NULL DEFAULT 0,
    total_spent INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

**Owner Types**:
- `user`: Individual AWB OAuth user (shared across Rabble, Fermi, Agent Bestiary)
- `workspace`: Team workspace for collaborative work

**Balance Invariants**:
- `balance >= 0` (enforced by application logic)
- `balance = total_deposited - total_spent` (enforced by atomic operations)

### Credit Ledger Table
```sql
CREATE TABLE credit_ledger (
    tx_id UUID PRIMARY KEY,
    wallet_id UUID NOT NULL REFERENCES wallets(wallet_id),
    amount INTEGER NOT NULL,           -- positive = credit, negative = debit
    balance_after INTEGER NOT NULL,
    tx_type TEXT NOT NULL CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee', 'publish_fee',
        'eval_fee', 'consolidation_fee',
        'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
        'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
        'collection_create', 'rabble_chat',
        'avatar_generate', 'embedding_import', 'ontology_generation',
        'notebook_execute', 'forecast_create', 'portfolio_analyze'
    )),
    description TEXT,
    related_id TEXT,                   -- episode_id, creature_id, notebook_id, etc.
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

**Transaction Types by Application**:
- **Agent Bestiary**: `execution_fee`, `gas_fee`, `consolidation_fee`, `marketplace_*`, `avatar_generate`
- **Rabble**: `creature_mint`, `creature_flight`, `swarm_create`, `swarm_join`, `collection_create`, `rabble_chat`
- **Fermi**: `notebook_execute`, `forecast_create`, `portfolio_analyze`
- **Platform**: `deposit`, `grant`, `refund`, `transfer_*`

## Critical Functions (fermi-auth/src/credits.rs)

### 1. get_or_create_wallet()
```rust
pub async fn get_or_create_wallet(
    pool: &PgPool,
    owner_type: &str,  // "user" or "workspace"
    owner_id: &str,    // user_id from AuthPrincipal
) -> Result<Wallet, AuthError>
```

**Pattern**: ALWAYS call this before any credit operation.
```rust
let user_id = principal.user_id();
let wallet = get_or_create_wallet(&state.db, "user", &user_id)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
```

### 2. charge() - Debit credits with balance check
```rust
pub async fn charge(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<CreditTransaction, AuthError>
```

**Atomicity guarantee**: Uses conditional UPDATE
```sql
UPDATE wallets 
SET balance = balance - $amount, total_spent = total_spent + $amount
WHERE wallet_id = $id AND balance >= $amount
RETURNING balance
```

If balance insufficient, UPDATE returns 0 rows → error returned BEFORE ledger insert.

**Pattern**:
```rust
charge(
    pool,
    wallet.wallet_id,
    cost,
    "creature_mint",
    &format!("Mint {}", scientific_name),
    Some(&creature_id.to_string()),
)
.await?;
```

### 3. grant() - Add free credits
```rust
pub async fn grant(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError>
```

**Use cases**:
- Beta user onboarding credits
- Promotional campaigns
- Admin grants for testing
- Compensation for platform issues

**Pattern**:
```rust
grant(
    pool,
    wallet.wallet_id,
    500,
    "Welcome to Rabble - starter credits",
)
.await?;
```

### 4. deposit() - Paid credits (Stripe)
```rust
pub async fn deposit(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    description: &str,
) -> Result<CreditTransaction, AuthError>
```

**Pattern**: Called from Stripe webhook after successful payment
```rust
deposit(
    pool,
    wallet.wallet_id,
    credits_purchased,
    &format!("Stripe payment {}", payment_intent_id),
)
.await?;
```

## Application Integration Patterns

### Pattern 1: Standard User Operation (Rabble, Fermi, AWB)

```rust
pub async fn some_operation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<OperationRequest>,
) -> Result<Json<Response>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    
    // 1. Get or create wallet
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // 2. Calculate cost
    let cost = calculate_operation_cost(&req);
    
    // 3. Charge BEFORE doing work (fail fast if insufficient)
    charge(
        pool,
        wallet.wallet_id,
        cost,
        "operation_type",
        &format!("Operation: {}", req.name),
        Some(&operation_id.to_string()),
    )
    .await
    .map_err(|e| match e {
        AuthError::InvalidInput(msg) => (StatusCode::PAYMENT_REQUIRED, msg),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;
    
    // 4. Do the actual work
    let result = perform_operation(pool, &req).await?;
    
    Ok(Json(result))
}
```

**Key points**:
- Charge BEFORE doing work (fail fast)
- Return 402 Payment Required on insufficient balance
- Use specific tx_type for each operation
- Include related_id for audit trail

### Pattern 2: Workspace Operations

```rust
// Get workspace wallet instead of user wallet
let team: Team = get_team(pool, &workspace_id).await?;
let workspace_wallet = get_or_create_wallet(pool, "workspace", &team.id.to_string())
    .await?;

charge(
    pool,
    workspace_wallet.wallet_id,
    cost,
    "workspace_operation",
    description,
    related_id,
)
.await?;
```

### Pattern 3: Admin Bulk Grant (NEW - for Rabble/Fermi onboarding)

```rust
pub async fn admin_bulk_grant_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<BulkGrantRequest>,
) -> Result<Json<BulkGrantResponse>, (StatusCode, String)> {
    // Check admin permission
    if !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Admin only".into()));
    }
    
    let pool = state.memory_store.pool();
    let mut granted = vec![];
    let mut failed = vec![];
    
    for user_id in &req.user_ids {
        let wallet = match get_or_create_wallet(pool, "user", user_id).await {
            Ok(w) => w,
            Err(e) => {
                failed.push(json!({"user_id": user_id, "error": e.to_string()}));
                continue;
            }
        };
        
        match grant(pool, wallet.wallet_id, req.amount, &req.description).await {
            Ok(tx) => granted.push(json!({
                "user_id": user_id,
                "amount": req.amount,
                "tx_id": tx.tx_id,
            })),
            Err(e) => failed.push(json!({"user_id": user_id, "error": e.to_string()})),
        }
    }
    
    Ok(Json(BulkGrantResponse { granted, failed }))
}
```

## Gas Fees Helper (src/gas.rs)

For AWB-specific operations with configurable gas fees:

```rust
pub async fn charge_gas(
    pool: &PgPool,
    wallet_id: Uuid,
    base_amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    // Gas fee = 10% of base amount
    let gas_fee = (base_amount as f64 * 0.1).ceil() as i32;
    let total = base_amount + gas_fee;
    
    charge(pool, wallet_id, total, tx_type, description, related_id)
        .await
        .map_err(|e| match e {
            AuthError::InvalidInput(msg) => (StatusCode::PAYMENT_REQUIRED, msg),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;
    
    Ok(())
}
```

**When to use**:
- Use `charge_gas()` for AWB platform operations (adds 10% platform fee)
- Use direct `charge()` for app-specific operations without platform fee

## Wallet Balance Checks

### Get balance (read-only)
```rust
use fermi_auth::get_balance;

let balance = get_balance(pool, wallet_id).await?;
if balance < required_amount {
    return Err((StatusCode::PAYMENT_REQUIRED, "Insufficient credits".into()));
}
```

### Get transaction history
```rust
use fermi_auth::get_transactions;

let transactions = get_transactions(pool, wallet_id, limit, offset).await?;
```

## New User Onboarding Pattern

For Rabble and Fermi applications, grant starter credits on first interaction:

```rust
pub async fn ensure_starter_credits(
    pool: &PgPool,
    user_id: &str,
    app_name: &str,
    amount: i32,
) -> Result<(), AuthError> {
    let wallet = get_or_create_wallet(pool, "user", user_id).await?;
    
    // Check if user has already received starter credits for this app
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT tx_id FROM credit_ledger 
         WHERE wallet_id = $1 AND description LIKE $2 LIMIT 1"
    )
    .bind(wallet.wallet_id)
    .bind(format!("{}%starter credits", app_name))
    .fetch_optional(pool)
    .await?;
    
    if existing.is_none() {
        grant(
            pool,
            wallet.wallet_id,
            amount,
            &format!("{} - Welcome starter credits", app_name),
        )
        .await?;
    }
    
    Ok(())
}
```

**Usage**:
```rust
// In first Rabble creature mint
ensure_starter_credits(pool, &user_id, "Rabble", 500).await?;

// In first Fermi notebook execution
ensure_starter_credits(pool, &user_id, "Fermi", 1000).await?;
```

## Transaction Type Guidelines

When adding new operations, follow these naming conventions:

- **Product actions**: `{product}_{action}` (e.g., `creature_mint`, `notebook_execute`)
- **Platform services**: `{service}_fee` (e.g., `execution_fee`, `gas_fee`)
- **Financial ops**: `{type}` (e.g., `deposit`, `grant`, `refund`)
- **Marketplace**: `marketplace_{action}` (e.g., `marketplace_listing_fee`)

## Testing Wallet Operations

### Local development faucet
```bash
curl -X POST http://localhost:3000/api/billing/dev-topup \
  -H "Authorization: Bearer $JWT_TOKEN"
# Returns: {"status": "granted", "credits": 500, "new_balance": 500}
```

**Note**: Auto-disabled when `STRIPE_SECRET_KEY` is set (production).

### Admin grant endpoint
```bash
curl -X POST http://localhost:3000/api/admin/users/$USER_ID/grant \
  -H "Authorization: Bearer $ADMIN_JWT" \
  -H "Content-Type: application/json" \
  -d '{"amount": 1000, "description": "Testing credits"}'
```

## Common Pitfalls (CRITICAL)

### ❌ NEVER do this:
```rust
// WRONG: Explicit transactions fail on PgBouncer
let mut tx = pool.begin().await?;
let balance = get_balance_with_tx(&tx, wallet_id).await?;
update_balance(&tx, wallet_id, new_balance).await?;
tx.commit().await?;  // <-- FAILS on Neon PgBouncer
```

### ✅ ALWAYS do this:
```rust
// CORRECT: Single atomic operation
charge(pool, wallet_id, amount, tx_type, description, related_id).await?;
```

### ❌ NEVER do this:
```rust
// WRONG: Check-then-charge race condition
let balance = get_balance(pool, wallet_id).await?;
if balance >= amount {
    charge(pool, wallet_id, amount, tx_type, description, related_id).await?;
}
```

### ✅ ALWAYS do this:
```rust
// CORRECT: Charge handles balance check atomically
charge(pool, wallet_id, amount, tx_type, description, related_id)
    .await
    .map_err(|e| match e {
        AuthError::InvalidInput(msg) if msg.contains("Insufficient") => {
            (StatusCode::PAYMENT_REQUIRED, msg)
        },
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;
```

### ❌ NEVER do this:
```rust
// WRONG: Charging after work (can't rollback)
perform_expensive_operation(pool, &req).await?;
charge(pool, wallet_id, cost, tx_type, description, related_id).await?;
```

### ✅ ALWAYS do this:
```rust
// CORRECT: Charge first (fail fast)
charge(pool, wallet_id, cost, tx_type, description, related_id).await?;
perform_expensive_operation(pool, &req).await?;
```

## Monitoring & Alerting

### Critical metrics to monitor:

1. **Balance integrity**:
   ```sql
   SELECT wallet_id, balance, total_deposited, total_spent,
          (total_deposited - total_spent) AS calculated_balance
   FROM wallets
   WHERE balance != (total_deposited - total_spent);
   ```
   Should return 0 rows. Alert if any discrepancies.

2. **Negative balances**:
   ```sql
   SELECT * FROM wallets WHERE balance < 0;
   ```
   Should return 0 rows. Critical alert if any exist.

3. **Ledger completeness**:
   ```sql
   SELECT w.wallet_id, w.balance,
          COALESCE(SUM(l.amount), 0) AS ledger_sum
   FROM wallets w
   LEFT JOIN credit_ledger l ON w.wallet_id = l.wallet_id
   GROUP BY w.wallet_id, w.balance
   HAVING w.balance != COALESCE(SUM(l.amount), 0);
   ```
   Should return 0 rows. Alert if ledger/wallet mismatch.

4. **Failed charges** (monitor application logs):
   - Track `AuthError::InvalidInput("Insufficient balance")` frequency
   - Alert if spike indicates users hitting credit limits

## Security Considerations

1. **User-scoped operations**: Always validate `principal.user_id()` matches resource owner
2. **Workspace operations**: Verify team membership before charging workspace wallet
3. **Admin operations**: Require `principal.can_admin()` for grants/refunds
4. **Rate limiting**: Implement per-user rate limits on credit-consuming operations
5. **Audit logging**: All ledger entries are immutable and timestamped for forensics

## Migration Strategy

When adding new transaction types:

1. Add to CHECK constraint in migration:
   ```sql
   ALTER TABLE credit_ledger DROP CONSTRAINT credit_ledger_tx_type_check;
   ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
       CHECK (tx_type IN ('existing_types', ..., 'new_type'));
   ```

2. Update `docs/GLOSSARY.md` with new type definition

3. Update this document with usage pattern

## Support & Troubleshooting

### User reports "insufficient credits" but shows balance
1. Check for concurrent operations (race condition)
2. Verify balance via SQL: `SELECT balance FROM wallets WHERE owner_id = $user_id`
3. Check recent ledger: `SELECT * FROM credit_ledger WHERE wallet_id = $wallet_id ORDER BY created_at DESC LIMIT 10`
4. If discrepancy found, investigate and grant compensatory credits

### Wallet balance mismatch
1. Run integrity query (see Monitoring section)
2. Compare wallet.balance with SUM(ledger.amount)
3. If mismatch, create incident report and manual correction via admin grant
4. Investigate root cause in application logs

### Missing ledger entries
1. Check for failed charge() calls in application logs
2. Verify transaction completed (check related resource created)
3. If resource exists but no ledger entry, indicates bug - file critical issue
4. Compensate with admin refund if user was overcharged

---

**Last updated**: 2026-02-11  
**Maintained by**: Platform Team  
**Critical system**: Changes require senior review + audit
