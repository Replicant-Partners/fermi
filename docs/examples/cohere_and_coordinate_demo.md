# Cohere & Coordinate — Example Workspace Session

## Setup

A workspace called **"Climate Policy Research"** with 4 agents:

| Agent | Role | Type |
|-------|------|------|
| `research_agent` | Evidence gathering | research |
| `fermi_estimator` | Quantitative estimation | research |
| `coherence_consultant` | Discourse quality advisor | coherence |
| `cohere_and_coordinate` | Workspace conductor | coherence |

## The Conversation So Far

```
[research_agent]: Based on IPCC AR6 data, global surface temperature has 
increased by approximately 1.1°C since pre-industrial levels. Agricultural 
yields in tropical regions have already declined 2-5% per decade.

[fermi_estimator]: I estimate the economic impact of a 2°C scenario at 
$23 trillion annually by 2100, based on damage functions from Nordhaus (2017) 
and updated population projections.

[research_agent]: The EU Carbon Border Adjustment Mechanism (CBAM) went into 
full effect in 2026. Early data suggests a 12% reduction in carbon-intensive 
imports.

[fermi_estimator]: However, the global cost of climate adaptation is already 
$300 billion per year. The gap between needed and available finance is growing.

[research_agent]: Some studies argue the social cost of carbon should be 
$185/ton, not the $51/ton used in US federal regulation. This fundamentally 
changes the cost-benefit calculus.

[fermi_estimator]: My estimate actually uses $51/ton. If we use $185/ton, the 
economic impact number jumps to $54 trillion — more than double. The two 
numbers can't both be right.
```

## Invoking Cohere & Coordinate

**User**: "How coherent is this workspace? What should we fix?"

### Step 1: Agent reads the room

The agent calls three tools:

```
→ coherence_snapshot()
  No previous evaluations found.

→ evaluate_coherence(message_limit: 50)
  {
    "global_score": 0.58,
    "quality_label": "Moderate",
    "principle_scores": {
      "P1_Symmetry": 0.72,
      "P2_Explanation": 0.61,
      "P3_Analogy": 0.15,
      "P4_Data_Priority": 0.82,
      "P5_Contradiction": 0.35,
      "P6_Competition": 0.40,
      "P7_Acceptability": 0.55
    },
    "health_indicators": {
      "feedback_action": "TargetedFeedback",
      "converged": true,
      "evidence_density": 0.67,
      "explanation_density": 0.33
    }
  }

→ get_workspace_messages(limit: 20)
  [6 messages returned]

→ list_workspace_agents()
  [research_agent, fermi_estimator, coherence_consultant, cohere_and_coordinate]
```

### Step 2: Agent diagnoses

**cohere_and_coordinate responds**:

> ## Workspace Coherence Report
> 
> **Global Score: 58% (Moderate)** — The workspace is producing solid evidence 
> but has unresolved tension that's dragging coherence down.
> 
> ### What's Working
> 
> - **Data Priority (P4): 82%** — Both agents are grounding claims in evidence 
>   (IPCC data, Nordhaus damage functions, CBAM statistics). This is strong.
> - **Symmetry (P1): 72%** — Agents are engaging with each other's contributions, 
>   not talking past each other.
> 
> ### What Needs Attention
> 
> - **Contradiction (P5): 35%** — There's an unresolved conflict: fermi_estimator's 
>   $23T estimate uses $51/ton social cost of carbon, but research_agent surfaced 
>   studies arguing for $185/ton. Fermi_estimator acknowledged this changes the 
>   number to $54T but said "the two numbers can't both be right." This 
>   **contradiction is hanging open** — nobody has resolved which to use or why.
> 
> - **Competition (P6): 40%** — Related: there are two competing economic 
>   frameworks (Nordhaus $51/ton vs. $185/ton). The workspace needs to either 
>   pick one with justification, or explicitly model both scenarios.
> 
> - **Analogy (P3): 15%** — Nobody has drawn structural parallels. Are there 
>   historical analogues for carbon pricing impacts? Has CBAM been compared to 
>   previous trade mechanisms?
> 
> ### Coordination Recommendations
> 
> 1. **research_agent**: Find the source arguing for $185/ton social cost of 
>    carbon. What methodology do they use? How does it differ from Nordhaus?
>    This evidence will resolve the P5 contradiction.
> 
> 2. **fermi_estimator**: Run **both** scenarios ($51/ton and $185/ton) as a 
>    range estimate rather than a point estimate. This converts a contradiction 
>    into a productive uncertainty band. Re-estimate with explicit assumptions.
> 
> 3. **Both agents**: Draw at least one analogy — compare CBAM's 12% import 
>    reduction to historical trade policy impacts (e.g., Smoot-Hawley, EU 
>    REACH regulation). This will lift P3 significantly.
> 
> **Next step**: research_agent should go first (evidence gathering for the 
> SCC debate), then fermi_estimator re-runs with both scenarios. Trigger 
> another coherence evaluation after that round.

### Step 3: Agent writes coordination brief

```
→ write_workspace_file(
    path: "_coordination/brief_001.md",
    content: "# Coordination Brief #1\n\nCoherence: 58%...",
    commit_message: "coordination: brief #1 — resolve SCC contradiction"
  )
```

## After the Recommended Actions

The agents follow the recommendations. Research_agent finds the Rennert et al. (2022) paper arguing $185/ton. Fermi_estimator produces a range: $23T-$54T depending on SCC assumption.

A second evaluation shows:

```
{
  "global_score": 0.78,
  "quality_label": "Strong",
  "principle_scores": {
    "P1_Symmetry": 0.80,
    "P2_Explanation": 0.75,
    "P3_Analogy": 0.45,
    "P4_Data_Priority": 0.88,
    "P5_Contradiction": 0.72,
    "P6_Competition": 0.70,
    "P7_Acceptability": 0.78
  }
}
```

Coherence jumped from 58% to 78%. The contradiction was resolved by framing it as a range with explicit assumptions. Competition was resolved by modeling both scenarios.

P3 (Analogy) improved but still has room — the CBAM/historical comparison was made but could be deeper.

## Why This Matters

Without `cohere_and_coordinate`:
- The $51/ton vs $185/ton contradiction would sit unresolved
- Both agents would continue producing outputs with incompatible assumptions
- The workspace would generate volume but not integrated knowledge

With it:
- The specific coordination failure is diagnosed (P5: contradiction, P6: competition)
- Each agent gets a concrete next action
- The contradiction becomes a productive uncertainty range
- Coherence measurably improves (58% → 78%)

This is the difference between agents that talk and agents that think together.
