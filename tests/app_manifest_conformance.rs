//! An App manifest field the spawner does not read is decoration.
//!
//! Every check here exists because `adaptogen_lab_regulatory` violated it and
//! nothing said so. That App had a working UI, a registered route, an
//! auto-hired agent named in its manifest and four declared actions — and
//! spawning it produced an empty workspace with no files and no agents,
//! because four separate fields were in the wrong place or the wrong shape.
//! None of it errored. `serde_json::Value` accepts any key, `filter_map` drops
//! what it cannot parse, and `ON CONFLICT DO UPDATE` overwrites with whatever
//! sorted last.
//!
//! The failures were only visible by reading `handlers::apps::spawn` and
//! comparing it to the JSON by eye. These tests are that comparison, run.

use std::collections::HashMap;

/// Manifests, from both layouts `seed_apps_to_database` walks: flat
/// `apps/*.json` and nested `apps/*/manifest.json`, `apps/*/*/manifest.json`
/// (`api_server.rs` `find_manifests`).
fn manifests() -> Vec<(String, serde_json::Value)> {
    fn collect(dir: &str, depth: usize, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                collect(&p.to_string_lossy(), depth + 1, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("json")
                && (depth == 0 || p.file_name().map(|n| n == "manifest.json").unwrap_or(false))
            {
                out.push(p);
            }
        }
    }
    let mut paths = Vec::new();
    collect("apps", 0, &mut paths);
    paths.sort();

    paths
        .into_iter()
        .filter_map(|p| {
            let raw = std::fs::read_to_string(&p).ok()?;
            let v: serde_json::Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", p.display()));
            Some((p.to_string_lossy().to_string(), v))
        })
        .collect()
}

fn slug(v: &serde_json::Value) -> &str {
    v["slug"].as_str().unwrap_or("<no slug>")
}

/// **Two manifests, one slug, and the loser is chosen by string sort.**
///
/// `seed_apps_to_database` upserts `ON CONFLICT (slug) DO UPDATE SET` over
/// every column, walking `all_paths` after `sort()`. So when two files declare
/// the same slug the alphabetically later one wins silently and completely.
///
/// This is not hypothetical. `apps/adaptogen_lab_regulatory.json` and
/// `apps/adaptogen-lab/regulatory-lens/manifest.json` both declared
/// `adaptogen_lab_regulatory`. `-` (0x2D) sorts before `_` (0x5F), so the flat
/// file was applied last, and it carried `composition_slug: null`, three
/// action types instead of four, and an empty `workspace_template`. Every edit
/// made to the nested manifest — the one with the App's actual content — was
/// overwritten in the database on the next boot.
#[test]
fn no_two_manifests_declare_the_same_slug() {
    let mut by_slug: HashMap<String, Vec<String>> = HashMap::new();
    for (path, m) in manifests() {
        by_slug.entry(slug(&m).to_string()).or_default().push(path);
    }
    let dupes: Vec<_> = by_slug.iter().filter(|(_, v)| v.len() > 1).collect();
    assert!(
        dupes.is_empty(),
        "these slugs are declared by more than one manifest: {dupes:#?}\n\n\
         The seeder upserts on slug and applies paths in sort order, so the \
         alphabetically last file wins every column and the other is dead \
         weight that looks live. Delete one."
    );
}

/// `auto_hire` belongs to `workspace_template`, which is where it is read.
///
/// `handlers/apps.rs:487-497` reads `template["auto_hire"]`. A top-level
/// `auto_hire` is read by nothing, and that is exactly how
/// `adaptogen_lab_regulatory` shipped: it named `regulatory_lens_translator`
/// as its fleet, and spawning hired nobody. The App's entire purpose is an
/// agent that was never present.
#[test]
fn auto_hire_is_declared_where_the_spawner_reads_it() {
    for (path, m) in manifests() {
        assert!(
            m.get("auto_hire").is_none(),
            "{path}: `auto_hire` is declared at the top level, where nothing \
             reads it. The spawner reads `workspace_template.auto_hire` \
             (handlers/apps.rs:489). Move it."
        );
        let fleet = m["workspace_template"].get("auto_hire");
        assert!(
            fleet
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty()),
            "{path} ({}): no `workspace_template.auto_hire`. An App that hires \
             nobody spawns a workspace with no agents in it.",
            slug(&m)
        );
    }
}

