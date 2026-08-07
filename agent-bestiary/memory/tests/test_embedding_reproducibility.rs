// Spec 22 §1.8 — verify_reproducible CI test.
//
// Proves that the provenance captured per row is SUFFICIENT to reproduce
// the stored vector from its `source_text` alone. This is the spec's
// literal acceptance criterion for Phase 1.
//
// Two test variants:
//
//   1. `test_mock_embedder_round_trip` — fast PR-gate variant: uses
//      `MockEmbeddings` (deterministic hash) and an in-process round-trip
//      to prove that the (source_text, model_id, model_version, dim) tuple
//      reproduces the same vector. No DB or external API required.
//
//   2. `verify_reproducible_sample_against_db` — nightly variant: samples
//      trusted rows from `embedding_provenance` over the last 24h,
//      re-embeds their `source_text` with the production embedder, and
//      asserts cosine similarity ≥ 0.9999. Requires `DATABASE_URL` and the
//      production API key. Marked `#[ignore]` so it never runs in PR CI.
//
// The nightly variant runs via:
//   cargo test --test test_embedding_reproducibility -- --ignored

use agent_bestiary_memory::{EmbeddingGenerator, MockEmbeddings, ProvenancedEmbedding};

/// PR-gate variant: cheap in-process round trip on the Mock embedder.
///
/// This is the discipline check — if `generate_provenanced` is doing its
/// job, the bundled (model_id, model_version) MUST be sufficient to
/// reproduce the vector from `source_text` alone.
#[tokio::test]
async fn test_mock_embedder_round_trip() {
    let embedder = MockEmbeddings::new(1024);
    let texts = vec![
        "the quick brown fox jumps over the lazy dog",
        "a much shorter text",
        "yet another distinct query for diversity",
        "embedding portability is an insurance requirement",
    ];

    for text in &texts {
        // First write: pretend this is what got persisted.
        let written: ProvenancedEmbedding = embedder.generate_provenanced(text).await.unwrap();

        // Round-trip: re-derive the vector from the stored provenance fields.
        // The contract: a future caller (re-embed worker, verify job)
        // reconstructs an embedder matching (model_id, model_version) and
        // calls `generate(source_text)` — the result must match.
        assert_eq!(written.source_text, *text, "source_text round-trip");
        assert_eq!(written.dim, embedder.dimension() as i32, "dim invariant");
        assert_eq!(
            written.vector.len(),
            written.dim as usize,
            "vector length matches dim"
        );

        let regenerated = embedder.generate(&written.source_text).await.unwrap();
        let cos = cosine_similarity(&regenerated, &written.vector);
        assert!(
            cos >= 0.9999,
            "Mock embedder is deterministic; round-trip should yield identical vectors, \
             got cos={} for text={:?}",
            cos,
            text
        );
    }
}

/// Provenance bundling: every call site that uses `generate_provenanced`
/// receives a struct whose fields match the embedder's identity exactly.
#[tokio::test]
async fn test_provenance_fields_match_embedder_identity() {
    let embedder = MockEmbeddings::new(512);
    let p = embedder.generate_provenanced("test text").await.unwrap();
    assert_eq!(p.model_id, embedder.model_id());
    assert_eq!(p.model_version, embedder.model_version());
    assert_eq!(p.dim, embedder.dimension() as i32);
    assert_eq!(p.source_text, "test text");
}

/// Batch consistency: provenanced batches must align positionally with the
/// input texts and all share the same model identity.
#[tokio::test]
async fn test_provenanced_batch_alignment() {
    let embedder = MockEmbeddings::new(256);
    let texts: Vec<String> = (0..10).map(|i| format!("query number {}", i)).collect();
    let batch = embedder.generate_provenanced_batch(&texts).await.unwrap();
    assert_eq!(batch.len(), texts.len());
    for (i, p) in batch.iter().enumerate() {
        assert_eq!(p.source_text, texts[i]);
        assert_eq!(p.model_id, embedder.model_id());
        assert_eq!(p.dim, 256);
    }
}

// ─── Nightly DB-backed variant ──────────────────────────────────────
//
// This test connects to the production database and the production
// embedder. It samples up to 50 recent trusted rows from
// `embedding_provenance`, re-embeds their `source_text`, and asserts the
// result matches the stored vector. Skipped in PR CI via `#[ignore]`.

#[tokio::test]
#[ignore]
async fn verify_reproducible_sample_against_db() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        }
    };
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set (would call mock embedder)");
            return;
        }
    };

    use agent_bestiary_memory::AnthropicEmbeddings;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::Row;

    let embedder = AnthropicEmbeddings::new(api_key);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("failed to connect to database");

    let rows = sqlx::query(
        r#"
        SELECT target_table, target_id::text as target_id_str,
               source_text, model_id, model_version, dim, embedding
          FROM embedding_provenance
         WHERE trusted = true
           AND source_text IS NOT NULL
           AND embedding IS NOT NULL
           AND created_at > NOW() - INTERVAL '24 hours'
         ORDER BY random()
         LIMIT 50
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("failed to query embedding_provenance");

    if rows.is_empty() {
        eprintln!(
            "No recent trusted provenance samples to verify. \
             This is expected if the system has not been writing embeddings recently."
        );
        return;
    }

    let mut mismatches: Vec<(String, String, f32)> = Vec::new();
    let mut verified = 0;
    let total = rows.len();
    for row in &rows {
        let model_id: String = row.try_get("model_id").unwrap_or_default();
        if model_id != embedder.model_id() {
            continue;
        }
        let target_table: String = row.try_get("target_table").unwrap_or_default();
        let target_id_str: String = row.try_get("target_id_str").unwrap_or_default();
        let source_text: String = row.try_get("source_text").unwrap_or_default();
        let original_vec: Option<pgvector::Vector> = row.try_get("embedding").ok();
        let original: Vec<f32> = match original_vec {
            Some(v) => v.to_vec(),
            None => continue,
        };

        let regenerated = embedder
            .generate(&source_text)
            .await
            .expect("re-embedding failed");
        let cos = cosine_similarity(&regenerated, &original);
        verified += 1;
        if cos < 0.9999 {
            mismatches.push((target_table, target_id_str, cos));
        }
    }

    assert!(
        mismatches.is_empty(),
        "Spec 22 verify_reproducible: {} of {} verified rows could NOT be reproduced \
         from stored provenance: {:?}",
        mismatches.len(),
        verified,
        mismatches
    );
    eprintln!(
        "Spec 22 verify_reproducible: {} samples verified successfully ({} skipped due to model mismatch)",
        verified,
        total - verified
    );
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
