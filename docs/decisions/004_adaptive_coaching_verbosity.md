# ADR-004: Adaptive Coaching Verbosity

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** Project team  
**Related:** ADR-003 (Hybrid Coaching), Module 1, [QUESTIONS_BY_MODULE.md](../QUESTIONS_BY_MODULE.md#q14-coaching-verbosity)

---

## Context

Fermi provides coaching suggestions to help users write better forecasts. We need to decide how often and aggressively these suggestions appear.

**User Experience Challenge:**
- **New users** need aggressive coaching to learn best practices
- **Experienced users** find aggressive coaching annoying
- **One-size-fits-all** approach will frustrate someone

**Tetlock Principles:**
- Forecasting improves with practice and feedback
- Calibration comes from repeated cycles
- Different skill levels need different guidance

**Technical Context:**
- We track user interactions with forecasts
- Can measure which suggestions are accepted/dismissed
- Backend stores user history and preferences

---

## Decision

We will implement **Adaptive Coaching Verbosity** that evolves with the user:

**Phase 1: Aggressive Onboarding (First 10 forecasts)**
- Show suggestions on nearly every line
- Explain best practices extensively
- Provide "Learn more" links frequently
- Goal: Teach FPL patterns and forecasting principles

**Phase 2: Moderate Coaching (10-50 forecasts)**
- Only show suggestions for significant issues
- Less verbose explanations
- Focus on common mistakes
- Goal: Reinforce learning, catch errors

**Phase 3: Adaptive (50+ forecasts)**
- Learn from user's accept/dismiss patterns
- Personalized verbosity level
- Focus on user's weak areas (e.g., if they always dismiss range warnings, stop showing them)
- Goal: Helpful without annoying

**User Override:**
- Users can manually set verbosity level (Off / Low / Medium / High)
- Adaptive system respects manual override
- "Reset coaching" option to go back to onboarding mode

---

## Consequences

### Positive Consequences

✅ **Better Onboarding**
- New users learn FPL and forecasting principles quickly
- Aggressive coaching when it's helpful (early learning phase)
- Clear path from beginner to expert

✅ **Reduced Annoyance**
- Experienced users don't get pestered
- System learns what each user cares about
- Respects user preferences over time

✅ **Personalized Experience**
- Different users have different needs
- System adapts to individual patterns
- Can identify user skill level automatically

✅ **Continuous Improvement**
- User feedback (accept/dismiss) trains the system
- Better coaching over time for everyone
- Data on which suggestions are actually helpful

### Negative Consequences

❌ **Complexity**
- Need to track user interactions
- Backend storage for coaching preferences
- ML/heuristics to determine verbosity level
- More code to maintain

❌ **Cold Start Problem**
- New users have no history
- Default to aggressive might annoy power users trying the tool
- Need good "skip onboarding" option

❌ **Privacy Concerns**
- Tracking user behavior (which suggestions dismissed)
- Need clear privacy policy
- Users might not want coaching history stored

### Neutral Consequences

⚖️ **Implementation Phases**
- Phase 1 can ship quickly (count-based)
- Phase 2 requires backend storage
- Phase 3 requires ML/heuristics (can defer)

⚖️ **User Education**
- Need to explain adaptive system to users
- Settings UI to show "Coaching Level: Moderate" and why
- Transparency builds trust

---

## Alternatives Considered

### Alternative A: Every Line (Copilot-style)

Show suggestions aggressively on every line where improvement is possible.

**Pros:**
- Maximum learning for new users
- Never miss an opportunity to teach
- Consistent behavior

**Cons:**
- Extremely annoying for experienced users
- Visual clutter in editor
- Users will disable coaching entirely
- No respect for user skill level

**Why not:** Power users would turn coaching off immediately, losing all benefit.

---

### Alternative B: On Significant Issues Only

Only show coaching for major mistakes (e.g., validation errors, bad patterns).

**Pros:**
- Not annoying
- Clear signal-to-noise ratio
- Won't interrupt flow

**Cons:**
- New users don't learn best practices
- Miss opportunities for proactive teaching
- No guidance on "good but could be better" situations

**Why not:** New users need more hand-holding than this provides.

---

### Alternative C: On Request Only

Users explicitly ask Fermi for help (e.g., "Fermi, review my forecast").

**Pros:**
- Never intrusive
- User controls when they want feedback
- Good for experienced users

**Cons:**
- New users don't know what to ask
- Passive users get no coaching
- Requires extra step (breaks flow)
- Misses real-time teaching opportunities

**Why not:** Real-time coaching is more effective than post-hoc review. We want coaching in the moment.

---

### Alternative D: Static Verbosity Levels

Let users choose one of 4 levels: Off / Low / Medium / High (never adapts).

**Pros:**
- Simple to implement
- Predictable behavior
- User has full control

**Cons:**
- Users have to manually adjust as they learn
- No automatic improvement
- Most users will never change the default
- Doesn't learn from user preferences

**Why not:** Adaptive is strictly better - it's static + learning. We can still offer manual override.

---

## Implementation Notes

### Phase 1: Count-Based (MVP)

**Local storage (no backend required):**
```rust
struct CoachingState {
    forecasts_created: usize,
    verbosity_override: Option<VerbosityLevel>,
}

fn get_verbosity_level(state: &CoachingState) -> VerbosityLevel {
    // User override takes precedence
    if let Some(override_level) = state.verbosity_override {
        return override_level;
    }
    
    // Adaptive based on count
    match state.forecasts_created {
        0..=10 => VerbosityLevel::High,      // Onboarding
        11..=50 => VerbosityLevel::Medium,   // Learning
        _ => VerbosityLevel::Low,            // Experienced
    }
}
```

**Coaching messages check verbosity:**
```rust
fn should_show_suggestion(&self, suggestion_type: SuggestionType) -> bool {
    let verbosity = self.get_verbosity_level();
    
    match verbosity {
        VerbosityLevel::High => true,  // Show all suggestions
        VerbosityLevel::Medium => {
            // Only show important suggestions
            matches!(suggestion_type, 
                SuggestionType::ValidationError | 
                SuggestionType::CommonMistake |
                SuggestionType::BestPractice
            )
        },
        VerbosityLevel::Low => {
            // Only show critical issues
            matches!(suggestion_type,
                SuggestionType::ValidationError |
                SuggestionType::RareButImportant
            )
        },
        VerbosityLevel::Off => false,
    }
}
```

---

### Phase 2: Accept/Dismiss Tracking (Backend)

**Track user interactions:**
```sql
CREATE TABLE coaching_interactions (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    suggestion_type VARCHAR(50) NOT NULL,
    action VARCHAR(20) NOT NULL,  -- 'accepted', 'dismissed', 'ignored'
    created_at TIMESTAMP NOT NULL
);

CREATE INDEX idx_coaching_user ON coaching_interactions(user_id);
```

**Calculate personalized verbosity:**
```rust
async fn get_personalized_verbosity(user_id: i32) -> HashMap<SuggestionType, bool> {
    // Query last 100 interactions
    let interactions = db.query(
        "SELECT suggestion_type, action FROM coaching_interactions 
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 100",
        &[&user_id]
    ).await?;
    
    // Calculate accept rate per suggestion type
    let mut accept_rates = HashMap::new();
    for interaction in interactions {
        let suggestion_type = interaction.get("suggestion_type");
        let action = interaction.get("action");
        
        let rate = accept_rates.entry(suggestion_type).or_insert((0, 0));
        if action == "accepted" {
            rate.0 += 1;  // Accepted count
        }
        rate.1 += 1;  // Total count
    }
    
    // Show suggestion if accept rate > 20%
    accept_rates.iter()
        .map(|(type, (accepted, total))| {
            let rate = *accepted as f32 / *total as f32;
            (type.clone(), rate > 0.2)
        })
        .collect()
}
```

---

### Phase 3: ML-Based (Future)

**Features for ML model:**
- User's Brier score history (calibration)
- Accept/dismiss patterns per suggestion type
- Forecast complexity (number of drivers, model complexity)
- Time spent on forecast
- Evidence quality (sources provided)

**Model output:**
- Probability that user will find suggestion helpful
- Only show if P(helpful) > threshold

**This is future work** - Phase 1 and 2 provide most of the value.

---

### Settings UI

**Coaching Settings Panel:**
```
┌─────────────────────────────────────┐
│ Fermi Coaching Settings             │
├─────────────────────────────────────┤
│ Verbosity Level:                    │
│ ● Adaptive (recommended)            │
│ ○ Off                               │
│ ○ Low                               │
│ ○ Medium                            │
│ ○ High                              │
│                                     │
│ Current Level: Medium               │
│ (Based on 23 forecasts created)    │
│                                     │
│ [ Reset Coaching ]                  │
│ Returns to onboarding mode          │
│                                     │
│ Learn more: How coaching works →    │
└─────────────────────────────────────┘
```

---

### Privacy & Transparency

**Privacy Policy:**
- Coaching interactions stored locally by default
- Optional: Sync to backend for cross-device experience
- Users can view and delete coaching history
- Anonymous aggregation for improving suggestions (opt-in)

**Transparency:**
- Show "Coaching Level: X" in status bar
- Explain why level changed ("You've completed 10 forecasts!")
- Let users see their coaching stats

---

## Testing Strategy

**Phase 1 Tests:**
- Unit tests for count-based verbosity
- Test transitions at 10, 50 forecast thresholds
- Test manual override works

**Phase 2 Tests:**
- Test accept/dismiss tracking
- Test personalized verbosity calculation
- Test edge cases (all dismissed, all accepted)

**User Testing:**
- A/B test: adaptive vs. static high verbosity
- Measure: coaching dismissal rate, user retention
- Survey: "Was coaching helpful?"

---

## Success Metrics

**Onboarding:**
- New users complete first forecast in <10 minutes
- >80% of new users follow at least one suggestion

**Retention:**
- Experienced users keep coaching enabled (not disabled)
- <10% dismissal rate for shown suggestions

**Learning:**
- User calibration improves over first 20 forecasts
- Brier scores decrease with coaching

---

## References

- Tetlock, Philip. *Superforecasting* - Adaptive learning
- Copilot adoption studies - Aggressive suggestions and user tolerance
- Related: ADR-003 (Hybrid Coaching Integration)
- Question answered: Q1.4 in QUESTIONS_BY_MODULE.md

---

## Revision History

- **2026-02-04:** Initial version - Status: Accepted
