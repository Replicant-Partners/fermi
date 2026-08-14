//! Provider rate card — the cost basis for economic attribution.
//!
//! # Why this module exists
//!
//! `registry::calculate_cost` priced every execution by matching on the
//! **model string alone**, against eight Anthropic ids, with `_ => 3.0`
//! for everything else. Two consequences, both live in production:
//!
//! - A DeepSeek agent (`efra_critical_factor`) was recorded at
//!   **$3.00/Mtok — Anthropic Sonnet's rate — against a real ~$0.44**.
//!   Two executions were logged at `$0.616272` and `$0.311628` when they
//!   cost roughly `$0.090` and `$0.046`: **~6.9× overstated.**
//! - `claude-haiku-4-5` was priced at `claude-3-haiku`'s $0.25/Mtok when
//!   Haiku 4.5 is $1/$5: **~4× understated.**
//!
//! Meanwhile the absent input/output split *understates* real Anthropic
//! runs by roughly 1.8× at a 20% output share. So the two dominant errors
//! point in **opposite directions**, which makes cross-provider cost
//! comparison not merely imprecise but *directionally wrong* — and
//! cross-provider comparison is the entire question the platform needs to
//! answer.
//!
//! # What changes here
//!
//! 1. **Keyed on `(provider, model)`**, not model alone. Two providers can
//!    serve the same model id at different prices — which is precisely
//!    what happens the moment anything is proxied through OpenRouter.
//! 2. **Separate input and output rates.** Providers charge 3–5× more for
//!    output. `PLATFORM_ECONOMICS.md` §4.1 names this the largest
//!    remaining source of error; the executors already track the split and
//!    discard it at the last line.
//! 3. **Unknown models are reported, not silently defaulted.** A missing
//!    rate returns [`CostBasis::UnknownModel`] so it can be counted and
//!    surfaced. A cost that is quietly wrong is worse than one that is
//!    loudly absent: the first corrupts the marketplace ledger, the second
//!    is a work item.
//! 4. **Data, not control flow.** Provider prices change monthly and
//!    `registry.rs` needed a deploy to follow them
//!    (`PLATFORM_ECONOMICS.md` §4.2, "rate card drift"). The table is a
//!    `const` seed, overridable at runtime from JSON via `RATE_CARD_PATH`.
//!
//! # Cost is derived, never stored alone
//!
//! The DeepSeek rows above cannot be corrected today, because the price
//! was baked into `cost_usd` at write time and the token split was thrown
//! away. Persisting `(input_tokens, output_tokens, provider, model)` makes
//! cost a **recomputable** quantity: fix a rate, re-derive history. For a
//! ledger that has to settle a marketplace, that property is not optional.
//!
//! Every estimate therefore carries its [`CostBasis`], so a consumer can
//! tell a measured figure from an assumed one instead of inferring it from
//! a deploy date (which is what `RATE_CARD_WIRED_ON` currently does).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// USD per 1M tokens, split by direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    /// USD per 1M input (prompt) tokens.
    pub input_per_mtok: f64,
    /// USD per 1M output (completion) tokens.
    pub output_per_mtok: f64,
}

impl Rate {
    pub const fn new(input_per_mtok: f64, output_per_mtok: f64) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
        }
    }

    /// Free substrate (local inference). Distinct from "unknown" — this is
    /// a positive assertion that no money moved.
    pub const FREE: Rate = Rate::new(0.0, 0.0);
}

/// How much to trust a cost figure. Recorded per estimate so the
/// distinction survives into the ledger rather than being reconstructed
/// from a deploy date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasis {
    /// Known rate, real input/output counts. The only fully trustworthy
    /// basis, and the target state for every execution.
    MeasuredSplit,
    /// Known rate, but only a total token count was available, so the
    /// split was assumed at [`ASSUMED_OUTPUT_SHARE`]. Applies to
    /// OpenAI-compatible paths until they report the split, and to any
    /// historical re-pricing.
    AssumedSplit,
    /// No rate is configured for this `(provider, model)`. Priced at
    /// [`FALLBACK_RATE`] purely so totals remain summable — **treat as a
    /// data gap, not a cost.** Count these and fix the table.
    UnknownModel,
    /// Provider is free at the point of use (local inference).
    NoCharge,
}