/// Every hired agent must have a card on disk.
#[test]
fn every_auto_hired_agent_exists() {
    for (path, m) in manifests() {
        let fleet = m["workspace_template"]["auto_hire"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for a in fleet {
            let name = a.as_str().unwrap_or_default();
            let curated = format!("agents/curated/{name}/agent_card.json");
            let system = format!("agents/system/{name}/agent_card.json");
            assert!(
                std::path::Path::new(&curated).exists() || std::path::Path::new(&system).exists(),
                "{path} auto-hires `{name}`, which has no agent card. The hire \
                 fails at spawn and the workspace comes up short a member."
            );
        }
    }
}

/// **`initial_files` entries need `content`, and a missing one is dropped.**
///
/// `handlers/apps.rs:499-510`:
///
/// ```ignore
/// let path = f.get("path")?.as_str()?.to_string();
/// let content = f.get("content")?.as_str()?.to_string();
/// ```
///
/// Both are `?` inside a `filter_map`, so an entry without `content` is
/// silently skipped rather than reported. `adaptogen_lab_regulatory` declared
/// four files with `source:` — a path to copy from — which is a shape the
/// spawner has never supported. All four were dropped on every spawn, and the
/// manifest read as though the workspace were being seeded.
#[test]
fn every_initial_file_carries_inline_content() {
    for (path, m) in manifests() {
        let files = m["workspace_template"]["initial_files"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for f in files {
            let p = f
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<no path>");
            assert!(
                f.get("content").and_then(|v| v.as_str()).is_some(),
                "{path}: initial_files entry `{p}` has no string `content` \
                 (keys: {:?}). The spawner requires `path` AND `content` and \
                 drops anything else without a word — `source:` is not \
                 supported.",
                f.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
        }
    }
}

/// The document an App is *about* should exist in a workspace it spawns.
///
/// `schema_json.canonical_document.path` names the document every action reads
/// or writes. If the spawner does not seed it, the first thing a user sees is
/// an App whose subject is missing. All four Apps satisfy this; adaptogen only
/// began to once its `initial_files` were repaired, which is the evidence that
/// the check has teeth rather than describing an accident.
#[test]
fn the_canonical_document_is_seeded() {
    for (path, m) in manifests() {
        let Some(canon) = m["schema_json"]["canonical_document"]["path"].as_str() else {
            continue; // Not every App declares one.
        };
        let seeded: Vec<&str> = m["workspace_template"]["initial_files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.get("path").and_then(|v| v.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            seeded.contains(&canon),
            "{path} ({}): canonical_document `{canon}` is not in \
             initial_files {seeded:?}. A spawned workspace would not contain \
             the document the App exists to edit.",
            slug(&m)
        );
    }
}

/// A declared action endpoint must be a route that exists.
///
/// Shrink-only. `kask_wild` declares `actions/update_goal` and no such route
/// is registered — a real finding this test surfaced rather than caused, left
/// as recorded debt so it cannot grow. Remove entries; never add one.
#[test]
fn declared_action_endpoints_are_routed() {
    /// `(slug, action_type)` pairs whose endpoint is not routed.
    const UNROUTED: &[(&str, &str)] = &[("kask_wild", "update_goal")];

    let routes = std::fs::read_to_string("src/api_server.rs").expect("read api_server.rs");
    let mut dangling: Vec<String> = Vec::new();

    for (path, m) in manifests() {
        let s = slug(&m).to_string();
        let actions = m["schema_json"]["action_types"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for a in actions {
            let ty = a.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let Some(ep) = a.get("api_endpoint").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(idx) = ep.find("/actions/") else {
                continue;
            };
            let name = ep[idx + "/actions/".len()..]
                .trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
            if routes.contains(&format!("/actions/{name}\"")) {
                continue;
            }
            if UNROUTED.contains(&(s.as_str(), ty)) {
                continue;
            }
            dangling.push(format!("{path} ({s}): {ty} -> {ep}"));
        }
    }

    assert!(
        dangling.is_empty(),
        "action endpoint(s) declared with no route registered in \
         api_server.rs:\n  {}\n\nAn App advertising an action the platform \
         cannot serve is a 404 the manifest promised. Register the route, or \
         remove the action.",
        dangling.join("\n  ")
    );
}
