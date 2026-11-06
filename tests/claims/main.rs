//! SPEC 36 claim tests: each test is the "Done when" of one requirement.
//!
//! Run: `ABW_TEST_DATABASE_URL=… cargo test --test claims -- --test-threads=1`
//!
//! The URL is read from `ABW_TEST_DATABASE_URL` **only** — never from
//! `DATABASE_URL` or `.env` (issue #28: tests that wrote to whatever `.env`
//! pointed at). Tests create uniquely named rows and delete what they created.
//! Without the variable every DB test is skipped with a message, not passed.

mod support;

mod app_spawn_funding;

mod deployed;