impl CostBasis {
    /// Stable string for JSON/SQL. Mirrors `CredentialSource::as_str`.
    pub fn as_str(&self) -> &'static str {
        match self {
            CostBasis::MeasuredSplit => "measured_split",
            CostBasis::AssumedSplit => "assumed_split",
            CostBasis::UnknownModel => "unknown_model",
            CostBasis::NoCharge => "no_charge",
        }
    }

    /// Whether this figure is sound enough to base a payout or a
    /// cost-per-Brier-point comparison on.
    pub fn is_trustworthy(&self) -> bool {
        matches!(self, CostBasis::MeasuredSplit | CostBasis::NoCharge)
    }
}

/// A priced execution.
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    pub usd: f64,
    pub basis: CostBasis,
    /// The `(provider, model)` key that resolved, normalised — so a reader
    /// can see *which* row priced this run. Empty when nothing matched.
    pub rate_key: String,
}

impl CostEstimate {
    /// Zero cost, positively asserted (local inference).
    fn no_charge(key: String) -> Self {
        Self {
            usd: 0.0,
            basis: CostBasis::NoCharge,
            rate_key: key,
        }
    }
}

/// Output-token share assumed when only a total is available.
///
/// Deliberately a round number, because it is an assumption and should
/// look like one — same reasoning as `economics.rs::DEFAULT_CREDIT_USD`.
/// Once every executor reports the split this constant stops being
/// load-bearing; until then it is the single place the assumption lives,
/// rather than being re-derived at each call site.
pub const ASSUMED_OUTPUT_SHARE: f64 = 0.20;

/// Rate applied to an unrecognised `(provider, model)`.
///
/// Set to a mid-market blend rather than Sonnet's $3/$15: the old
/// `_ => 3.0` silently asserted "everything is as expensive as Anthropic
/// Sonnet", which is what overstated DeepSeek by ~7×. Any run priced here
/// is flagged [`CostBasis::UnknownModel`], so the number is a placeholder
/// pending a table entry — it is not meant to be accurate, only bounded.
pub const FALLBACK_RATE: Rate = Rate::new(1.0, 4.0);

/// Providers that cost nothing at the point of use.
fn provider_is_free(provider: &str) -> bool {
    matches!(provider, "ollama" | "local")
}

