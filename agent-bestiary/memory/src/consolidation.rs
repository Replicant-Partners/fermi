//! Consolidation workflow orchestration
//!
//! This module implements the Active Dreaming Memory consolidation process:
//! 1. Acquire lock for agent
//! 2. Fetch unconsolidated episodes
//! 3. Cluster failure episodes using DBSCAN
//! 4. Extract semantic rules from clusters
//! 5. Extract entities and facts from episodes
//! 6. Store consolidated knowledge
//! 7. Mark episodes as consolidated
//! 8. Update job statistics

use crate::{
    generate_structured_with_usage, Cardinality, ConsolidationLock, DBSCANClustering,
    EmbeddingGenerator, Entity, Episode, EpisodeCluster, ExecutionStatus, ExtractionFloor, Fact,
    GenerationConfig, LLMProvider, MemoryError, MemoryStore, Message, MessageRole,
    ProvenanceOracle, ProvenancedEmbedding, Result, SemanticRule, VerificationStatus,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

/// Append the extractor's learned rules to an extraction system prompt.
///
/// A free function so the composition can be tested without a database, a
/// worker, or a model. The precedence line is load-bearing: these rules are the
/// extractor's own generalisations about its past behaviour, and without an
/// explicit ordering a stale self-derived rule could quietly override the
/// task instructions it was supposed to refine.
fn compose_system_prompt(base: &str, guidance: Option<&str>) -> String {
    match guidance.map(str::trim).filter(|g| !g.is_empty()) {
        Some(g) => format!(
            "{base}\n\n\
             ## What you have learned about extraction\n\n\
             These are rules you derived from your own past extraction cycles and that \
             survived into your knowledge graph. Apply them. If one conflicts with the \
             instructions above, the instructions above win.\n\n{g}"
        ),
        None => base.to_string(),
    }
}

/// The extractor's own learned rules, rendered for injection into its system
/// prompt. `None` when it has learned nothing yet.
///
/// Lives here rather than in a handler because it has two callers on opposite
/// sides of the crate split — the API consolidation handler and the batch
/// `consolidate` CLI — and a read-back that one path performs and the other
/// skips is worse than none: the extractor would behave differently depending
/// on which entry point happened to invoke it, and nothing would say so.
///
/// SELECTION
///
/// Verified rules first, then by confidence. Verification does not run yet
/// (`rules_verified`/`rules_rejected` are hardcoded 0 below, and
/// `update_semantic_rule_verification` has no production caller), so today this
/// is effectively "highest-confidence active rules". The ordering is written to
/// prefer verified ones the moment that changes.
///
/// Capped at `limit`. This text rides on EVERY extraction call in a cycle — one
/// per cluster plus entity and fact batches — so an uncapped preamble would
/// grow the per-cycle bill linearly in everything the extractor has ever
/// learned.
pub async fn extractor_self_knowledge(
    pool: &sqlx::PgPool,
    extractor_id: Uuid,
    limit: i64,
) -> Option<String> {
    use sqlx::Row as _;

    let rows = sqlx::query(
        "SELECT rule_content, rule_description, confidence_score, verification_status
           FROM semantic_rules
          WHERE agent_id = $1 AND is_active AND invalidated_at IS NULL
          ORDER BY (verification_status = 'verified') DESC, confidence_score DESC
          LIMIT $2",
    )
    .bind(extractor_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .ok()?;

    let mut out = String::new();
    for r in &rows {
        let content: String = r.try_get("rule_content").unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let status: String = r.try_get("verification_status").unwrap_or_default();
        let conf: f64 = r.try_get("confidence_score").unwrap_or(0.0);
        // The model is told what each rule is worth. An unverified rule is the
        // extractor's own untested hypothesis about its own behaviour, and
        // presenting that with the same authority as a verified one is how a
        // guess gets laundered into a constraint.
        out.push_str(&format!("- [{status}, confidence {conf:.2}] {content}\n"));
        if let Ok(Some(d)) = r.try_get::<Option<String>, _>("rule_description") {
            if !d.trim().is_empty() {
                out.push_str(&format!("    ({})\n", d.trim()));
            }
        }
    }

    (!out.trim().is_empty()).then_some(out)
}

/// What the extraction model was asked to do during one cycle.
///
/// Accumulated across every `generate_structured_with_usage` call the worker
/// makes, so the cycle can report what the extractor actually cost. Before
/// this existed the ontologist ran several times per cycle and left no trace
/// of any of it.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExtractorUsage {
    /// Number of completed LLM round-trips (successful or parse-failed).
    pub calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl ExtractorUsage {
    fn record(&mut self, u: &crate::TokenUsage) {
        self.calls += 1;
        self.prompt_tokens += u.prompt_tokens as u64;
        self.completion_tokens += u.completion_tokens as u64;
        self.total_tokens += u.total_tokens as u64;
    }
}

/// Consolidation workflow orchestrator
pub struct ConsolidationWorker {
    store: Arc<MemoryStore>,
    lock: Arc<ConsolidationLock>,
    embedder: Arc<dyn EmbeddingGenerator>,
    llm: Option<Arc<dyn LLMProvider>>,
    worker_id: String,
    /// Extraction cost for the cycle in flight. `Mutex` rather than threading a
    /// counter through five private methods; contention is nil because a worker
    /// runs one cycle at a time under `ConsolidationLock`.
    usage: std::sync::Mutex<ExtractorUsage>,
    /// What the extractor has learned about extracting, injected into every
    /// system prompt this cycle. See [`ConsolidationWorker::with_extractor_guidance`].
    extractor_guidance: Option<String>,
    /// Who to credit for rules produced this cycle.
    /// See [`ConsolidationWorker::with_extractor_identity`].
    extractor_identity: Option<Uuid>,
    /// Resolves how well-grounded the source episodes were.
    /// See [`ConsolidationWorker::with_provenance_oracle`].
    provenance_oracle: Option<Arc<dyn ProvenanceOracle>>,
}

impl ConsolidationWorker {
    /// Creates a new consolidation worker
    pub fn new(
        store: Arc<MemoryStore>,
        lock: Arc<ConsolidationLock>,
        embedder: Arc<dyn EmbeddingGenerator>,
        worker_id: String,
    ) -> Self {
        Self {
            store,
            lock,
            embedder,
            llm: None,
            worker_id,
            usage: std::sync::Mutex::new(ExtractorUsage::default()),
            extractor_guidance: None,
            extractor_identity: None,
            provenance_oracle: None,
        }
    }

    /// Creates a new consolidation worker with LLM support
    pub fn with_llm(
        store: Arc<MemoryStore>,
        lock: Arc<ConsolidationLock>,
        embedder: Arc<dyn EmbeddingGenerator>,
        llm: Arc<dyn LLMProvider>,
        worker_id: String,
    ) -> Self {
        Self {
            store,
            lock,
            embedder,
            llm: Some(llm),
            worker_id,
            usage: std::sync::Mutex::new(ExtractorUsage::default()),
            extractor_guidance: None,
            extractor_identity: None,
            provenance_oracle: None,
        }
    }

    /// Whom to credit for the rules this cycle produces (migration 201).
    ///
    /// Rules are stored under the SUBJECT agent, because that is who will use
    /// them. Nothing recorded the author, so the extractor could not be
    /// evaluated on a single rule it had ever written — the signal half of its
    /// own Loop 1 had no data source at all.
    ///
    /// Left `None` when the caller cannot identify the extractor. `None` means
    /// "author unrecorded", never "no author", and readers must exclude it
    /// rather than attributing it to anyone.
    pub fn with_extractor_identity(mut self, agent_id: Option<Uuid>) -> Self {
        self.extractor_identity = agent_id;
        self
    }

    /// How to find out how well-grounded the evidence was (migration 203).
    ///
    /// The rules this worker writes do not stay in `semantic_rules`. They are
    /// retrieved and injected into other agents' prompts, which makes them
    /// things the platform tells its own agents are true. A rule extracted
    /// from ten tool-verified lookups and a rule extracted from ten paragraphs
    /// of model prose are otherwise stored identically and retrieved
    /// identically — and the second is worse than a bare hallucination,
    /// because its citation is real: `source_episode_cluster` genuinely points
    /// at episodes that genuinely said that.
    ///
    /// The oracle lives in the upper crate because the field contracts do. See
    /// [`crate::provenance`] for why this is a trait and not a function.
    ///
    /// `None` — no oracle wired, as in tests — means every rule this cycle
    /// writes records an UNKNOWN floor. Not a clean one. The distinction is
    /// the whole point of the column.
    pub fn with_provenance_oracle(mut self, oracle: Option<Arc<dyn ProvenanceOracle>>) -> Self {
        self.provenance_oracle = oracle;
        self
    }

    /// The provenance floor for a rule about to be written from `episode_ids`.
    ///
    /// One helper rather than three inlined copies, because there are three
    /// rule-construction sites and a fourth will be added. A site that forgot
    /// to call this would write `None` — which is the safe direction, but
    /// silently, and would show up in reporting as missing coverage rather
    /// than as a bug.
    ///
    /// An oracle error is UNKNOWN, never clean: a database hiccup must not be
    /// able to promote a rule's grounding, and consolidation must not fail
    /// because the floor could not be computed. The reason is recorded so the
    /// two cases stay distinguishable in the basis column.
    async fn floor_for(&self, episode_ids: &[Uuid]) -> ExtractionFloor {
        let Some(oracle) = self.provenance_oracle.as_ref() else {
            // Loud on purpose. A cycle that writes rules nobody can grade is
            // a cycle whose output will be injected into prompts marked
            // "grounding unknown" forever, and the only place that decision
            // is visible is here.
            tracing::warn!(
                worker = %self.worker_id,
                "no provenance oracle wired: every rule this cycle writes will \
                 record an UNKNOWN grounding floor"
            );
            return ExtractionFloor::unknown("no_provenance_oracle_wired");
        };
        match oracle.extraction_floor(episode_ids).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    sources = episode_ids.len(),
                    "provenance floor unresolved; recording UNKNOWN rather than \
                     assuming clean"
                );
                ExtractionFloor::unknown("oracle_error")
            }
        }
    }

    /// Give the extractor back what it has learned about extracting.
    ///
    /// THIS IS THE READ-BACK HALF OF LOOP 1 FOR THE EXTRACTOR ITSELF.
    ///
    /// Ordinary agents get their learned rules re-injected by
    /// `enrich_with_kg_context`, which runs in the HTTP handlers. Consolidation
    /// does not go through a handler — it is handed a bare `LLMProvider` and
    /// calls `generate_raw` — so the extractor was the one agent on the
    /// platform structurally incapable of consulting its own memory. It could
    /// accumulate a perfect ontology and behave identically to one that had
    /// never dreamt, which is precisely the failure `kg_context.rs` documents
    /// having fixed for everybody else.
    ///
    /// The caller supplies the text because retrieval lives in the upper crate.
    /// Deliberately NOT similarity-retrieved against the episodes being
    /// consolidated: the extractor's rules are *procedural* — lessons about how
    /// to extract well — not facts about the subject's domain, so matching them
    /// against the subject's episode content would be a category error. A
    /// stable per-cycle preamble is the right shape.
    pub fn with_extractor_guidance(mut self, guidance: Option<String>) -> Self {
        self.extractor_guidance = guidance.filter(|g| !g.trim().is_empty());
        self
    }

    /// Build the system message for an extraction call, appending whatever the
    /// extractor has learned. One helper rather than four inlined copies, so a
    /// new extraction path cannot silently skip the read-back.
    fn system_message(&self, base: &str) -> Message {
        Message {
            role: MessageRole::System,
            content: compose_system_prompt(base, self.extractor_guidance.as_deref()),
        }
    }

    /// Record one extraction round-trip. Poisoned-lock safe: a lost token count
    /// must never abort a consolidation cycle.
    fn record_usage(&self, u: &crate::TokenUsage) {
        if let Ok(mut g) = self.usage.lock() {
            g.record(u);
        }
    }

    /// Extraction cost accumulated so far this cycle.
    pub fn extractor_usage(&self) -> ExtractorUsage {
        self.usage.lock().map(|g| *g).unwrap_or_default()
    }

    /// The model the extractor is running, when one is configured.
    pub fn extractor_model(&self) -> Option<String> {
        self.llm.as_ref().map(|l| l.model_name().to_string())
    }

    /// Runs consolidation for a specific agent, creating its own job record.
    pub async fn consolidate_agent(
        &self,
        agent_id: Uuid,
        epsilon: f64,
        min_samples: usize,
    ) -> Result<ConsolidationResult> {
        self.consolidate_agent_with_job(agent_id, epsilon, min_samples, None)
            .await
    }

    /// Runs consolidation, optionally recording progress against a job row the
    /// caller already created.
    ///
    /// An async HTTP caller has to hand the client a job id *before* the work
    /// begins. If the worker then invents its own id, every status poll looks
    /// up a row that does not exist: the consolidation succeeds, the job row is
    /// written and completed correctly under the worker's id, and the client
    /// sees nothing at all. Passing `Some(job_id)` keeps the client's receipt
    /// and the worker's bookkeeping on the same row.
    pub async fn consolidate_agent_with_job(
        &self,
        agent_id: Uuid,
        epsilon: f64,
        min_samples: usize,
        job_id: Option<Uuid>,
    ) -> Result<ConsolidationResult> {
        // Step 1: Acquire lock
        let acquired = self.lock.acquire(agent_id, 30).await?;
        if !acquired {
            return Err(MemoryError::LockUnavailable(format!(
                "Could not acquire lock for agent {}",
                agent_id
            )));
        }

        // Ensure lock is released even if we error
        let result = self
            .consolidate_agent_internal(agent_id, epsilon, min_samples, job_id)
            .await;

        // Release lock
        self.lock.release(agent_id).await?;

        result
    }

    /// Successful episodes ordered by authority, capped at `budget`.
    ///
    /// Extraction can only afford to send a bounded number of episodes to the
    /// LLM. Which ones get dropped is a correctness question, not an
    /// arbitrary one: a HITL correction carries `authority_weight = 1.0` and
    /// represents a human decision that passed the coherence gate, while an
    /// ordinary successful run carries 0.5. Truncating by recency alone threw
    /// away the former whenever the agent had been busy since.
    ///
    /// Stable sort, so within an authority band the caller's order (newest
    /// first, from `get_unconsolidated_episodes`) is preserved.
    fn rank_success_episodes_by_authority(episodes: &[Episode], budget: usize) -> Vec<&Episode> {
        let mut ranked: Vec<&Episode> = episodes
            .iter()
            .filter(|e| matches!(e.execution_status, ExecutionStatus::Success))
            .collect();
        ranked.sort_by(|a, b| {
            b.authority_weight
                .partial_cmp(&a.authority_weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(budget);
        ranked
    }

    async fn consolidate_agent_internal(
        &self,
        agent_id: Uuid,
        epsilon: f64,
        min_samples: usize,
        existing_job_id: Option<Uuid>,
    ) -> Result<ConsolidationResult> {
        // Step 2: Fetch unconsolidated episodes
        let episodes = self.store.get_unconsolidated_episodes(agent_id).await?;

        if episodes.is_empty() {
            return Ok(ConsolidationResult::default());
        }

        let episode_ids: Vec<Uuid> = episodes.iter().map(|e| e.episode_id).collect();

        // Step 3: Create consolidation job — or adopt the caller's, so the id
        // the client is polling is the id this run reports against.
        let job_id = match existing_job_id {
            Some(id) => {
                // Idempotent insert: the caller normally created the row
                // already, but adopting an id that has no row would put us back
                // to updating nothing.
                self.store
                    .create_consolidation_job_with_id(
                        id,
                        agent_id,
                        episode_ids[0],
                        episode_ids[episode_ids.len() - 1],
                    )
                    .await?
            }
            None => {
                self.store
                    .create_consolidation_job(
                        agent_id,
                        episode_ids[0],
                        episode_ids[episode_ids.len() - 1],
                    )
                    .await?
            }
        };

        // Step 4: Cluster failure episodes
        let failure_episodes: Vec<Episode> = episodes
            .iter()
            .filter(|e| matches!(e.execution_status, crate::ExecutionStatus::Failure))
            .cloned()
            .collect();

        // Phase 3.2 (Spec 21): DBSCAN is O(n²·d) CPU work — move off the async
        // executor thread with spawn_blocking so the Tokio runtime stays free.
        let mut clusters = Vec::new();
        if !failure_episodes.is_empty() {
            let tc = tokio::time::Instant::now();
            let episodes_for_cluster = failure_episodes.clone();
            clusters = tokio::task::spawn_blocking(move || {
                let clusterer = DBSCANClustering::new(epsilon, min_samples);
                clusterer.cluster(episodes_for_cluster)
            })
            .await
            .map_err(|join_err| crate::MemoryError::InternalError(join_err.to_string()))??;
            tracing::info!(
                elapsed_ms = tc.elapsed().as_millis() as u64,
                episodes = failure_episodes.len(),
                clusters = clusters.len(),
                "dbscan_cluster"
            );
        }

        let mut result = ConsolidationResult {
            episodes_processed: episodes.len(),
            clusters_identified: clusters.len(),
            rules_extracted: 0,
            rules_verified: 0,
            rules_rejected: 0,
            entities_created: 0,
            facts_created: 0,
        };

        // Step 5a: Extract semantic rules from failure clusters
        // Provenance is carried alongside each rule so the storing fn can stamp
        // model_id/version/dim and append a provenance event in one transaction.
        for cluster in &clusters {
            let rules_with_prov = self.extract_rules_from_cluster(agent_id, cluster).await?;
            result.rules_extracted += rules_with_prov.len();

            for (rule, provenance) in rules_with_prov {
                let source_ref = serde_json::json!({
                    "kind": "consolidation_failure_cluster",
                    "agent_id": agent_id,
                    "source_episode_cluster": rule.source_episode_cluster,
                });
                self.store
                    .store_semantic_rule_with_provenance(
                        rule,
                        provenance.as_ref(),
                        Some(source_ref),
                    )
                    .await?;
            }
        }

        // Step 5b: Extract knowledge rules from successful episodes (LLM only)
        if let Some(llm) = &self.llm {
            // Highest authority first, then take the budget.
            //
            // This used to `.take(30)` straight off `get_unconsolidated_episodes`,
            // which returns `ORDER BY timestamp_ref DESC`. Nothing anywhere read
            // `authority_weight`, so a HITL correction — stamped 1.0, coherence
            // -gated, and for agent-wide scope signed off by two independent
            // reviewers — sat in that queue as an ordinary row. An agent that had
            // run thirty times since the correction dropped it from extraction
            // entirely, silently.
            //
            // That is the whole of Loop 2's value: a human said "this is wrong,
            // here is the right answer", and whether it survived to become a
            // semantic rule depended on how busy the agent had been since. Sorting
            // first costs nothing and makes the authority stamp mean something.
            //
            // `sort_by` is stable, so within an authority band the existing
            // newest-first order is preserved.
            let success_episodes = Self::rank_success_episodes_by_authority(&episodes, 30);

            if !success_episodes.is_empty() {
                match self
                    .extract_knowledge_rules(agent_id, &success_episodes, llm)
                    .await
                {
                    Ok(knowledge_rules) => {
                        result.rules_extracted += knowledge_rules.len();
                        for (rule, provenance) in knowledge_rules {
                            let source_ref = serde_json::json!({
                                "kind": "consolidation_knowledge_rule",
                                "agent_id": agent_id,
                                "source_episode_cluster": rule.source_episode_cluster,
                            });
                            self.store
                                .store_semantic_rule_with_provenance(
                                    rule,
                                    provenance.as_ref(),
                                    Some(source_ref),
                                )
                                .await?;
                        }
                    }
                    Err(e) => {
                        eprintln!("Knowledge rule extraction failed (non-fatal): {}", e);
                    }
                }
            }
        }

        // Step 6: Extract entities from episodes
        let entities_stored = if let Some(llm) = &self.llm {
            match self
                .extract_entities_with_llm(agent_id, &episodes, llm)
                .await
            {
                Ok(entities_with_prov) => {
                    result.entities_created = entities_with_prov.len();
                    let mut stored = Vec::new();
                    for (entity, provenance) in entities_with_prov {
                        let source_ref = serde_json::json!({
                            "kind": "consolidation_llm_entity",
                            "agent_id": agent_id,
                            "source_episodes": entity.source_episodes,
                        });
                        self.store
                            .store_entity_with_provenance(
                                entity.clone(),
                                provenance.as_ref(),
                                Some(source_ref),
                            )
                            .await?;
                        stored.push(entity);
                    }
                    stored
                }
                Err(e) => {
                    eprintln!(
                        "LLM entity extraction failed, falling back to heuristic: {}",
                        e
                    );
                    let mut stored = Vec::new();
                    for episode in episodes.iter().take(100) {
                        let entities_with_prov = self
                            .extract_entities_from_episode(agent_id, episode)
                            .await?;
                        result.entities_created += entities_with_prov.len();
                        for (entity, provenance) in entities_with_prov {
                            let source_ref = serde_json::json!({
                                "kind": "consolidation_heuristic_entity",
                                "agent_id": agent_id,
                                "source_episode": episode.episode_id,
                            });
                            self.store
                                .store_entity_with_provenance(
                                    entity.clone(),
                                    provenance.as_ref(),
                                    Some(source_ref),
                                )
                                .await?;
                            stored.push(entity);
                        }
                    }
                    stored
                }
            }
        } else {
            let mut stored = Vec::new();
            for episode in episodes.iter().take(100) {
                let entities_with_prov = self
                    .extract_entities_from_episode(agent_id, episode)
                    .await?;
                result.entities_created += entities_with_prov.len();
                for (entity, provenance) in entities_with_prov {
                    let source_ref = serde_json::json!({
                        "kind": "consolidation_heuristic_entity",
                        "agent_id": agent_id,
                        "source_episode": episode.episode_id,
                    });
                    self.store
                        .store_entity_with_provenance(
                            entity.clone(),
                            provenance.as_ref(),
                            Some(source_ref),
                        )
                        .await?;
                    stored.push(entity);
                }
            }
            stored
        };

        // Step 6b: Extract facts (relationships) between entities (LLM only)
        if let Some(llm) = &self.llm {
            if entities_stored.len() >= 2 {
                match self
                    .extract_facts_with_llm(agent_id, &entities_stored, &episodes, llm)
                    .await
                {
                    Ok(facts) => {
                        result.facts_created = facts.len();
                        for fact in facts {
                            self.store.store_fact(fact).await?;
                        }
                    }
                    Err(e) => {
                        eprintln!("Fact extraction failed (non-fatal): {}", e);
                    }
                }
            }
        }

        // Step 7: Mark episodes as consolidated — but only if this run could
        // actually learn from them.
        //
        // Consuming the input is what turned a recoverable outage into
        // permanent data loss. When the extraction LLM was unavailable, this
        // marked every episode consolidated anyway; the facts and rules paths
        // are both gated on `self.llm`, so nothing could have been extracted.
        // On this deployment that left 62 agents with 1,035 episodes marked
        // consumed, an empty ontology, and zero episodes eligible for a retry:
        // re-running dreaming did nothing because there was nothing left that
        // counted as unconsolidated.
        //
        // A run WITH an extractor that finds nothing is different — that is a
        // real "there was no durable knowledge here" answer, and consuming the
        // episodes is correct. Otherwise a barren set would be reprocessed
        // forever. So the condition is the presence of the extractor, not the
        // size of the yield.
        if self.llm.is_some() {
            self.store
                .mark_episodes_consolidated(&episode_ids, job_id)
                .await?;
        } else {
            tracing::warn!(
                agent_id = %agent_id,
                episodes = episode_ids.len(),
                "[consolidation] no extraction model — leaving episodes unconsolidated so \
                 they can be re-dreamt once one is available"
            );
        }

        // Step 8: Update job statistics
        self.store
            .update_consolidation_job(
                job_id,
                result.episodes_processed as i32,
                result.clusters_identified as i32,
                result.rules_extracted as i32,
                result.rules_verified as i32,
                result.rules_rejected as i32,
                result.entities_created as i32,
                result.facts_created as i32,
            )
            .await?;

        // Step 9: Complete job
        self.store
            .complete_consolidation_job(job_id, "completed", None)
            .await?;

        Ok(result)
    }

    /// Extracts semantic rules from an episode cluster.
    ///
    /// Returns each rule paired with its `ProvenancedEmbedding` (if generation
    /// succeeded) so the persistence layer can stamp full Spec 22 provenance.
    async fn extract_rules_from_cluster(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
    ) -> Result<Vec<(SemanticRule, Option<ProvenancedEmbedding>)>> {
        let episode_ids: Vec<Uuid> = cluster.episodes.iter().map(|e| e.episode_id).collect();

        // Use LLM if available, otherwise fall back to pattern-based extraction
        if let Some(llm) = &self.llm {
            self.extract_rules_with_llm(agent_id, cluster, &episode_ids, llm)
                .await
        } else {
            self.extract_rules_pattern_based(agent_id, cluster, &episode_ids)
                .await
        }
    }

    /// LLM-powered rule extraction
    async fn extract_rules_with_llm(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
        episode_ids: &[Uuid],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<(SemanticRule, Option<ProvenancedEmbedding>)>> {
        let mut rules: Vec<(SemanticRule, Option<ProvenancedEmbedding>)> = Vec::new();

        // Prepare cluster summary for LLM
        let error_messages: Vec<String> = cluster
            .episodes
            .iter()
            .filter_map(|e| e.error_details.clone())
            .take(10) // Limit to avoid token overflow
            .collect();

        let queries: Vec<String> = cluster
            .episodes
            .iter()
            .map(|e| e.query.clone())
            .take(10)
            .collect();

        if error_messages.is_empty() {
            return Ok(rules);
        }

        // Build prompt for LLM
        let system_prompt = "You are an expert at analyzing failure patterns in AI agent execution logs. \
            Your task is to identify common patterns, root causes, and actionable rules from clusters of failed episodes. \
            Generate 1-3 concise, actionable semantic rules that capture the essence of the failure pattern. \
            Each rule should be a clear statement about what went wrong and ideally suggest how to avoid it.";

        let user_prompt = format!(
            "Analyze this cluster of {} failed episodes and extract semantic rules:\n\n\
            Sample Queries:\n{}\n\n\
            Sample Errors:\n{}\n\n\
            Generate 1-3 semantic rules in JSON format:\n\
            [{{\n  \
              \"rule\": \"<concise rule statement>\",\n  \
              \"description\": \"<detailed explanation>\",\n  \
              \"confidence\": <0.0-1.0>\n\
            }}]",
            cluster.episodes.len(),
            queries.join("\n"),
            error_messages.join("\n")
        );

        let messages = vec![
            self.system_message(system_prompt),
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.3, // Lower temperature for more consistent analysis
            max_tokens: Some(2048),
            ..Default::default()
        };

        // Define expected structure
        #[derive(serde::Deserialize)]
        struct LLMRule {
            rule: String,
            description: String,
            confidence: f64,
        }

        // Call LLM with structured output (automatic parsing + graceful degradation)
        let (llm_rules, usage): (Vec<LLMRule>, _) =
            generate_structured_with_usage(llm.as_ref(), messages, &config).await?;
        self.record_usage(&usage);

        // One floor per cluster, not per rule: every rule from this call was
        // extracted from the same episodes, so the answer cannot differ, and
        // asking once keeps a three-rule extraction from making three
        // identical database round-trips.
        let floor = self.floor_for(episode_ids).await;

        // Convert to SemanticRule objects with provenance
        for llm_rule in llm_rules {
            let provenance = self
                .embedder
                .generate_provenanced(&llm_rule.rule)
                .await
                .ok();
            let embedding = provenance.as_ref().map(|p| p.vector.clone());

            let rule = SemanticRule {
                rule_id: Uuid::new_v4(),
                agent_id,
                rule_content: llm_rule.rule,
                rule_description: Some(llm_rule.description),
                confidence_score: llm_rule.confidence.clamp(0.0, 1.0),
                verification_status: VerificationStatus::Pending,
                verification_method: Some(format!("llm_extraction:{}", llm.model_name())),
                // Migration 201. The rule is FOR `agent_id`; it was WRITTEN by
                // the extractor, and only the former was ever recorded.
                extracted_by: self.extractor_identity,
                source_episode_cluster: episode_ids.to_vec(),
                episode_count: cluster.episodes.len() as i32,
                embedding,
                is_active: true,
                created_at: chrono::Utc::now(),
                // Migration 203.
                provenance_floor: floor.floor.clone(),
                provenance_floor_basis: Some(floor.basis.clone()),
            };

            rules.push((rule, provenance));
        }

        Ok(rules)
    }

    /// Pattern-based rule extraction (fallback)
    async fn extract_rules_pattern_based(
        &self,
        agent_id: Uuid,
        cluster: &EpisodeCluster,
        episode_ids: &[Uuid],
    ) -> Result<Vec<(SemanticRule, Option<ProvenancedEmbedding>)>> {
        let mut rules: Vec<(SemanticRule, Option<ProvenancedEmbedding>)> = Vec::new();

        // Extract common error patterns
        let error_messages: Vec<String> = cluster
            .episodes
            .iter()
            .filter_map(|e| e.error_details.clone())
            .collect();

        if !error_messages.is_empty() {
            let floor = self.floor_for(episode_ids).await;
            let rule_content = format!(
                "Common failure pattern identified across {} episodes",
                cluster.episodes.len()
            );

            let rule_description = if !error_messages.is_empty() {
                Some(format!("Error example: {}", &error_messages[0]))
            } else {
                None
            };

            // Generate embedding for the rule content (with provenance)
            let provenance = self.embedder.generate_provenanced(&rule_content).await.ok();
            let embedding = provenance.as_ref().map(|p| p.vector.clone());

            let rule = SemanticRule {
                rule_id: Uuid::new_v4(),
                agent_id,
                rule_content,
                rule_description,
                confidence_score: calculate_confidence(&cluster.episodes),
                verification_status: VerificationStatus::Pending,
                verification_method: Some("pattern_based".to_string()),
                source_episode_cluster: episode_ids.to_vec(),
                episode_count: cluster.episodes.len() as i32,
                embedding,
                is_active: true,
                created_at: chrono::Utc::now(),
                // Deliberately NOT the extractor. This is the pattern-based
                // fallback, which runs when no extraction model is available —
                // the ontologist had no part in it, so crediting it would
                // reward it for rules a regex wrote and pollute the very signal
                // migration 201 exists to produce.
                extracted_by: None,
                // The floor is about the EVIDENCE, not the extractor, so this
                // path records it exactly like the LLM path does. A regex
                // reading ungrounded episodes produces an ungrounded rule.
                provenance_floor: floor.floor,
                provenance_floor_basis: Some(floor.basis),
            };

            rules.push((rule, provenance));
        }

        Ok(rules)
    }

    /// Extracts entities from an episode
    async fn extract_entities_from_episode(
        &self,
        agent_id: Uuid,
        episode: &Episode,
    ) -> Result<Vec<(Entity, Option<ProvenancedEmbedding>)>> {
        let mut entities: Vec<(Entity, Option<ProvenancedEmbedding>)> = Vec::new();

        // For now, simple keyword extraction
        // In production, this would use NER or LLM-based extraction
        let text = format!("{} {:?}", episode.query, episode.context);

        // Simple heuristic: extract capitalized words as potential entities
        for word in text.split_whitespace() {
            if word.len() > 3 && word.chars().next().unwrap().is_uppercase() {
                let entity_name = word
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string();

                if entity_name.len() > 3 {
                    let provenance = self.embedder.generate_provenanced(&entity_name).await.ok();
                    let embedding = provenance.as_ref().map(|p| p.vector.clone());

                    let entity = Entity {
                        entity_id: Uuid::new_v4(),
                        agent_id,
                        entity_name: entity_name.clone(),
                        entity_type: "Unknown".to_string(),
                        summary: Some(format!("Extracted from: {}", episode.query)),
                        t_valid: Utc::now(),
                        t_invalid: None,
                        source_episodes: vec![episode.episode_id],
                        extraction_confidence: 0.5,
                        embedding,
                        properties: None,
                    };

                    entities.push((entity, provenance));
                }
            }
        }

        Ok(entities)
    }

    /// LLM-powered entity extraction from a batch of episodes
    async fn extract_entities_with_llm(
        &self,
        agent_id: Uuid,
        episodes: &[Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<(Entity, Option<ProvenancedEmbedding>)>> {
        let mut all_entities: Vec<(Entity, Option<ProvenancedEmbedding>)> = Vec::new();

        // Batch episodes into groups of 20 for LLM calls
        for chunk in episodes.chunks(20) {
            let episode_summaries: Vec<String> = chunk
                .iter()
                .map(|e| {
                    let ctx = serde_json::to_string(&e.context).unwrap_or_default();
                    // Char-safe truncation: `&ctx[..200]` panics when byte 200
                    // lands inside a multi-byte UTF-8 char (e.g. '≈' in a
                    // macro-econ episode), which killed the whole consolidation
                    // worker task. Take by chars, not bytes.
                    let ctx_preview = if ctx.len() > 200 {
                        let head: String = ctx.chars().take(200).collect();
                        format!("{}...", head)
                    } else {
                        ctx
                    };
                    format!("- Query: {}\n  Context: {}", e.query, ctx_preview)
                })
                .collect();

            let system_prompt = "You are an expert knowledge graph constructor. \
                Extract named entities from AI agent execution logs. \
                Identify specific people, organizations, concepts, technologies, locations, \
                events, metrics, and domain-specific terms that represent distinct knowledge nodes. \
                Return ONLY a JSON array. Do not extract generic words — focus on proper nouns and domain concepts.";

            let user_prompt = format!(
                "Extract named entities from these {} agent execution episodes:\n\n{}\n\n\
                Return a JSON array:\n\
                [{{\"name\": \"<entity name>\", \"type\": \"<Person|Organization|Concept|Technology|Location|Event|Metric|Domain>\", \"summary\": \"<one-sentence description>\"}}]",
                chunk.len(),
                episode_summaries.join("\n")
            );

            let messages = vec![
                self.system_message(system_prompt),
                Message {
                    role: MessageRole::User,
                    content: user_prompt,
                },
            ];

            let config = GenerationConfig {
                temperature: 0.2,
                max_tokens: Some(2048),
                ..Default::default()
            };

            #[derive(serde::Deserialize)]
            struct LLMEntity {
                name: String,
                #[serde(rename = "type")]
                entity_type: String,
                summary: String,
            }

            let llm_entities: Vec<LLMEntity> =
                match generate_structured_with_usage(llm.as_ref(), messages, &config).await {
                    Ok((e, usage)) => {
                        self.record_usage(&usage);
                        e
                    }
                    Err(e) => {
                        eprintln!("Entity extraction batch failed: {}", e);
                        continue;
                    }
                };

            // Deduplicate by name (case-insensitive) within batch
            let mut seen = std::collections::HashSet::new();
            let episode_ids: Vec<Uuid> = chunk.iter().map(|e| e.episode_id).collect();

            for llm_entity in llm_entities {
                let key = llm_entity.name.to_lowercase();
                if seen.contains(&key) || llm_entity.name.len() < 2 {
                    continue;
                }
                seen.insert(key);

                let provenance = self
                    .embedder
                    .generate_provenanced(&llm_entity.name)
                    .await
                    .ok();
                let embedding = provenance.as_ref().map(|p| p.vector.clone());

                all_entities.push((
                    Entity {
                        entity_id: Uuid::new_v4(),
                        agent_id,
                        entity_name: llm_entity.name,
                        entity_type: llm_entity.entity_type,
                        summary: Some(llm_entity.summary),
                        t_valid: Utc::now(),
                        t_invalid: None,
                        source_episodes: episode_ids.clone(),
                        extraction_confidence: 0.8,
                        embedding,
                        properties: None,
                    },
                    provenance,
                ));
            }
        }

        Ok(all_entities)
    }

    /// LLM-powered fact (relationship) extraction between entities
    async fn extract_facts_with_llm(
        &self,
        agent_id: Uuid,
        entities: &[Entity],
        episodes: &[Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<Fact>> {
        let entity_list: Vec<String> = entities
            .iter()
            .map(|e| format!("- {} ({})", e.entity_name, e.entity_type))
            .collect();

        let episode_context: Vec<String> = episodes
            .iter()
            .take(15)
            .map(|e| format!("- {}", e.query))
            .collect();

        let system_prompt = "You are an expert at identifying relationships between entities \
            in a knowledge domain. Given a list of entities and context from agent execution logs, \
            identify meaningful relationships between them. \
            Return ONLY a JSON array. Only include relationships you are confident about.";

        let user_prompt = format!(
            "Entities:\n{}\n\nContext (sample queries):\n{}\n\n\
            Identify relationships between these entities. Return a JSON array:\n\
            [{{\"source\": \"<source entity name>\", \"target\": \"<target entity name>\", \
            \"relation\": \"<relationship type>\", \
            \"cardinality\": \"one_to_one\"|\"one_to_many\"|\"many_to_one\"|\"many_to_many\", \
            \"confidence\": <0.0-1.0>, \
            \"reasoning\": \"<brief explanation>\"}}]",
            entity_list.join("\n"),
            episode_context.join("\n")
        );

        let messages = vec![
            self.system_message(system_prompt),
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.2,
            max_tokens: Some(2048),
            ..Default::default()
        };

        #[derive(serde::Deserialize)]
        struct LLMFact {
            source: String,
            target: String,
            relation: String,
            #[serde(default = "default_cardinality")]
            cardinality: String,
            #[serde(default = "default_confidence")]
            confidence: f64,
            reasoning: Option<String>,
        }
        fn default_cardinality() -> String {
            "many_to_many".to_string()
        }
        fn default_confidence() -> f64 {
            0.7
        }

        let (llm_facts, usage): (Vec<LLMFact>, _) =
            generate_structured_with_usage(llm.as_ref(), messages, &config).await?;
        self.record_usage(&usage);

        // Build name -> entity lookup (case-insensitive)
        let entity_map: std::collections::HashMap<String, &Entity> = entities
            .iter()
            .map(|e| (e.entity_name.to_lowercase(), e))
            .collect();

        let episode_ids: Vec<Uuid> = episodes.iter().take(15).map(|e| e.episode_id).collect();
        let mut facts = Vec::new();

        for llm_fact in llm_facts {
            let source = entity_map.get(&llm_fact.source.to_lowercase());
            let target = entity_map.get(&llm_fact.target.to_lowercase());

            if let (Some(src), Some(tgt)) = (source, target) {
                let cardinality = match llm_fact.cardinality.as_str() {
                    "one_to_one" => Cardinality::OneToOne,
                    "one_to_many" => Cardinality::OneToMany,
                    "many_to_one" => Cardinality::ManyToOne,
                    _ => Cardinality::ManyToMany,
                };

                facts.push(Fact {
                    fact_id: Uuid::new_v4(),
                    agent_id,
                    source_entity_id: src.entity_id,
                    target_entity_id: tgt.entity_id,
                    relation_type: llm_fact.relation,
                    relation_cardinality: cardinality,
                    confidence: llm_fact.confidence.clamp(0.0, 1.0),
                    reasoning: llm_fact.reasoning,
                    t_valid: Utc::now(),
                    t_invalid: None,
                    source_episodes: episode_ids.clone(),
                    data: None,
                });
            }
        }

        Ok(facts)
    }

    /// LLM-powered knowledge rule extraction from successful episodes
    async fn extract_knowledge_rules(
        &self,
        agent_id: Uuid,
        episodes: &[&Episode],
        llm: &Arc<dyn LLMProvider>,
    ) -> Result<Vec<(SemanticRule, Option<ProvenancedEmbedding>)>> {
        let episode_summaries: Vec<String> = episodes
            .iter()
            .map(|e| {
                let ctx = serde_json::to_string(&e.context).unwrap_or_default();
                // Char-safe truncation (see extract_entities_with_llm): byte
                // slicing panics on a multi-byte boundary.
                let ctx_preview = if ctx.len() > 300 {
                    let head: String = ctx.chars().take(300).collect();
                    format!("{}...", head)
                } else {
                    ctx
                };
                format!("- Query: {}\n  Context: {}", e.query, ctx_preview)
            })
            .collect();

        let episode_ids: Vec<Uuid> = episodes.iter().map(|e| e.episode_id).collect();

        let system_prompt =
            "You are an expert at distilling knowledge from AI agent execution logs. \
            Extract 2-5 semantic rules — reusable insights, patterns, or domain knowledge that \
            the agent has learned through its successful executions. \
            Each rule should be a clear, actionable insight that could improve future performance. \
            Return ONLY a JSON array.";

        let user_prompt = format!(
            "Analyze these {} successful agent executions and extract knowledge rules:\n\n{}\n\n\
            Return a JSON array:\n\
            [{{\"rule\": \"<concise rule statement>\", \
            \"description\": \"<detailed explanation>\", \
            \"confidence\": <0.0-1.0>}}]",
            episodes.len(),
            episode_summaries.join("\n")
        );

        let messages = vec![
            self.system_message(system_prompt),
            Message {
                role: MessageRole::User,
                content: user_prompt,
            },
        ];

        let config = GenerationConfig {
            temperature: 0.3,
            max_tokens: Some(2048),
            ..Default::default()
        };

        #[derive(serde::Deserialize)]
        struct LLMRule {
            rule: String,
            description: String,
            #[serde(default = "default_rule_confidence")]
            confidence: f64,
        }
        fn default_rule_confidence() -> f64 {
            0.7
        }

        let (llm_rules, usage): (Vec<LLMRule>, _) =
            generate_structured_with_usage(llm.as_ref(), messages, &config).await?;
        self.record_usage(&usage);

        let floor = self.floor_for(&episode_ids).await;

        let mut rules: Vec<(SemanticRule, Option<ProvenancedEmbedding>)> = Vec::new();
        for llm_rule in llm_rules {
            let provenance = self
                .embedder
                .generate_provenanced(&llm_rule.rule)
                .await
                .ok();
            let embedding = provenance.as_ref().map(|p| p.vector.clone());

            rules.push((
                SemanticRule {
                    rule_id: Uuid::new_v4(),
                    agent_id,
                    rule_content: llm_rule.rule,
                    rule_description: Some(llm_rule.description),
                    confidence_score: llm_rule.confidence.clamp(0.0, 1.0),
                    verification_status: VerificationStatus::Pending,
                    verification_method: Some(format!(
                        "llm_knowledge_extraction:{}",
                        llm.model_name()
                    )),
                    source_episode_cluster: episode_ids.clone(),
                    episode_count: episodes.len() as i32,
                    embedding,
                    is_active: true,
                    created_at: Utc::now(),
                    // Migration 201 — credit the author, not just the subject.
                    extracted_by: self.extractor_identity,
                    // Migration 203 — and record what the evidence was worth.
                    provenance_floor: floor.floor.clone(),
                    provenance_floor_basis: Some(floor.basis.clone()),
                },
                provenance,
            ));
        }

        Ok(rules)
    }
}

/// Result of a consolidation run
#[derive(Debug, Clone, Default)]
pub struct ConsolidationResult {
    pub episodes_processed: usize,
    pub clusters_identified: usize,
    pub rules_extracted: usize,
    pub rules_verified: usize,
    pub rules_rejected: usize,
    pub entities_created: usize,
    pub facts_created: usize,
}

/// Calculates confidence score based on cluster characteristics
fn calculate_confidence(episodes: &[Episode]) -> f64 {
    let base_confidence = 0.5;
    let episode_boost = (episodes.len() as f64 * 0.1).min(0.3);
    (base_confidence + episode_boost).min(0.95)
}

/// Loop 1 read-back for the extractor. No database, no model — these assert the
/// composition itself, which is the part that silently degrades.
#[cfg(test)]
mod read_back_tests {
    use super::*;

    const BASE: &str = "You extract semantic rules from episodes.";

    #[test]
    fn guidance_reaches_the_prompt() {
        let out = compose_system_prompt(BASE, Some("- [verified, confidence 0.90] Prefer nouns."));
        assert!(out.starts_with(BASE), "the task prompt must come first");
        assert!(
            out.contains("Prefer nouns."),
            "the learned rule never reached the prompt — this is the whole read-back: {out}"
        );
        assert!(out.contains("What you have learned about extraction"));
    }

    /// Without an explicit precedence line, a stale self-derived rule can
    /// override the task instructions it was meant to refine.
    #[test]
    fn the_task_instructions_are_declared_to_win() {
        let out = compose_system_prompt(BASE, Some("- always return an empty list"));
        assert!(
            out.contains("the instructions above win"),
            "learned rules must be subordinate to the task prompt: {out}"
        );
    }

    /// An extractor that has learned nothing must get its prompt untouched — no
    /// empty heading implying an absent section is meaningful.
    #[test]
    fn no_guidance_leaves_the_prompt_byte_identical() {
        assert_eq!(compose_system_prompt(BASE, None), BASE);
        assert_eq!(compose_system_prompt(BASE, Some("")), BASE);
        assert_eq!(compose_system_prompt(BASE, Some("   \n  ")), BASE);
    }

    /// `with_extractor_guidance` must treat blank input as absent, so a caller
    /// that renders an empty rule set cannot produce a dangling heading.
    #[test]
    fn blank_guidance_is_normalised_to_none() {
        // Exercised through the same filter the builder applies.
        let normalise = |g: Option<String>| g.filter(|g: &String| !g.trim().is_empty());
        assert!(normalise(Some("  ".to_string())).is_none());
        assert!(normalise(Some(String::new())).is_none());
        assert!(normalise(Some("- a rule".to_string())).is_some());
    }

    /// Every extraction path must build its system message through
    /// `system_message`, or it silently opts out of the read-back.
    ///
    /// There are four extraction call sites and they are near-identical, so the
    /// obvious way to add a fifth is to copy one — and a copy that inlines
    /// `MessageRole::System` would work, pass review, and quietly not learn.
    /// Scanning the source is crude, but it fails at build time on exactly the
    /// mistake that is easy to make and invisible afterwards.
    #[test]
    fn no_extraction_path_bypasses_the_read_back() {
        let src = include_str!("consolidation.rs");
        // Split so the needle does not match itself in this file.
        let needle = concat!("role: MessageRole", "::System");
        // The single legitimate construction, inside `system_message` itself.
        let inline = src.matches(needle).count();
        assert_eq!(
            inline, 1,
            "found {inline} inline System-message constructions; every extraction call must \
             go through `self.system_message(..)` so the extractor's learned rules are \
             injected. Exactly one is expected: the one inside `system_message`."
        );

        // And the helper is actually used by the extraction paths.
        let uses = concat!("self.system_message(", "system_prompt)");
        assert!(
            src.matches(uses).count() >= 4,
            "expected all four extraction sites to call self.system_message"
        );
    }

    /// Rules carry their verification status into the prompt. An unverified rule
    /// is the extractor's untested hypothesis about itself; presenting it with
    /// the authority of a verified one is how a guess becomes a constraint.
    #[test]
    fn rule_status_survives_into_the_prompt() {
        let out = compose_system_prompt(
            BASE,
            Some("- [pending, confidence 0.31] Merge similar entities aggressively."),
        );
        assert!(out.contains("pending"), "{out}");
        assert!(out.contains("0.31"), "{out}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Agent, MockEmbeddings};
    use serde_json::json;

    async fn get_test_store() -> MemoryStore {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        MemoryStore::new(&database_url).await.unwrap()
    }

    #[tokio::test]
    async fn test_consolidation_workflow() {
        let store = Arc::new(get_test_store().await);
        let pool = Arc::new(store.pool().clone());
        let lock = Arc::new(ConsolidationLock::new(pool, "test-worker".to_string()));
        let embedder = Arc::new(MockEmbeddings::new(1024));

        let worker =
            ConsolidationWorker::new(store.clone(), lock, embedder, "test-worker".to_string());

        // Create agent
        let agent = Agent {
            agent_id: Uuid::new_v4(),
            agent_name: format!("test_agent_{}", Uuid::new_v4()),
            agent_type: "test".to_string(),
            version: "1.0.0".to_string(),
            tier: "test".to_string(),
            executor_type: "llm".to_string(),
            model: "test-model".to_string(),
            temperature: 0.3,
            // None = undescribed ("Incertae sedis"), which is the right default
            // for a consolidation fixture that says nothing about taxonomy.
            taxonomy: None,
            mcp_servers: None,
            mcp_tools: None,
            description: None,
            author: "test".to_string(),
            current_ontology_commit: None,
            current_ontology_snapshot_id: None,
            last_consolidated_at: None,
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_cost_usd: None,
            avg_execution_time_ms: 0,
            dreaming_budget_credits: 0,
            dreaming_credits_used: 0,
            dreaming_budget_reset_at: None,
            system_prompt: None,
            visibility: "public".to_string(),
            owner_id: None,
            tags: vec![],
            education_budget_credits: 0,
            education_credits_used: 0,
            display_alias: None,
            llm_provider: "anthropic".to_string(),
            embedding_provider: "anthropic".to_string(),
            embedding_model: "voyage-2".to_string(),
            embedding_dimension: 1024,
            sample_queries: vec![],
            status: "draft".to_string(),
            fork_pricing: None,
            forked_from: None,
            fork_count: 0,
            accepts: vec![],
            produces: vec![],
            workflow_template: None,
            prompt_template: None,
            requires_secrets: None,
            auto_collect_pct: 0,
            model_ladder: serde_json::Value::Array(vec![]),
            min_tier: "free".to_string(),
            capability_gates: serde_json::Value::Object(serde_json::Map::new()),
            persona_version: 1,
            fermi_contract: None,
            output_contract: None,
            model_params: serde_json::Value::Object(serde_json::Map::new()),
            valence: None,
        };
        store.upsert_agent(agent.clone()).await.unwrap();

        // Create test episodes with failures
        for i in 0..10 {
            // Provenanced, not bare: migration 136's
            // `episodes_embedding_has_provenance` rejects a row that carries an
            // embedding with NULL model_id/version/dim, which is what the
            // deprecated `store_episode` writes. Constructed by hand rather
            // than via an embedder because the vector here is a fixed stub and
            // the clustering assertions below depend on it staying identical.
            let prov = crate::ProvenancedEmbedding {
                vector: vec![0.1; 1024],
                source_text: format!("Test query {}", i),
                model_id: "test/consolidation-fixture".to_string(),
                model_version: "test-v1".to_string(),
                dim: 1024,
            };
            let episode = Episode {
                response_text: None,
                episode_id: Uuid::new_v4(),
                agent_id: agent.agent_id,
                timestamp_ref: Utc::now(),
                query: format!("Test query {}", i),
                context: json!({"test": i}),
                execution_status: if i % 3 == 0 {
                    crate::ExecutionStatus::Failure
                } else {
                    crate::ExecutionStatus::Success
                },
                error_details: if i % 3 == 0 {
                    Some(format!("Error {}", i))
                } else {
                    None
                },
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(rust_decimal::Decimal::new(1, 3)),
                input_tokens: None,
                output_tokens: None,
                cost_basis: None,
                cost_rate_key: None,
                parent_episode_id: None,
                embedding: Some(prov.vector.clone()),
                consolidated: false,
                tags: vec![],
                provenance: crate::Provenance::AutoPass,
                authority_weight: 0.5,
                dyad_id: None,
                persona_version_at_write: None,
                provider_used: None,
                model_used: None,
            };
            store
                .store_episode_with_provenance(episode, Some(&prov), None)
                .await
                .unwrap();
        }

        // Run consolidation
        let result = worker
            .consolidate_agent(agent.agent_id, 0.5, 2)
            .await
            .unwrap();

        assert_eq!(result.episodes_processed, 10);
        assert!(result.rules_extracted > 0 || result.clusters_identified == 0);

        // Episodes must NOT be consumed by a run that had no extractor.
        //
        // This worker is built with `ConsolidationWorker::new`, so `self.llm`
        // is None and neither the rules nor the facts path can produce
        // anything. Step 7 of `consolidate_agent` therefore leaves the
        // episodes unconsolidated on purpose, so they can be re-dreamt once a
        // model is available.
        //
        // This assertion used to require `remaining.len() == 0` — i.e. it
        // demanded exactly the behaviour that turned an LLM outage into
        // permanent data loss for 62 agents and 1,035 episodes, and which the
        // gate was added to prevent. It has been failing since that fix
        // landed, invisibly, because CI stopped at the migration ratchet long
        // before the DB tests ran. See docs/plans/CI_MIGRATION_RATCHET.md.
        //
        // Asserting the guard instead makes this a regression test for the
        // data-loss fix rather than a demand for its return.
        let remaining = store
            .get_unconsolidated_episodes(agent.agent_id)
            .await
            .unwrap();
        assert_eq!(
            remaining.len(),
            10,
            "a consolidation run with no extraction model must leave every \
             episode re-dreamable; marking them consumed is the data-loss bug"
        );

        println!("✅ Consolidation workflow works!");
        println!("   Episodes processed: {}", result.episodes_processed);
        println!("   Clusters identified: {}", result.clusters_identified);
        println!("   Rules extracted: {}", result.rules_extracted);
        println!("   Entities created: {}", result.entities_created);

        // v0.10.25: teardown so this test doesn't accumulate
        // `test_agent_<uuid>` rows in the shared DB. CASCADE handles
        // episodes, entities, semantic_rules, consolidation_jobs, etc.
        let _ = sqlx::query("DELETE FROM agents WHERE agent_id = $1")
            .bind(agent.agent_id)
            .execute(store.pool())
            .await;
    }
}

#[cfg(test)]
mod authority_tests {
    use super::*;
    use chrono::Utc;

    fn ep(weight: f64, query: &str, status: ExecutionStatus) -> Episode {
        Episode {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::nil(),
            timestamp_ref: Utc::now(),
            query: query.to_string(),
            context: serde_json::json!({}),
            execution_status: status,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            cost_basis: None,
            cost_rate_key: None,
            parent_episode_id: None,
            response_text: None,
            embedding: None,
            consolidated: false,
            tags: vec![],
            provenance: crate::Provenance::AutoPass,
            authority_weight: weight,
            dyad_id: None,
            persona_version_at_write: None,
            provider_used: None,
            model_used: None,
        }
    }

    /// Loop 2's output must survive Loop 1's extraction budget.
    ///
    /// A human correction is written with `authority_weight = 1.0` after
    /// passing the coherence gate. Before this ordering existed, extraction
    /// took the first 30 successful episodes in recency order, so an agent
    /// that had run 30 times since the correction dropped it entirely and the
    /// human decision never became a rule.
    #[test]
    fn human_corrections_survive_the_extraction_budget() {
        let mut episodes: Vec<Episode> = (0..40)
            .map(|i| ep(0.5, &format!("ordinary {i}"), ExecutionStatus::Success))
            .collect();
        // The correction is the OLDEST, i.e. last in recency order and well
        // outside a naive take(30).
        episodes.push(ep(1.0, "human correction", ExecutionStatus::Success));

        let ranked = ConsolidationWorker::rank_success_episodes_by_authority(&episodes, 30);

        assert_eq!(ranked.len(), 30);
        assert_eq!(
            ranked[0].query, "human correction",
            "the highest-authority episode must be extracted first, not truncated away"
        );
    }

    /// Ordering must not smuggle failures into the success-only extractor.
    #[test]
    fn failures_are_excluded_regardless_of_authority() {
        let episodes = vec![
            ep(1.0, "failed correction", ExecutionStatus::Failure),
            ep(0.5, "ok", ExecutionStatus::Success),
        ];
        let ranked = ConsolidationWorker::rank_success_episodes_by_authority(&episodes, 30);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].query, "ok");
    }

    /// Within one authority band the caller's newest-first order is kept, so
    /// this change cannot silently reshuffle ordinary consolidation.
    #[test]
    fn equal_authority_preserves_input_order() {
        let episodes: Vec<Episode> = (0..5)
            .map(|i| ep(0.5, &format!("e{i}"), ExecutionStatus::Success))
            .collect();
        let ranked = ConsolidationWorker::rank_success_episodes_by_authority(&episodes, 5);
        let order: Vec<&str> = ranked.iter().map(|e| e.query.as_str()).collect();
        assert_eq!(order, vec!["e0", "e1", "e2", "e3", "e4"]);
    }
}
