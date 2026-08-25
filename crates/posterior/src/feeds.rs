//! Feeds — where observation rows come from.
//!
//! A [`Feed`] is the mirror of an [`Extractor`](crate::extractors::Extractor).
//! An extractor turns *one shaped record* into a scalar; a feed produces *the
//! records*. Before this module existed the feed side was a single hardcoded
//! branch (`if feeds_from.source == "upstream_resolutions"` in
//! `src/handlers/workspace/refit.rs`), which is why BayesOps could only ever
//! learn from other Fermi workspaces.
//!
//! ```text
//! Feed::fetch  ──▶  Vec<ObservationRow>  ──▶  fit_marginal
//!  (where rows come from)   (value + weight + provenance)
//! ```
//!
//! ## Two methods, and the second one is the product
//!
//! [`Feed::fetch`] is the obvious one. [`Feed::describe`] is the one that turns
//! *wiring* into *picking*: a source that can enumerate its own numeric series
//! can be rendered as a dropdown, so a user binds a parameter to a column
//! instead of hand-writing a `source` / `extractor` / `config` triple against
//! JSON they have never seen.
//!
//! ## Where implementations live
//!
//! The trait and its contract types live here, alongside `FittedDistribution`,
//! because they are the shared vocabulary. **Implementations do not.** Real
//! feeds read Postgres, workspace files, or HTTP, and this crate is
//! domain-and-transport-neutral by `docs/specs/14_BAYESOPS_SPEC.md` §9.
//!
//! The trait is therefore pool-free: an implementation holds whatever handle it
//! needs as its own state, constructed at server boot. Platform feeds live in
//! `src/feeds/` in the root crate.
//!
//! `async-trait` is a dependency of this crate for the same reason `thiserror`
//! is — a language utility, not a transport. No I/O crate is pulled in here.
//!
//! See `docs/specs/35_BAYESOPS_PLATFORM_LAYER.md` §4.1.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::extractors::WorkspaceContext;

/// Errors a feed can return.
///
/// Unlike [`ExtractorError`](crate::extractors::ExtractorError) — which is
/// per-row and always non-fatal — a feed error aborts collection for that
/// binding, because it means the *source* could not be reached or understood.
/// The caller records it as a skip with a reason rather than fitting on a
/// partial read, since a silently truncated observation set produces a
/// confident wrong answer.
#[derive(Debug, Error)]
pub enum FeedError {
    #[error("feed '{name}' not registered")]
    Unknown { name: String },

    #[error("missing required config key '{key}'")]
    MissingConfig { key: String },

    #[error("config key '{key}' has unexpected type (got {got}, want {want})")]
    BadConfig {
        key: String,
        got: String,
        want: String,
    },

    #[error("series '{key}' not offered by feed '{feed}'")]
    UnknownSeries { feed: String, key: String },

    #[error("workspace_context has no entity_id (required by feed '{feed}')")]
    NoEntity { feed: String },

    #[error("source unreachable: {0}")]
    Unreachable(String),

    #[error("internal feed error: {0}")]
    Internal(String),
}

/// One numeric series a feed can offer, as reported by [`Feed::describe`].
///
/// This is what populates the column dropdown in the binding picker. Every
/// field except `key` and `label` exists so the picker can show the user what
/// they are about to bind *before* they bind it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    /// Machine key. Goes into the binding config; stable across calls.
    pub key: String,
    /// Human-readable label for the dropdown.
    pub label: String,
    /// Unit if the source knows one. Advisory — never used for conversion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// How many rows this series currently has.
    pub n_rows: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Epoch millis of the most recent row, when the feed tracks time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
    /// First few values, for a sparkline in the picker. Not the whole series.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<f64>,
}

impl Series {
    pub fn new(key: impl Into<String>, label: impl Into<String>, n_rows: usize) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            unit: None,
            n_rows,
            min: None,
            max: None,
            last_updated: None,
            preview: Vec::new(),
        }
    }

    /// Fill `min` / `max` / `preview` from the values themselves.
    pub fn with_stats(mut self, values: &[f64], preview_len: usize) -> Self {
        let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
        if !finite.is_empty() {
            self.min = finite.iter().copied().reduce(f64::min);
            self.max = finite.iter().copied().reduce(f64::max);
        }
        self.preview = finite.into_iter().take(preview_len).collect();
        self
    }
}

