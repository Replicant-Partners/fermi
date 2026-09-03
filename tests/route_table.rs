//! # The route table has to be insertable, and `cargo check` cannot tell you
//!
//! `Router::route` panics at **construction**, not at compile time. So a bad
//! path is a green build, a green test suite, a successful deploy, and a
//! process that dies on boot — which is exactly what happened:
//!
//! ```text
//! thread 'main' panicked at src/api_server.rs:2709:10:
//! Invalid route "/a2a/:slug/message:stream": insertion failed due to
//! conflict with previously registered route: /a2a/:slug/message:send
//! ```
//!
//! The A2A REST transport names custom methods AIP-136 style, with a colon:
//! `message:send`, `message:stream`. But axum 0.7 routes through `matchit` 0.7,
//! where `:` opens a path parameter **anywhere in a segment**. Neither string
//! was a literal. `message:send` was the static text `message` followed by a
//! parameter named `send`; `message:stream` was the same text followed by a
//! parameter named `stream`. Two differently-named parameters in one slot is a
//! conflict.
//!
//! The single-route version is worse than the panic, and worth saying out loud:
//! **had only one of the two existed there would have been no error at all**,
//! and `/a2a/anything/messageWHATEVER` would have matched it. A silently
//! over-broad route is not something a reader of the source would ever suspect,
//! because the string looks like a literal path.
//!
//! So this file does the one thing a type checker cannot: it builds the router.

use axum::{routing::get, Router};

/// Every path literal passed to `.route(` in the API server, in source order.
///
/// Read out of the source rather than out of the app, because building the real
/// app needs an `AppState`, a database and a live pool — and a guard that needs
/// production credentials is a guard that does not run.
fn declared_paths() -> Vec<(usize, String)> {
    let src = include_str!("../src/api_server.rs");
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let Some(rest) = line.split_once(".route(") else {
            continue;
        };
        // `.route("/x", ...)` on one line, or `.route(` with the path on the
        // next. Both spellings appear in this file.
        let candidate = if rest.1.trim_start().starts_with('"') {
            rest.1.trim_start()
        } else {
            match src.lines().nth(i + 1) {
                Some(next) if next.trim_start().starts_with('"') => next.trim_start(),
                _ => continue,
            }
        };
        let body = &candidate[1..];
        let Some(end) = body.find('"') else { continue };
        out.push((i + 1, body[..end].to_string()));
    }
    out
}

/// The whole point: every declared path must be insertable together.
#[test]
fn the_router_builds() {
    let paths = declared_paths();
    assert!(
        paths.len() > 400,
        "only {} route path(s) found in src/api_server.rs — the scraper has \
         stopped matching and this guard is close to vacuous",
        paths.len()
    );

    // `/rabble` is nested, so its router is built separately and cannot collide
    // with the top-level table. Everything else shares one namespace.
    let mut router: Router<()> = Router::new();
    let mut seen: Vec<String> = Vec::new();

    for (line, path) in &paths {
        // axum panics on a duplicate path within one router, but the real file
        // registers several methods for one path across separate `.route`
        // calls, which is legal because they land in different `MethodRouter`s.
        // Dedupe so this test checks CONFLICTS rather than repetition.
        if seen.contains(path) {
            continue;
        }
        seen.push(path.clone());

        // `Router` holds an `UnsafeCell` internally so it is not `RefUnwindSafe`;
        // rebuild the accumulated prefix inside the closure instead of moving a
        // clone across the boundary. Quadratic and irrelevant at this size.
        let so_far: Vec<String> = seen.clone();
        let attempt = std::panic::catch_unwind(move || {
            let mut r: Router<()> = Router::new();
            for p in &so_far {
                r = r.route(p, get(|| async {}));
            }
            // Built to prove insertability; the value itself is not needed.
            let _ = r;
        });

        if attempt.is_err() {
            panic!(
                "src/api_server.rs:{line} declares `{path}`, which axum refuses \
                 to insert.\n\n\
                 If the path contains a `:` that is not at the start of a \
                 segment, that is the cause: matchit 0.7 opens a path parameter \
                 at any `:`, so `a:b` is the static text `a` followed by a \
                 parameter named `b`. Capture the whole segment and match on it \
                 in the handler — see `handlers::a2a::method_dispatch_handler`."
            );
        }
    }
    let _ = router;
}

/// The narrower rule, named, so the failure explains itself before anyone has
/// to reason about matchit's grammar.
///
/// Separate from `the_router_builds` because a single mid-segment colon does
/// **not** conflict and therefore builds fine while matching far more than its
/// author intended. That case has no symptom at all until a request arrives.
#[test]
fn no_route_hides_a_parameter_inside_a_segment() {
    let mut bad = Vec::new();
    for (line, path) in declared_paths() {
        for seg in path.split('/') {
            if seg.len() > 1 && seg[1..].contains(':') {
                bad.push(format!(
                    "src/api_server.rs:{line}  {path}  (segment `{seg}`)"
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "a `:` inside a path segment is a parameter, not a literal colon, so \
         these routes match far more than they appear to:\n  {}\n\n\
         matchit 0.7 has no escape for a literal colon; the `{{brace}}` syntax \
         that would allow one arrives with axum 0.8. Capture the segment whole \
         and compare it in the handler.",
        bad.join("\n  ")
    );
}
