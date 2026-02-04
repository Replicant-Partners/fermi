# UFFP Domain Model
## Core Rust Implementation

**Version**: 0.1.0  
**Date**: 2026-02-04

---

## Domain Entities (Rust)

```rust
// crates/uffp-core/src/lib.rs

use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================
// CORE TYPES
// ============================================

pub type ForecastId = Uuid;
pub type DriverId = Uuid;
pub type EvidenceId = Uuid;
pub type AgentId = String;
pub type UserId = Uuid;

/// Probability: 0.0 to 1.0
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Probability(f64);

impl Probability {
    pub fn new(value: f64) -> Result<Self, &'static str> {
        if !(0.0..=1.0).contains(&value) {
            Err("Probability must be between 0.0 and 1.0")
        } else {
            Ok(Probability(value))
        }
    }
    
    pub fn from_percent(percent: f64) -> Result<Self, &'static str> {
        Self::new(percent / 100.0)
    }
    
    pub fn value(&self) -> f64 {
        self.0
    }
    
    pub fn as_percent(&self) -> f64 {
        self.0 * 100.0
    }
}

// ============================================
// FORECAST (Aggregate Root)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    pub id: ForecastId,
    pub user_id: UserId,
    
    // Question
    pub question: String,
    pub resolution_criteria: String,
    pub target_date: NaiveDate,
    pub domain: Option<String>,
    
    // Outside View
    pub base_rate: Option<BaseRate>,
    
    // Inside View (Fermi Decomposition)
    pub drivers: Vec<Driver>,
    pub model: Option<FermiModel>,
    
    // Current State
    pub current_probability: Option<Probability>,
    pub last_simulation: Option<SimulationResult>,
    
    // Resolution
    pub resolved: bool,
    pub outcome: Option<Outcome>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub brier_score: Option<f64>,
    
    // Versioning
    pub version: Version,
    pub version_history: Vec<ForecastVersion>,
    
    // Metadata
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    pub fn initial() -> Self {
        Version { major: 1, minor: 0 }
    }
    
    pub fn bump_minor(&mut self) {
        self.minor += 1;
    }
    
    pub fn bump_major(&mut self) {
        self.major += 1;
        self.minor = 0;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Private,
    Team(Uuid),  // Team ID
    Organization(Uuid),  // Org ID
    Public,
}

// ============================================
// BASE RATE (Outside View)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRate {
    pub reference_class: String,
    pub historical_frequency: Probability,
    pub sample_size: Option<usize>,
    pub source: String,
    pub confidence: ConfidenceLevel,
    pub generated_by: GenerationSource,
    pub reasoning: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationSource {
    Ai,
    User,
    Database,
}

// ============================================
// DRIVER
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Driver {
    pub id: DriverId,
    pub forecast_id: ForecastId,
    pub name: String,
    pub description: Option<String>,
    pub driver_type: DriverType,
    
    // Quantification
    pub distribution: Distribution,
    
    // For binary drivers
    pub event_probability: Option<Probability>,
    
    // Conditional effects
    pub if_true_effects: Vec<ConditionalEffect>,
    
    // Evidence & Research
    pub evidence: Vec<Evidence>,
    pub attached_agents: Vec<AgentAttachment>,
    
    // Rationale
    pub rationale: Option<String>,
    
    // Constraints
    pub constraints: Vec<Constraint>,
    
    // Versioning
    pub version: Version,
    
    // Metadata
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverType {
    Continuous,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Distribution {
    Triangular {
        p5: f64,
        p50: f64,
        p95: f64,
        unit: Option<String>,
    },
    Normal {
        mean: f64,
        stddev: f64,
        unit: Option<String>,
    },
    Lognormal {
        median: f64,
        sigma: f64,
        unit: Option<String>,
    },
    Uniform {
        low: f64,
        high: f64,
        unit: Option<String>,
    },
    Beta {
        alpha: f64,
        beta: f64,
    },
}

impl Distribution {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Distribution::Triangular { p5, p50, p95, .. } => {
                if p5 >= p50 {
                    return Err(format!("p5 ({}) must be < p50 ({})", p5, p50));
                }
                if p50 >= p95 {
                    return Err(format!("p50 ({}) must be < p95 ({})", p50, p95));
                }
                Ok(())
            }
            Distribution::Normal { mean: _, stddev, .. } => {
                if *stddev <= 0.0 {
                    return Err("stddev must be positive".to_string());
                }
                Ok(())
            }
            Distribution::Lognormal { median, sigma, .. } => {
                if *median <= 0.0 {
                    return Err("median must be positive".to_string());
                }
                if *sigma <= 0.0 {
                    return Err("sigma must be positive".to_string());
                }
                Ok(())
            }
            Distribution::Uniform { low, high, .. } => {
                if low >= high {
                    return Err(format!("low ({}) must be < high ({})", low, high));
                }
                Ok(())
            }
            Distribution::Beta { alpha, beta } => {
                if *alpha <= 0.0 || *beta <= 0.0 {
                    return Err("alpha and beta must be positive".to_string());
                }
                Ok(())
            }
        }
    }
}

// ============================================
// CONSTRAINTS
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub condition: Condition,
    pub action: ConstraintAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    Comparison {
        left: Expression,
        op: ComparisonOp,
        right: Expression,
    },
    Boolean(Expression),
    InRange {
        value: Expression,
        low: f64,
        high: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Identifier(String),
    Number(f64),
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstraintAction {
    Clamp {
        target: DriverId,
        min: Option<f64>,
        max: Option<f64>,
    },
    Shift {
        target: DriverId,
        amount: f64,
    },
    Scale {
        target: DriverId,
        factor: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalEffect {
    pub target_driver: String,
    pub effect_type: EffectType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EffectType {
    Multiply(f64),
    Add(f64),
    SetTo(f64),
}

// ============================================
// EVIDENCE
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub driver_id: DriverId,
    
    // Content
    pub source: String,
    pub content: String,
    pub evidence_type: EvidenceType,
    pub url: Option<String>,
    
    // Impact Assessment
    pub impact_direction: Option<ImpactDirection>,
    pub impact_magnitude: Option<ImpactMagnitude>,
    
    // Metadata
    pub added_by: AddedBy,
    pub created_at: DateTime<Utc>,
    
    // Full data (for AI-generated evidence)
    pub full_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    Research,
    Data,
    Expert,
    Market,
    News,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactDirection {
    Increases,
    Decreases,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactMagnitude {
    Weak,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AddedBy {
    User(UserId),
    Agent {
        agent_type: AgentType,
        run_id: Uuid,
    },
}

// ============================================
// AGENTS
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAttachment {
    pub id: Uuid,
    pub driver_id: DriverId,
    pub agent_type: AgentType,
    pub query: String,
    pub schedule: Schedule,
    pub active: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub triggers: Vec<Trigger>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentType {
    ResearchAnalyst,
    CompetitiveIntel,
    RegulatoryMonitor,
    FinancialAnalyst,
    MarketResearcher,
    SentimentMonitor,
    ExpertSynthesizer,
    TechnologyValidator,
    HiringTracker,
    PricingIntel,
    GrowthSignals,
}

impl AgentType {
    pub fn all() -> Vec<AgentType> {
        vec![
            AgentType::ResearchAnalyst,
            AgentType::CompetitiveIntel,
            AgentType::RegulatoryMonitor,
            AgentType::FinancialAnalyst,
            AgentType::MarketResearcher,
            AgentType::SentimentMonitor,
            AgentType::ExpertSynthesizer,
            AgentType::TechnologyValidator,
            AgentType::HiringTracker,
            AgentType::PricingIntel,
            AgentType::GrowthSignals,
        ]
    }
    
    pub fn yokai_icon(&self) -> &'static str {
        match self {
            AgentType::ResearchAnalyst => "👺",      // Tengu
            AgentType::CompetitiveIntel => "🐱",     // Nekomata
            AgentType::RegulatoryMonitor => "👹",    // Oni
            AgentType::FinancialAnalyst => "🐢",     // Kappa
            AgentType::MarketResearcher => "🦝",     // Tanuki
            AgentType::SentimentMonitor => "🐦‍⬛",    // Karasu
            AgentType::ExpertSynthesizer => "🦊",    // Kitsune
            AgentType::TechnologyValidator => "⚙️",  // Tsukumogami
            AgentType::HiringTracker => "👤",        // Generic
            AgentType::PricingIntel => "💰",         // Generic
            AgentType::GrowthSignals => "🌱",        // Kodama
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Schedule {
    OnDemand,
    Daily,
    Weekly { weekday: Weekday, time: String },
    Monthly,
    Cron(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    Keyword(String),
    SentimentChange { threshold: f64 },
    ValueThreshold { field: String, value: f64 },
}

// ============================================
// FERMI MODEL
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FermiModel {
    pub model_type: ModelType,
    pub equation: Expression,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelType {
    Multiplicative,
    Additive,
    ScenarioWeighted,
}

// ============================================
// SIMULATION
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub id: Uuid,
    pub forecast_id: ForecastId,
    pub iterations: usize,
    
    // Results
    pub probability: Probability,
    pub histogram: Vec<HistogramBin>,
    pub percentiles: Percentiles,
    pub sensitivity: Vec<DriverSensitivity>,
    
    // Metadata
    pub trigger_reason: Option<String>,
    pub executed_at: DateTime<Utc>,
    pub runtime_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBin {
    pub value: f64,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Percentiles {
    pub p10: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p90: f64,
    pub mean: f64,
    pub stddev: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverSensitivity {
    pub driver_id: DriverId,
    pub driver_name: String,
    pub variance_contribution: f64,  // 0.0 to 1.0
}

// ============================================
// OUTCOME & RESOLUTION
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome {
    Binary(bool),
    Numeric(f64),
}

// ============================================
// VERSIONING
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastVersion {
    pub version_id: Uuid,
    pub version: Version,
    pub timestamp: DateTime<Utc>,
    pub change_type: ChangeType,
    pub change_description: String,
    pub snapshot: serde_json::Value,  // Full forecast state
    pub triggered_simulation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Major,  // Significant parameter changes
    Minor,  // Evidence added, minor tweaks
}

// ============================================
// CALIBRATION & TRACKING
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub user_id: UserId,
    pub total_forecasts: usize,
    pub resolved_forecasts: usize,
    pub avg_brier_score: Option<f64>,
    pub calibration_curve: Vec<CalibrationBin>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationBin {
    pub probability_range: (f64, f64),  // e.g., (0.4, 0.5)
    pub predicted_probability: f64,      // avg of forecasts in bin
    pub actual_frequency: f64,           // % that actually happened
    pub count: usize,                    // sample size
}

// ============================================
// EXTERNAL SIGNALS
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSignal {
    pub source: String,
    pub probability: Probability,
    pub timestamp: DateTime<Utc>,
    pub confidence: ConfidenceLevel,
    pub signal_type: SignalType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    AnalystConsensus,
    OptionsImplied,
    PredictionMarket,
    BaseRate,
}

// ============================================
// ARBITRAGE OPPORTUNITY
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub internal_forecast: Probability,
    pub external_signals: Vec<ExternalSignal>,
    pub disequilibrium: Disequilibrium,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disequilibrium {
    pub magnitude: f64,
    pub direction: DisequilibriumDirection,
    pub hypothesis: String,
    pub trade_signal: Option<TradeSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisequilibriumDirection {
    Overvalued,   // Market too optimistic
    Undervalued,  // Market too pessimistic
    Aligned,      // Agreement
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSignal {
    Long,
    Short,
    Neutral,
}
```