/// Seed rate card: `(provider, model_key, input, output)` in USD/Mtok.
///
/// `model_key` matches a **normalised** model id (lowercased, provider
/// namespace stripped — see [`normalise_model`]) either exactly or as a
/// prefix, longest prefix winning. Prefixes let a family be priced once
/// (`claude-sonnet-4` covers every dated Sonnet 4.x snapshot) while an
/// exact row still overrides it.
///
/// **These are seed values and require operator confirmation against
/// current provider price sheets.** They are list prices for the common
/// case and deliberately ignore cache-read/cache-write tiers, batch
/// discounts, and negotiated rates. That is the point of making the table
/// overridable rather than compiled-in: see [`load_override`].
#[rustfmt::skip]
const SEED_RATES: &[(&str, &str, f64, f64)] = &[
    // ── Anthropic ────────────────────────────────────────────────────
    // Note claude-3-haiku and claude-haiku-4-5 are NOT the same price.
    // Collapsing them is what understated Haiku 4.5 by ~4x.
    ("anthropic", "claude-opus-4",     15.0, 75.0),
    ("anthropic", "claude-3-opus",     15.0, 75.0),
    ("anthropic", "claude-sonnet-4",    3.0, 15.0),
    ("anthropic", "claude-3-5-sonnet",  3.0, 15.0),
    ("anthropic", "claude-3-sonnet",    3.0, 15.0),
    ("anthropic", "claude-haiku-4",     1.0,  5.0),
    ("anthropic", "claude-3-5-haiku",   0.8,  4.0),
    ("anthropic", "claude-3-haiku",     0.25, 1.25),

    // ── OpenAI ───────────────────────────────────────────────────────
    ("openai", "gpt-4o-mini",  0.15,  0.60),
    ("openai", "gpt-4o",       2.50, 10.00),
    ("openai", "gpt-4-turbo", 10.00, 30.00),
    ("openai", "o1-mini",      1.10,  4.40),
    ("openai", "o1",          15.00, 60.00),

    // ── DeepSeek ─────────────────────────────────────────────────────
    // The provider that exposed the old fallback bug.
    ("deepseek", "deepseek-chat",     0.27, 1.10),
    ("deepseek", "deepseek-reasoner", 0.55, 2.19),

    // ── Zhipu GLM ────────────────────────────────────────────────────
    ("glm", "glm-4.6",  0.60, 2.20),
    ("glm", "glm-4.5",  0.60, 2.20),
    ("glm", "glm-4",    0.60, 2.20),
    // `KNOWN_PROVIDER_SECRETS` maps GLM_API_KEY to the name "zhipu",
    // while the executor dispatches on "glm". Price both so the ledger
    // does not depend on which spelling reached it.
    ("zhipu", "glm", 0.60, 2.20),

    // ── Moonshot Kimi ────────────────────────────────────────────────
    ("kimi",     "kimi", 0.60, 2.50),
    ("kimi",     "moonshot", 0.60, 2.50),
    ("moonshot", "kimi", 0.60, 2.50),

    // ── Alibaba Qwen ─────────────────────────────────────────────────
    ("qwen", "qwen-max",  1.60, 6.40),
    ("qwen", "qwen-plus", 0.40, 1.20),
    ("qwen", "qwen",      0.40, 1.20),

    // ── Mistral ──────────────────────────────────────────────────────
    ("mistral", "mistral-large", 2.00, 6.00),
    ("mistral", "mistral-small", 0.20, 0.60),
    ("mistral", "open-mistral",  0.20, 0.60),
    ("mistral", "mixtral",       0.70, 0.70),

    // ── Google Gemini ────────────────────────────────────────────────
    // Reachable by neither executor registry today, but priced so that
    // wiring it up is not gated on a second change.
    ("gemini", "gemini-2.5-pro",     1.25, 10.00),
    ("gemini", "gemini-2.5-flash",   0.30,  2.50),
    ("gemini", "gemini-1.5-pro",     1.25,  5.00),
    ("gemini", "gemini-1.5-flash",   0.075, 0.30),

    // ── Local ────────────────────────────────────────────────────────
    ("ollama", "", 0.0, 0.0),

    // ── OpenRouter ───────────────────────────────────────────────────
    // A proxy, so the *model* determines the price and the namespace is
    // stripped by `normalise_model`. Entries here cover the pass-through
    // uplift on the models actually declared in agent cards. Anything
    // else resolves via `openrouter_underlying_provider` below, which
    // reuses the upstream vendor's row — so proxied Claude is priced as
    // Claude rather than falling to the unknown bucket.
    ("openrouter", "openrouter/free", 0.0, 0.0),
];

/// Pass-through uplift applied when a request is proxied.
///
/// OpenRouter takes a fee at credit top-up rather than per call, so the
/// effective per-token cost is the upstream vendor's price plus a margin.
/// Modelled as a flat multiplier because that is the shape of the fee, and
/// small enough (Sobol: 0.08% of margin variance) that precision here is
/// not worth a per-model table.
pub const PROXY_UPLIFT: f64 = 1.055;

/// The full rate card: seed table plus any runtime override, indexed for
/// lookup. Built once.
struct RateCard {
    /// `(provider, model_key)` → rate. `model_key` may be a prefix.
    entries: Vec<(String, String, Rate)>,
}

impl RateCard {
    fn build() -> Self {
        let mut entries: Vec<(String, String, Rate)> = SEED_RATES
            .iter()
            .map(|(p, m, i, o)| ((*p).to_string(), (*m).to_string(), Rate::new(*i, *o)))
            .collect();

        // Runtime override wins over the seed, so a price change is a
        // config edit rather than a deploy.
        if let Some(over) = load_override() {
            for (provider, models) in over {
                for (model, rate) in models {
                    let p = provider.to_lowercase();
                    let m = normalise_model(&model);
                    if let Some(slot) = entries.iter_mut().find(|(ep, em, _)| *ep == p && *em == m)
                    {
                        slot.2 = rate;
                    } else {
                        entries.push((p, m, rate));
                    }
                }
            }
        }

        // Longest model_key first so a specific row beats a family
        // prefix regardless of declaration order.
        entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        Self { entries }
    }

