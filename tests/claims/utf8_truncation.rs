//! SPEC 36 R0.6 / LLD G06 (#42): truncation never splits a UTF-8 character.
//!
//! `&s[..N]` panics when byte `N` is inside a multi-byte character. On the
//! request path that is a remote panic: a tool result, a message or a bio
//! with a non-ASCII character at the cut fails the request
//! [V: `src/handlers/users.rs:202` @ 95cabef9 rendered `&b[..100]` of any
//! user's bio in `/api/users/search`].

use crate::deployed::base_url;
use crate::support;

/// R0.6 done-when: no fixed-width byte slice on a string in
/// `src/agent_backend`, and no `[..s.len().min(N)]` cut anywhere in the
/// server or auth crate. Comments and `#[cfg(test)] mod` blocks are ignored.
#[test]
fn no_byte_slice_truncation_on_strings() {
    let root = env!("CARGO_MANIFEST_DIR");
    let fixed = regex::Regex::new(r"\[\.\.[A-Z_0-9]+\]").unwrap();
    let len_min = regex::Regex::new(r"\[\.\.[A-Za-z_.]+\.len\(\)\.min\(").unwrap();
    let mut hits = Vec::new();
    for (dir, rxs) in [
        ("src/agent_backend", vec![&fixed, &len_min]),
        ("src", vec![&len_min]),
        ("fermi-auth/src", vec![&len_min]),
    ] {
        for (rel, src) in rust_files(root, dir) {
            for rx in &rxs {
                for m in rx.find_iter(&src) {
                    hits.push(format!("{rel}: {}", m.as_str()));
                }
            }
        }
    }
    hits.sort();
    hits.dedup();
    assert!(
        hits.is_empty(),
        "use fermi::text::truncate_at_char_boundary:\n{}",
        hits.join("\n")
    );
}

#[test]
fn truncation_backs_off_to_a_char_boundary() {
    let s = format!("{}\u{1F600}tail", "a".repeat(30));
    for n in 30..34 {
        assert_eq!(
            fermi::text::truncate_at_char_boundary(&s, n),
            "a".repeat(30)
        );
    }
}

/// Live: a user whose bio has a 2-byte character across byte 100 is found by
/// `/api/users/search`. Before R0.6 the snippet `&b[..100]` panicked and the
/// request returned 502.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_search_survives_a_multibyte_bio() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let searcher = support::make_user(&pool, &mut *c.lock().await).await;
        let target = support::make_user(&pool, &mut *c.lock().await).await;
        let needle = support::tag("utf8");
        // 99 ASCII bytes, then 'é' (2 bytes) occupying bytes 99..101.
        let bio = format!("{}\u{e9} and more", "b".repeat(99));
        sqlx::query("UPDATE users SET display_name = $1, bio = $2 WHERE user_id = $3")
            .bind(&needle)
            .bind(&bio)
            .bind(&target)
            .execute(&pool)
            .await
            .expect("set bio");
        let (key, info) = fermi_auth::api_keys::create_api_key(
            &pool,
            &searcher,
            "spec36 claims",
            &["write".to_string()],
        )
        .await
        .expect("api key");
        c.lock().await.api_keys.push(info.key_id);

        let r = reqwest::Client::new()
            .get(format!("{base}/api/users/search"))
            .query(&[("q", needle.as_str())])
            .bearer_auth(&key)
            .send()
            .await
            .expect("request");
        let status = r.status().as_u16();
        let body = r.text().await.unwrap_or_default();
        assert_eq!(status, 200, "search with a multi-byte bio: {body}");
        assert!(body.contains(&needle), "target found: {body}");
    })
    .await;
}

fn rust_files(root: &str, dir: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(format!("{root}/{dir}"))];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("tests.rs") {
                let rel = p.strip_prefix(root).unwrap().display().to_string();
                out.push((rel, production_only(&std::fs::read_to_string(&p).unwrap())));
            }
        }
    }
    out
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
