# Testing Strategy

**Date:** 2026-02-07  
**Status:** Foundation established  
**Target Coverage:** 70%+

---

## Current State

**Coverage:** ~1% (estimated)  
**Tests Written:** ~20 placeholder/unit tests  
**CI/CD:** GitHub Actions configured  
**Test Database:** PostgreSQL via GitHub Actions services

---

## Testing Pyramid

```
         ┌─────────────────┐
         │   E2E Tests     │  (5% - Manual for now)
         │   User flows    │
         └─────────────────┘
              ▲
         ┌────────────────────┐
         │ Integration Tests  │  (25% - API, DB, Services)
         │  API endpoints     │
         │  Database ops      │
         └────────────────────┘
              ▲
         ┌──────────────────────────┐
         │     Unit Tests           │  (70% - Logic, Types, Utils)
         │  Business logic          │
         │  Data structures         │
         │  Pure functions          │
         └──────────────────────────┘
```

---

## Test Categories

### 1. Unit Tests (70% of tests)

**What to test:**
- Core types (Episode, SemanticRule, Entity)
- Utility functions
- Business logic
- Validation rules
- Error handling

**Location:** `src/**/*.rs` (inline with `#[cfg(test)]`)

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_episode_creation() {
        let episode = Episode::new(...);
        assert_eq!(episode.agent_id, expected_id);
    }
}
```

### 2. Integration Tests (25% of tests)

**What to test:**
- API endpoints
- Database operations
- Service interactions
- External API mocks

**Location:** `tests/*.rs` (separate directory)

**Example:**
```rust
#[tokio::test]
#[ignore] // Requires database
async fn test_store_episode() {
    let store = MemoryStore::new(&db_url).await?;
    let id = store.store_episode(episode).await?;
    assert!(id.is_some());
}
```

### 3. E2E Tests (5% of tests)

**What to test:**
- Complete user workflows
- UI interactions
- Agent execution end-to-end

**Location:** `e2e/*.rs` or manual testing

**Status:** Manual for now, automate later

---

## Test Structure

### fermi-memory
```
fermi-memory/
├── src/
│   ├── lib.rs          # Module tests
│   ├── types.rs        # Type tests (#[cfg(test)])
│   ├── store.rs        # Store tests (#[cfg(test)])
│   └── error.rs        # Error tests
├── tests/
│   └── integration_tests.rs  # DB integration tests
└── examples/
    └── test_connection.rs    # Manual smoke tests
```

### Agent Bestiary API
```
fermi/
├── src/
│   └── api_server.rs   # Needs refactoring for testability
├── tests/
│   └── api_tests.rs    # API endpoint tests
└── Cargo.toml
```

---

## Testing Tools

### Current Stack
- `cargo test` - Test runner
- `tokio::test` - Async test runtime
- `assert!` - Standard assertions
- `#[ignore]` - Skip tests requiring external resources

### To Add
- `mockall` - Mocking framework
- `proptest` - Property-based testing
- `criterion` - Benchmarking
- `insta` - Snapshot testing (for JSON responses)

---

## Database Testing

### Test Database Setup

**GitHub Actions:**
- PostgreSQL 15 service container
- Auto-created `fermi_test` database
- Schema applied before tests run

**Local Development:**
```bash
# Create test database
createdb fermi_test

# Run schema
psql fermi_test < docs/agent-bestiary/MEMORY_SCHEMA.sql

# Run tests
TEST_DATABASE_URL=postgresql://localhost/fermi_test cargo test -- --ignored
```

### Database Test Patterns

**1. Transaction Rollback (Ideal)**
```rust
#[tokio::test]
async fn test_with_rollback() {
    let mut tx = pool.begin().await?;
    // ... test operations ...
    tx.rollback().await?; // Clean up
}
```

**2. Test Data Cleanup**
```rust
#[tokio::test]
async fn test_with_cleanup() {
    let id = create_test_data().await?;
    // ... test ...
    delete_test_data(id).await?;
}
```

**3. Isolated Test Database**
- Use separate database per test (slow but clean)
- Good for critical tests

---

## CI/CD Pipeline

### GitHub Actions Workflow

**On Push/PR:**
1. ✅ Checkout code
2. ✅ Install Rust 1.85
3. ✅ Cache dependencies
4. ✅ Start PostgreSQL service
5. ✅ Run schema migration
6. ✅ Run unit tests (no DB)
7. ✅ Run integration tests (with DB)
8. ✅ Run clippy (linter)
9. ✅ Check formatting
10. ✅ Security audit

**Status:** Pipeline configured, needs refinement

---

## Test Coverage Goals

### Phase 1 (Current - Week 1)
- [x] Test infrastructure setup
- [x] GitHub Actions CI/CD
- [ ] 20% coverage (critical paths)
- [ ] All new code has tests

### Phase 2 (Week 2-3)
- [ ] 40% coverage
- [ ] API endpoint tests
- [ ] Database operation tests
- [ ] Mocked external APIs

### Phase 3 (Week 4-6)
- [ ] 60% coverage
- [ ] E2E test framework
- [ ] Performance benchmarks
- [ ] Load testing

### Phase 4 (Week 7+)
- [ ] 70%+ coverage
- [ ] Comprehensive test suite
- [ ] Automated E2E tests
- [ ] Production monitoring

---

## What to Test First

### Priority 1 (This Week)
1. **fermi-memory core operations**
   - Episode CRUD
   - Semantic rule CRUD
   - Database connections

2. **API health checks**
   - /api/health endpoint
   - Database connectivity
   - Service availability

3. **Critical user paths**
   - List agents
   - View agent detail
   - Generate avatar (with mock)

### Priority 2 (Next Week)
1. **Authentication** (once implemented)
   - Login/logout
   - Token validation
   - Permission checks

2. **Agent execution**
   - Run agent
   - Store results
   - Error handling

3. **Data validation**
   - Input sanitization
   - Schema validation
   - Error responses

### Priority 3 (Later)
1. **Performance tests**
   - API response times
   - Database query speed
   - Memory usage

2. **Security tests**
   - SQL injection attempts
   - XSS attacks
   - Auth bypass attempts

---

## Testing Best Practices

### Do's ✅
- Write tests for new code
- Test error cases, not just happy path
- Use descriptive test names
- Keep tests fast (mock external calls)
- Run tests before committing
- Fix failing tests immediately

### Don'ts ❌
- Don't test implementation details
- Don't write flaky tests
- Don't skip tests (unless temporarily)
- Don't commit failing tests
- Don't mock everything (test some integration)
- Don't test framework code

---

## Mocking Strategy

### External Services to Mock
1. **Gemini API** (avatar generation)
   - Mock responses
   - Test error handling
   - Cache testing

2. **Database** (for unit tests)
   - Use real DB for integration tests
   - Mock for business logic tests

3. **File System** (ontology files)
   - Mock file reads
   - Test error cases

### Mocking Pattern
```rust
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
trait GeminiClient {
    async fn generate_image(&self, prompt: String) -> Result<Image>;
}

#[tokio::test]
async fn test_with_mock() {
    let mut mock = MockGeminiClient::new();
    mock.expect_generate_image()
        .returning(|_| Ok(fake_image()));
    // ... test with mock ...
}
```

---

## Test Data Management

### Test Fixtures
```rust
// tests/fixtures/mod.rs
pub fn sample_episode() -> Episode {
    Episode::new(
        Uuid::new_v4(),
        "Test query".to_string(),
        json!({"result": "test"}),
        ExecutionStatus::Success,
    )
}

pub fn sample_agent_card() -> Value {
    json!({
        "agent_id": "test_agent",
        "agent_type": "research",
        // ...
    })
}
```

### Test Database Seeding
```sql
-- tests/seed_test_data.sql
INSERT INTO agents (agent_id, agent_name, agent_type, ...)
VALUES ('test-agent-1', 'Test Agent', 'research', ...);
```

---

## Known Issues

### Current Blockers
1. **sqlx compile-time checking** - Blocked by Rust 1.85
   - Workaround: Use runtime queries for now
   - Will fix when upgrading to Rust 1.88+

2. **API server not testable**
   - Need to refactor api_server.rs
   - Extract app creation into function
   - Enable dependency injection

3. **No test database isolation**
   - Tests may interfere with each other
   - Need transaction rollback pattern

---

## Success Metrics

### Weekly Targets
- **Week 1:** 20% coverage, CI/CD working
- **Week 2:** 35% coverage, API tests complete
- **Week 3:** 50% coverage, DB tests complete
- **Week 4:** 65% coverage, E2E framework
- **Week 5+:** 70%+ coverage, comprehensive suite

### Quality Gates
- ❌ **Block merge if:**
  - Tests fail
  - Coverage decreases
  - Security vulnerabilities
  - Formatting issues

---

## Resources

### Documentation
- [Rust Book: Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [tokio::test](https://docs.rs/tokio/latest/tokio/attr.test.html)
- [cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html)

### Tools
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) - Coverage
- [cargo-nextest](https://nexte.st/) - Fast test runner
- [cargo-watch](https://github.com/watchexec/cargo-watch) - Auto-run tests

---

**Status:** Foundation complete, ready to build comprehensive test suite  
**Next:** Implement Priority 1 tests, refactor API for testability
