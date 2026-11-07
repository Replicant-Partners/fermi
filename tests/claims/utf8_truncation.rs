//! SPEC 36 R0.6 / LLD G06 (#42): truncation never splits a UTF-8 character.
//!
//! `&s[..N]` panics when byte `N` is inside a multi-byte character. On the
//! request path that is a remote panic: a tool result, a message or a bio
//! with a non-ASCII character at the cut fails the request
//! [V: `src/handlers/users.rs:202` @ 95cabef9 rendered `&b[..100]` of any
//! user's bio in `/api/users/search`].

use crate::deployed::base_url;
use crate::support;

/// R0.6 done-when, widened: no byte-offset cut of a string in server code
/// (`src/`, `fermi-auth/src`, `agent-bestiary/*/src`). Denied forms:
/// `[..CONST]` / `[..123]`, `[..x.len().min(`, `[..x - n]`, and
/// `.truncate(<number>)`. Each surviving site must be listed below with the
/// reason its input is ASCII (or not a string) and its exact count, so a new
/// site in a listed file still fails. Comments and `#[cfg(test)] mod` blocks
/// are ignored.
#[test]
fn no_byte_slice_truncation_on_strings() {
    let root = env!("CARGO_MANIFEST_DIR");
    let rxs = [
        regex::Regex::new(r"\[\.\.[A-Z_0-9]+\]").unwrap(),
        regex::Regex::new(r"\[\.\.[A-Za-z_.]+\.len\(\)\.min\(").unwrap(),
        regex::Regex::new(r"\[\.\.[a-z_]+ - [0-9]+\]").unwrap(),
        regex::Regex::new(r"\.truncate\([0-9][0-9_]*\)").unwrap(),
    ];
    // (file, hits, why the input is ASCII or not a string)
    let allowed: &[(&str, usize, &str)] = &[
        ("src/handlers/rabble_workspace.rs", 1, "UUID string prefix"),
        ("src/handlers/apps.rs", 1, "UUID string prefix"),
        (
            "src/handlers/auth.rs",
            1,
            "hex address: fermi-auth/src/siwe.rs:161 formats a 20-byte address",
        ),
        ("src/calibration.rs", 1, "Vec slice, not a string"),
        (
            "fermi-auth/src/api_keys.rs",
            1,
            "create_api_key: platform-generated ASCII key",
        ),
        (
            "agent-bestiary/memory/src/seed.rs",
            3,
            "Vec slices, not strings",
        ),
        ("agent-bestiary/ontology/src/git.rs", 1, "git SHA hex"),
        (
            "agent-bestiary/ontology/src/workspace_git.rs",
            2,
            "git SHA hex",
        ),
        (
            "src/apps/workspace_fork.rs",
            1,
            "slug built from [a-z0-9_] only",
        ),
        ("src/handlers/admin.rs", 1, "slug built from [a-z0-9_] only"),
        ("src/agent_backend/kg_context.rs", 2, "Vec::truncate"),
        (
            "src/agent_backend/tools/domains/biology.rs",
            1,
            "Vec::truncate",
        ),
    ];
    let mut dirs = vec!["src".to_string(), "fermi-auth/src".to_string()];
    for e in std::fs::read_dir(format!("{root}/agent-bestiary"))
        .unwrap()
        .flatten()
    {
        if e.path().join("src").is_dir() {
            dirs.push(format!(
                "agent-bestiary/{}/src",
                e.file_name().to_string_lossy()
            ));
        }
    }
    let mut counts: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for dir in &dirs {
        for (rel, src) in rust_files(root, dir) {
            if rel.starts_with("src/bin/") && rel != "src/bin/agent-mcp-server.rs" {
                continue;
            }
            for rx in &rxs {
                for m in rx.find_iter(&src) {
                    counts
                        .entry(rel.clone())
                        .or_default()
                        .push(m.as_str().to_string());
                }
            }
        }
    }
    let mut problems = Vec::new();
    for (file, hits) in &counts {
        match allowed.iter().find(|(f, _, _)| f == file) {
            Some((_, n, _)) if *n == hits.len() => {}
            Some((_, n, why)) => problems.push(format!(
                "{file}: {} cut(s), {n} allowed ({why}): {hits:?}",
                hits.len()
            )),
            None => problems.push(format!("{file}: {hits:?}")),
        }
    }
    for (f, _, _) in allowed {
        if !counts.contains_key(*f) {
            problems.push(format!("{f}: allowlisted but has no cut; remove it"));
        }
    }
    assert!(
        problems.is_empty(),
        "use fermi::text::truncate_at_char_boundary (or list the site with its reason):\n{}",
        problems.join("\n")
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