---

## Domain Services

```rust
// crates/uffp-core/src/services/forecast_service.rs

use crate::*;
use anyhow::Result;

pub struct ForecastService;

impl ForecastService {
    /// Create a new forecast
    pub fn create_forecast(
        user_id: UserId,
        question: String,
        resolution_criteria: String,
        target_date: NaiveDate,
    ) -> Result<Forecast> {
        Ok(Forecast {
            id: Uuid::new_v4(),
            user_id,
            question,
            resolution_criteria,
            target_date,
            domain: None,
            base_rate: None,
            drivers: vec![],
            model: None,
            current_probability: None,
            last_simulation: None,
            resolved: false,
            outcome: None,
            resolved_at: None,
            brier_score: None,
            version: Version::initial(),
            version_history: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tags: vec![],
            visibility: Visibility::Private,
        })
    }
    
    /// Add driver to forecast
    pub fn add_driver(
        forecast: &mut Forecast,
        driver: Driver,
    ) -> Result<()> {
        driver.distribution.validate()?;
        forecast.drivers.push(driver);
        forecast.version.bump_minor();
        forecast.updated_at = Utc::now();
        Ok(())
    }
    
    /// Calculate Brier score
    pub fn calculate_brier_score(
        probability: Probability,
        outcome: bool,
    ) -> f64 {
        let p = probability.value();
        let o = if outcome { 1.0 } else { 0.0 };
        (p - o).powi(2)
    }
    
    /// Resolve forecast
    pub fn resolve_forecast(
        forecast: &mut Forecast,
        outcome: Outcome,
    ) -> Result<()> {
        if forecast.resolved {
            return Err(anyhow::anyhow!("Forecast already resolved"));
        }
        
        forecast.resolved = true;
        forecast.resolved_at = Some(Utc::now());
        
        // Calculate Brier score for binary outcomes
        if let (Some(prob), Outcome::Binary(outcome_bool)) = 
            (forecast.current_probability, &outcome) {
            forecast.brier_score = Some(
                Self::calculate_brier_score(prob, *outcome_bool)
            );
        }
        
        forecast.outcome = Some(outcome);
        Ok(())
    }
}
```