    fn lookup(&self, provider: &str, model: &str) -> Option<(&str, Rate)> {
        self.entries
            .iter()
            .find(|(p, m, _)| {
                p == provider
                    && (m.is_empty() || model == m.as_str() || model.starts_with(m.as_str()))
            })
            .map(|(_, m, r)| (m.as_str(), *r))
    }
}

fn card() -> &'static RateCard {
    static CARD: OnceLock<RateCard> = OnceLock::new();
    CARD.get_or_init(RateCard::build)
}

/// Optional JSON override: `{ "provider": { "model": {"input_per_mtok": x,
/// "output_per_mtok": y} } }`, path in `RATE_CARD_PATH`.
///
/// Operator config (which prices are current), not a credential, so
/// reading env here does not violate SPEC_28's "no env for agent keys".
fn load_override() -> Option<HashMap<String, HashMap<String, Rate>>> {
    let path = std::env::var("RATE_CARD_PATH").ok()?;
    match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                tracing::error!(
                    path = %path,
                    error = %e,
                    "[rate-card] RATE_CARD_PATH is not valid rate JSON; using seed rates. \
                     Costs will be seed-priced until this is fixed."
                );
                None
            }
        },
        Err(e) => {
            tracing::error!(
                path = %path,
                error = %e,
                "[rate-card] RATE_CARD_PATH unreadable; using seed rates."
            );
            None
        }
    }
}

/// Normalise a model id for table lookup.
///
/// Strips a provider namespace (`anthropic/claude-sonnet-4-6` →
/// `claude-sonnet-4-6`) so the same model prices identically whether it
/// was called directly or proxied, and lowercases. `openrouter/free` is
/// preserved intact because it is a bare sentinel rather than a namespaced
/// id — it is also not a real model, and is retained only so historical
/// rows resolve.
pub fn normalise_model(model: &str) -> String {
    let m = model.trim().to_lowercase();
    if m == "openrouter/free" {
        return m;
    }
    match m.split_once('/') {
        Some((_ns, rest)) if !rest.is_empty() => rest.to_string(),
        _ => m,
    }
}

/// For a proxied request, the upstream vendor that actually served it —
/// inferred from the namespace the caller used, falling back to the model
/// family. Lets proxied Claude price as Claude instead of landing in the
/// unknown bucket.
fn openrouter_underlying_provider(model: &str) -> Option<&'static str> {
    let m = model.trim().to_lowercase();
    let ns = m.split_once('/').map(|(ns, _)| ns);
    let by_ns = match ns {
        Some("anthropic") => Some("anthropic"),
        Some("openai") => Some("openai"),
        Some("deepseek") => Some("deepseek"),
        Some("mistralai") | Some("mistral") => Some("mistral"),
        Some("google") => Some("gemini"),
        Some("qwen") | Some("alibaba") => Some("qwen"),
        Some("moonshotai") | Some("moonshot") => Some("kimi"),
        Some("zhipu") | Some("z-ai") | Some("thudm") => Some("glm"),
        _ => None,
    };
    if by_ns.is_some() {
        return by_ns;
    }
    // No usable namespace — fall back to the model family.
    let bare = normalise_model(&m);
    if bare.starts_with("claude") {
        Some("anthropic")
    } else if bare.starts_with("gpt") || bare.starts_with("o1") || bare.starts_with("o3") {
        Some("openai")
    } else if bare.starts_with("deepseek") {
        Some("deepseek")
    } else if bare.starts_with("glm") {
        Some("glm")
    } else if bare.starts_with("gemini") {
        Some("gemini")
    } else {
        None
    }
}