/// One observation, with the provenance needed to audit the fit it lands in.
///
/// The three slots named in the spec — `value`, `at`, `entity` — are the whole
/// contract. Anything that can produce them can inform a learnable parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRow {
    /// The observation. Feeds whose rows are shaped JSON apply the configured
    /// extractor themselves, so by the time a row exists it is a scalar.
    pub value: f64,
    /// Epoch millis, when the source knows when this happened. Captured now,
    /// used later — recency weighting is deliberately not implemented yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    /// Which subject this row is about, for sources that mix subjects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// 1.0 for a real observation; 0.0–0.3 for synthetic. Flows into
    /// `fit_marginal`'s weighted `n_eff`, so synthetic rows cannot manufacture
    /// confidence.
    pub weight: f64,
    /// Where this number came from, in human-readable form. Persisted with the
    /// fit so "what was this built from?" is answerable after the fact.
    pub source_ref: String,
}

impl ObservationRow {
    /// A real (weight 1.0) observation with no timestamp or entity.
    pub fn real(value: f64, source_ref: impl Into<String>) -> Self {
        Self {
            value,
            at: None,
            entity: None,
            weight: 1.0,
            source_ref: source_ref.into(),
        }
    }

    pub fn at(mut self, at: Option<i64>) -> Self {
        self.at = at;
        self
    }

    pub fn entity(mut self, entity: Option<String>) -> Self {
        self.entity = entity;
        self
    }

    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }
}

/// A collected set of observations, ready to fit.
///
/// Exists so provenance survives collection. The previous shape was a bare
/// `Vec<f64>`, which discarded where every number came from — meaning a fit
/// could not be audited once written.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObservationSet {
    pub rows: Vec<ObservationRow>,
}