---

## Validation Rules

```rust
// crates/uffp-core/src/validation.rs

use crate::*;

pub struct ForecastValidator;

impl ForecastValidator {
    pub fn validate(forecast: &Forecast) -> Vec<ValidationError> {
        let mut errors = vec![];
        
        // Check drivers
        if forecast.drivers.is_empty() {
            errors.push(ValidationError::warning(
                "No drivers defined",
                "Fermi decomposition requires at least one driver"
            ));
        }
        
        if forecast.drivers.len() == 1 {
            errors.push(ValidationError::warning(
                "Only one driver",
                "One driver = guessing. Add more for real decomposition."
            ));
        }
        
        // Validate each driver
        for driver in &forecast.drivers {
            if let Err(e) = driver.distribution.validate() {
                errors.push(ValidationError::error(
                    format!("Invalid distribution for {}", driver.name),
                    e
                ));
            }
            
            // Check evidence
            if driver.evidence.is_empty() {
                errors.push(ValidationError::info(
                    format!("No evidence for driver '{}'", driver.name),
                    "Consider adding evidence or attaching research agents"
                ));
            }
        }
        
        // Check base rate
        if forecast.base_rate.is_none() {
            errors.push(ValidationError::warning(
                "No base rate",
                "Start with outside view (base rate) before inside analysis"
            ));
        }
        
        // Check target date
        if forecast.target_date < Utc::now().naive_utc().date() {
            errors.push(ValidationError::error(
                "Target date in past",
                "Target date must be in the future"
            ));
        }
        
        errors
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub severity: Severity,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl ValidationError {
    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            title: title.into(),
            message: message.into(),
        }
    }
    
    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            title: title.into(),
            message: message.into(),
        }
    }
    
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            title: title.into(),
            message: message.into(),
        }
    }
}
```