/// Look up the rate for a `(provider, model)` pair.
///
/// Returns the resolved key alongside the rate so callers can record
/// *which* row priced a run. `None` means the table has no entry — the
/// caller should record [`CostBasis::UnknownModel`] rather than inventing
/// a number.
pub fn rate_for(provider: &str, model: &str) -> Option<(String, Rate)> {
    let p = provider.trim().to_lowercase();
    let m = normalise_model(model);

    if provider_is_free(&p) {
        return Some((format!("{p}/*"), Rate::FREE));
    }

    if let Some((key, rate)) = card().lookup(&p, &m) {
        return Some((format!("{p}/{key}"), rate));
    }

    // Proxied: price as the upstream vendor, plus the pass-through uplift.
    if p == "openrouter" {
        if let Some(upstream) = openrouter_underlying_provider(model) {
            if let Some((key, rate)) = card().lookup(upstream, &m) {
                return Some((
                    format!("openrouter:{upstream}/{key}"),
                    Rate::new(
                        rate.input_per_mtok * PROXY_UPLIFT,
                        rate.output_per_mtok * PROXY_UPLIFT,
                    ),
                ));
            }
        }
    }

    // An empty provider means Anthropic throughout the executor layer
    // (`build_execution_credentials` makes the same substitution), so
    // honour it here rather than dropping the run into `UnknownModel`.
    if p.is_empty() {
        if let Some((key, rate)) = card().lookup("anthropic", &m) {
            return Some((format!("anthropic/{key}"), rate));
        }
    }

    None
}