impl ObservationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, row: ObservationRow) {
        self.rows.push(row);
    }

    pub fn extend(&mut self, rows: impl IntoIterator<Item = ObservationRow>) {
        self.rows.extend(rows);
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Values in collection order, for `fit_marginal`.
    pub fn values(&self) -> Vec<f64> {
        self.rows.iter().map(|r| r.value).collect()
    }

    /// Weights, or `None` when every row is a real observation.
    ///
    /// Returning `None` for the all-real case is deliberate: it is exactly the
    /// argument the caller passed before this type existed, so an unweighted
    /// collection fits bit-identically to how it did before.
    pub fn weights(&self) -> Option<Vec<f64>> {
        if self
            .rows
            .iter()
            .all(|r| (r.weight - 1.0).abs() < f64::EPSILON)
        {
            None
        } else {
            Some(self.rows.iter().map(|r| r.weight).collect())
        }
    }

    /// Sum of weights — the honest denominator. Never the row count.
    pub fn weight_sum(&self) -> f64 {
        self.rows.iter().map(|r| r.weight).sum()
    }

    /// One line per distinct source, e.g. `"17 real from upstream_resolutions;
    /// 40 synthetic from cascade_projection"`. Written to the fit's note so a
    /// snapshot can say what it was built from.
    pub fn provenance_summary(&self) -> String {
        let mut counts: Vec<(String, usize, f64)> = Vec::new();
        for row in &self.rows {
            match counts.iter_mut().find(|(s, _, _)| *s == row.source_ref) {
                Some((_, n, w)) => {
                    *n += 1;
                    *w += row.weight;
                }
                None => counts.push((row.source_ref.clone(), 1, row.weight)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts
            .iter()
            .map(|(src, n, w)| {
                if (*w - *n as f64).abs() < 1e-9 {
                    format!("{} from {}", n, src)
                } else {
                    format!("{} from {} (weight {:.2})", n, src, w)
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The feed primitive. Implementors are `Send + Sync` so the registry can hand
/// them out as `Arc<dyn Feed>` to concurrent refit hooks, and hold their own
/// connection handles rather than receiving one per call.
#[async_trait::async_trait]
pub trait Feed: Send + Sync {
    /// Stable identifier. Used as the registry key and as the value of
    /// `feeds_from.source`.
    fn name(&self) -> &str;

    /// Human-readable, surfaced in the source picker.
    fn description(&self) -> &str {
        ""
    }

    /// Optional JSON Schema for the config shape, for editor support.
    fn config_schema(&self) -> Option<JsonValue> {
        None
    }

    /// What numeric series can this feed offer for this workspace?
    ///
    /// Powers the column dropdown. Feeds that cannot enumerate themselves —
    /// because the shape depends on config the user has not supplied yet —
    /// may return an empty vec, at the cost of not being pickable.
    async fn describe(&self, context: &WorkspaceContext) -> Result<Vec<Series>, FeedError>;

    /// Fetch the rows for one binding.
    ///
    /// Feeds whose underlying records are shaped JSON (upstream resolutions)
    /// apply the configured extractor internally, so every returned row is
    /// already scalar.
    async fn fetch(
        &self,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Vec<ObservationRow>, FeedError>;
}

/// What `list()` reports, for discoverability by the picker and by agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedDescription {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<JsonValue>,
}

/// Registry of named feeds. Cheap to clone (Arcs internally), built once at
/// server boot, immutable from then on. Mirrors `ExtractorRegistry`.
#[derive(Clone, Default)]
pub struct FeedRegistry {
    feeds: HashMap<String, Arc<dyn Feed>>,
}

impl FeedRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a feed. Panics on name collision — at boot time this is a
    /// programming error worth surfacing loudly.
    pub fn register(&mut self, feed: Arc<dyn Feed>) {
        let name = feed.name().to_string();
        if self.feeds.contains_key(&name) {
            panic!("FeedRegistry: duplicate feed name '{}'", name);
        }
        self.feeds.insert(name, feed);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Feed>> {
        self.feeds.get(name).cloned()
    }

    pub fn list(&self) -> Vec<FeedDescription> {
        let mut out: Vec<_> = self
            .feeds
            .values()
            .map(|f| FeedDescription {
                name: f.name().to_string(),
                description: f.description().to_string(),
                config_schema: f.config_schema(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.feeds.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_real_weights_report_none() {
        let mut set = ObservationSet::new();
        set.push(ObservationRow::real(1.0, "a"));
        set.push(ObservationRow::real(2.0, "a"));
        // None is what the pre-refactor caller passed to fit_marginal, so an
        // all-real set must fit bit-identically to how it did before.
        assert!(set.weights().is_none());
        assert_eq!(set.values(), vec![1.0, 2.0]);
    }

    #[test]
    fn mixed_weights_report_some() {
        let mut set = ObservationSet::new();
        set.push(ObservationRow::real(1.0, "real"));
        set.push(ObservationRow::real(2.0, "synthetic").weight(0.2));
        assert_eq!(set.weights(), Some(vec![1.0, 0.2]));
        assert!((set.weight_sum() - 1.2).abs() < 1e-12);
    }

    #[test]
    fn weight_sum_is_not_row_count() {
        // The invariant that stops synthetic augmentation manufacturing
        // confidence: 10 synthetic rows are worth 2 real ones, not 10.
        let mut set = ObservationSet::new();
        for _ in 0..10 {
            set.push(ObservationRow::real(1.0, "synthetic").weight(0.2));
        }
        assert_eq!(set.len(), 10);
        assert!((set.weight_sum() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn provenance_summary_groups_by_source() {
        let mut set = ObservationSet::new();
        set.push(ObservationRow::real(1.0, "upstream_resolutions"));
        set.push(ObservationRow::real(2.0, "upstream_resolutions"));
        set.push(ObservationRow::real(3.0, "workspace_output"));
        let s = set.provenance_summary();
        assert!(s.contains("2 from upstream_resolutions"), "got: {}", s);
        assert!(s.contains("1 from workspace_output"), "got: {}", s);
    }

    #[test]
    fn provenance_summary_shows_weight_when_discounted() {
        let mut set = ObservationSet::new();
        set.push(ObservationRow::real(1.0, "cascade").weight(0.2));
        set.push(ObservationRow::real(2.0, "cascade").weight(0.2));
        let s = set.provenance_summary();
        assert!(s.contains("weight 0.40"), "got: {}", s);
    }

    #[test]
    fn series_stats_ignore_non_finite() {
        let s = Series::new("k", "K", 4).with_stats(&[1.0, f64::NAN, 3.0, f64::INFINITY], 10);
        assert_eq!(s.min, Some(1.0));
        assert_eq!(s.max, Some(3.0));
        assert_eq!(s.preview, vec![1.0, 3.0]);
    }

    #[test]
    #[should_panic(expected = "duplicate feed name")]
    fn registry_rejects_duplicate_names() {
        struct Dummy;
        #[async_trait::async_trait]
        impl Feed for Dummy {
            fn name(&self) -> &str {
                "dup"
            }
            async fn describe(&self, _: &WorkspaceContext) -> Result<Vec<Series>, FeedError> {
                Ok(vec![])
            }
            async fn fetch(
                &self,
                _: &WorkspaceContext,
                _: &JsonValue,
            ) -> Result<Vec<ObservationRow>, FeedError> {
                Ok(vec![])
            }
        }
        let mut r = FeedRegistry::new();
        r.register(Arc::new(Dummy));
        r.register(Arc::new(Dummy));
    }
}