---

## Repository Interface

```rust
// crates/uffp-core/src/repository.rs

use crate::*;
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait ForecastRepository: Send + Sync {
    async fn save(&self, forecast: &Forecast) -> Result<()>;
    async fn find_by_id(&self, id: ForecastId) -> Result<Option<Forecast>>;
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Forecast>>;
    async fn update(&self, forecast: &Forecast) -> Result<()>;
    async fn delete(&self, id: ForecastId) -> Result<()>;
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, forecast_id: ForecastId, event: ForecastEvent) -> Result<()>;
    async fn get_events(&self, forecast_id: ForecastId) -> Result<Vec<ForecastEvent>>;
    async fn get_snapshot(&self, forecast_id: ForecastId, version: Version) -> Result<Option<Forecast>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForecastEvent {
    Created {
        forecast_id: ForecastId,
        question: String,
        timestamp: DateTime<Utc>,
    },
    DriverAdded {
        driver_id: DriverId,
        driver_name: String,
        timestamp: DateTime<Utc>,
    },
    DriverUpdated {
        driver_id: DriverId,
        changes: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    EvidenceAdded {
        evidence_id: EvidenceId,
        driver_id: DriverId,
        added_by: AddedBy,
        timestamp: DateTime<Utc>,
    },
    SimulationRun {
        simulation_id: Uuid,
        probability: Probability,
        timestamp: DateTime<Utc>,
    },
    Resolved {
        outcome: Outcome,
        brier_score: Option<f64>,
        timestamp: DateTime<Utc>,
    },
}
```

---

## Tests

```rust
// crates/uffp-core/src/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_probability_validation() {
        assert!(Probability::new(0.5).is_ok());
        assert!(Probability::new(0.0).is_ok());
        assert!(Probability::new(1.0).is_ok());
        assert!(Probability::new(-0.1).is_err());
        assert!(Probability::new(1.1).is_err());
    }
    
    #[test]
    fn test_triangular_validation() {
        let valid = Distribution::Triangular {
            p5: 10.0,
            p50: 50.0,
            p95: 100.0,
            unit: Some("USD".to_string()),
        };
        assert!(valid.validate().is_ok());
        
        let invalid = Distribution::Triangular {
            p5: 100.0,
            p50: 50.0,
            p95: 10.0,
            unit: None,
        };
        assert!(invalid.validate().is_err());
    }
    
    #[test]
    fn test_brier_score() {
        let prob = Probability::new(0.7).unwrap();
        
        // Outcome: true
        let score_true = ForecastService::calculate_brier_score(prob, true);
        assert_eq!(score_true, 0.09);  // (0.7 - 1.0)^2 = 0.09
        
        // Outcome: false
        let score_false = ForecastService::calculate_brier_score(prob, false);
        assert_eq!(score_false, 0.49);  // (0.7 - 0.0)^2 = 0.49
    }
    
    #[test]
    fn test_version_bump() {
        let mut v = Version::initial();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        
        v.bump_minor();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 1);
        
        v.bump_major();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
    }
}
```

This domain model provides the complete foundation for the UFFP system!