/// Price a run from real input/output token counts. The trustworthy path.
pub fn cost_of_split(
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> CostEstimate {
    let p = provider.trim().to_lowercase();
    if provider_is_free(&p) {
        return CostEstimate::no_charge(format!("{p}/*"));
    }
    match rate_for(provider, model) {
        Some((key, rate)) => {
            let usd = (input_tokens as f64 / 1_000_000.0) * rate.input_per_mtok
                + (output_tokens as f64 / 1_000_000.0) * rate.output_per_mtok;
            CostEstimate {
                usd,
                basis: if rate == Rate::FREE {
                    CostBasis::NoCharge
                } else {
                    CostBasis::MeasuredSplit
                },
                rate_key: key,
            }
        }
        None => CostEstimate {
            usd: unknown_cost(input_tokens, output_tokens),
            basis: CostBasis::UnknownModel,
            rate_key: String::new(),
        },
    }
}

/// Price a run when only a total token count exists, assuming
/// [`ASSUMED_OUTPUT_SHARE`] output.
///
/// Used by the OpenAI-compatible executors until they report the split,
/// and by any re-pricing of history — the split cannot be recovered
/// retroactively, so historical rows are honestly marked
/// [`CostBasis::AssumedSplit`] rather than passed off as measured.
pub fn cost_of_total(provider: &str, model: &str, total_tokens: u32) -> CostEstimate {
    let output = (total_tokens as f64 * ASSUMED_OUTPUT_SHARE).round() as u32;
    let input = total_tokens.saturating_sub(output);
    let mut est = cost_of_split(provider, model, input, output);
    // Downgrade the basis: the rate may be known but the split is not.
    if est.basis == CostBasis::MeasuredSplit {
        est.basis = CostBasis::AssumedSplit;
    }
    est
}

fn unknown_cost(input_tokens: u32, output_tokens: u32) -> f64 {
    (input_tokens as f64 / 1_000_000.0) * FALLBACK_RATE.input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * FALLBACK_RATE.output_per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The bug this module was written to fix ───────────────────────

    #[test]
    fn deepseek_is_not_priced_as_anthropic_sonnet() {
        // The production case: efra_critical_factor, 205_424 tokens,
        // recorded as $0.616272 by the old `_ => 3.0` fallback.
        let est = cost_of_total("deepseek", "deepseek-chat", 205_424);
        assert_eq!(est.basis, CostBasis::AssumedSplit);
        // Real cost is ~$0.09, an order of magnitude below the old figure.
        assert!(
            est.usd < 0.12,
            "deepseek run should cost well under $0.12, got {}",
            est.usd
        );
        let old_flat_rate_figure = 0.616272;
        assert!(
            est.usd < old_flat_rate_figure / 5.0,
            "expected the old $3/Mtok figure to be >5x this one; got {} vs {}",
            old_flat_rate_figure,
            est.usd
        );
    }

    #[test]
    fn haiku_45_is_not_priced_as_haiku_3() {
        // The old table collapsed both to $0.25/Mtok, understating 4.5 ~4x.
        let (_, h45) = rate_for("anthropic", "claude-haiku-4-5-20251001").unwrap();
        let (_, h3) = rate_for("anthropic", "claude-3-haiku-20240307").unwrap();
        assert!(
            h45.input_per_mtok > h3.input_per_mtok,
            "haiku 4.5 ({:?}) must not be priced at haiku 3's rate ({:?})",
            h45,
            h3
        );
    }

    #[test]
    fn output_tokens_cost_more_than_input() {
        // The single largest remaining error in PLATFORM_ECONOMICS.md §4.1.
        let all_input = cost_of_split("anthropic", "claude-sonnet-4-6", 10_000, 0);
        let all_output = cost_of_split("anthropic", "claude-sonnet-4-6", 0, 10_000);
        assert!(all_output.usd > all_input.usd * 4.0);
        assert_eq!(all_input.basis, CostBasis::MeasuredSplit);
    }

    // ── Unknown models are reported, not invented ────────────────────

    #[test]
    fn unknown_model_is_flagged_rather_than_silently_defaulted() {
        let est = cost_of_split("someprovider", "some-new-model", 1000, 200);
        assert_eq!(est.basis, CostBasis::UnknownModel);
        assert!(est.rate_key.is_empty());
        assert!(!est.basis.is_trustworthy());
        // Still summable so totals don't silently drop runs.
        assert!(est.usd > 0.0);
    }

    #[test]
    fn ollama_is_free_and_says_so_positively() {
        let est = cost_of_split("ollama", "qwen2.5:7b", 50_000, 10_000);
        assert_eq!(est.usd, 0.0);
        assert_eq!(est.basis, CostBasis::NoCharge);
        // Distinct from "unknown": a zero here is an assertion, not a gap.
        assert!(est.basis.is_trustworthy());
    }

    // ── Provider is part of the key ──────────────────────────────────

    #[test]
    fn same_model_prices_differently_per_provider() {
        let direct = cost_of_split("anthropic", "claude-sonnet-4-6", 100_000, 20_000);
        let proxied = cost_of_split("openrouter", "anthropic/claude-sonnet-4-6", 100_000, 20_000);
        assert!(
            proxied.usd > direct.usd,
            "proxying should cost more (uplift), got {} vs {}",
            proxied.usd,
            direct.usd
        );
        assert_eq!(proxied.basis, CostBasis::MeasuredSplit);
        assert!(proxied.rate_key.starts_with("openrouter:anthropic"));
    }

    #[test]
    fn proxied_claude_does_not_fall_into_the_unknown_bucket() {
        // Before this module, `anthropic/claude-...` matched no arm and
        // was priced at the $3 default while ALSO being misattributed to
        // provider "openrouter" by the model-string heuristic.
        let est = cost_of_split("openrouter", "anthropic/claude-haiku-4-5", 10_000, 2_000);
        assert_eq!(est.basis, CostBasis::MeasuredSplit);
    }

    #[test]
    fn namespaced_and_bare_ids_resolve_to_the_same_family() {
        let bare = rate_for("anthropic", "claude-sonnet-4-6").unwrap().1;
        let namespaced = rate_for("anthropic", "anthropic/claude-sonnet-4-6")
            .unwrap()
            .1;
        assert_eq!(bare, namespaced);
    }

    #[test]
    fn empty_provider_means_anthropic() {
        // The executor layer treats "" as anthropic; pricing must agree or
        // a whole class of runs lands in UnknownModel.
        let est = cost_of_split("", "claude-sonnet-4-6", 1000, 100);
        assert_eq!(est.basis, CostBasis::MeasuredSplit);
        assert!(est.rate_key.starts_with("anthropic/"));
    }

    // ── Lookup mechanics ─────────────────────────────────────────────

    #[test]
    fn specific_row_beats_family_prefix() {
        // "claude-3-haiku" must not be swallowed by "claude-haiku-4".
        let (k3, r3) = rate_for("anthropic", "claude-3-haiku-20240307").unwrap();
        assert_eq!(k3, "anthropic/claude-3-haiku");
        assert_eq!(r3.input_per_mtok, 0.25);
    }

    #[test]
    fn dated_snapshots_resolve_via_family_prefix() {
        for m in [
            "claude-sonnet-4-5-20250929",
            "claude-sonnet-4-6",
            "claude-opus-4-6",
        ] {
            let est = cost_of_split("anthropic", m, 1000, 100);
            assert_eq!(
                est.basis,
                CostBasis::MeasuredSplit,
                "{m} should resolve to a known rate"
            );
        }
    }

    #[test]
    fn every_provider_the_executors_dispatch_to_has_at_least_one_rate() {
        // Guards against the drift that produced two divergent provider
        // registries: if a provider is dispatchable it must be priceable.
        for provider in [
            "anthropic",
            "openai",
            "mistral",
            "qwen",
            "openrouter",
            "glm",
            "deepseek",
            "kimi",
            "ollama",
        ] {
            assert!(
                card().lookup(provider, "").is_some()
                    || card().entries.iter().any(|(p, _, _)| p == provider)
                    || provider_is_free(provider),
                "provider {provider} is dispatchable but has no rate card entry"
            );
        }
    }

    // ── Provenance: every basis must be distinguishable in the ledger ───

    #[test]
    fn the_four_bases_are_distinguishable_and_only_two_are_trustworthy() {
        // A ledger that cannot tell a measured cost from a guessed one
        // cannot settle a marketplace. Pin the classification.
        let measured = cost_of_split("anthropic", "claude-sonnet-4-6", 800, 200);
        let assumed = cost_of_total("anthropic", "claude-sonnet-4-6", 1000);
        let unknown = cost_of_split("nobody", "nothing", 800, 200);
        let free = cost_of_split("ollama", "qwen2.5:7b", 800, 200);

        assert!(measured.basis.is_trustworthy());
        assert!(free.basis.is_trustworthy());
        assert!(!assumed.basis.is_trustworthy());
        assert!(!unknown.basis.is_trustworthy());

        // The strings are what land in `episodes.cost_basis`, and the
        // migration's CHECK constraint enumerates exactly these.
        for b in [measured.basis, assumed.basis, unknown.basis, free.basis] {
            assert!(matches!(
                b.as_str(),
                "measured_split" | "assumed_split" | "unknown_model" | "no_charge"
            ));
        }
    }

    #[test]
    fn a_missing_split_is_never_read_as_a_free_run() {
        // The trap: OpenAI-compatible providers often omit the
        // prompt/completion breakdown. Treating that as (0, 0) would price
        // a real run at $0 and silently erase it from the ledger.
        let est = cost_of_total("deepseek", "deepseek-chat", 100_000);
        assert!(est.usd > 0.0, "a run with tokens must cost something");
        assert_eq!(est.basis, CostBasis::AssumedSplit);
    }

    #[test]
    fn total_and_split_agree_when_the_split_matches_the_assumption() {
        let total = 10_000u32;
        let output = (total as f64 * ASSUMED_OUTPUT_SHARE).round() as u32;
        let from_total = cost_of_total("anthropic", "claude-sonnet-4-6", total);
        let from_split = cost_of_split("anthropic", "claude-sonnet-4-6", total - output, output);
        assert!((from_total.usd - from_split.usd).abs() < 1e-9);
        // But the basis is honest about which one measured the split.
        assert_eq!(from_total.basis, CostBasis::AssumedSplit);
        assert_eq!(from_split.basis, CostBasis::MeasuredSplit);
    }

    #[test]
    fn assumed_split_costs_less_than_all_output_and_more_than_all_input() {
        let t = 100_000u32;
        let assumed = cost_of_total("anthropic", "claude-sonnet-4-6", t);
        let all_in = cost_of_split("anthropic", "claude-sonnet-4-6", t, 0);
        let all_out = cost_of_split("anthropic", "claude-sonnet-4-6", 0, t);
        assert!(assumed.usd > all_in.usd);
        assert!(assumed.usd < all_out.usd);
    }
}
