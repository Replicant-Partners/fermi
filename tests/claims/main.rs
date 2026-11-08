//! SPEC 36 claim tests: each test is the "Done when" of one requirement.
//!
//! Run: `ABW_TEST_DATABASE_URL=… cargo test --test claims -- --test-threads=1`
//!
//! The URL is read from `ABW_TEST_DATABASE_URL` **only** — never from
//! `DATABASE_URL` or `.env` (issue #28: tests that wrote to whatever `.env`
//! pointed at). Tests create uniquely named rows and delete what they created.
//! Without the variable a DB test returns early and `cargo test` reports it as
//! `ok`; CI sets `CLAIMS_REQUIRED=1`, which turns that into a failure.
//! `deployed.rs` tests are `#[ignore]`d: run them with `-- --ignored` and
//! `ABW_TEST_BASE_URL` against a deployment.

mod support;

mod app_spawn_funding;

mod deployed;

mod deployed_happy_path;

mod a2a_task_tenancy;

mod episode_read_tenancy;

mod episode_owner_read;

mod ssrf_guard;

mod ssrf_deployed;

mod running_status;

mod utf8_truncation;

mod pulse_always_closes;

mod migration_ledger;

mod credit_holds;

mod tool_join;
