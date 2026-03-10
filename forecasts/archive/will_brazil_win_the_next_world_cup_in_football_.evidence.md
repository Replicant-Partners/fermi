# will brazil win the next world cup in football?

**Probability:** 9.9% · **Version:** v7 · **Updated:** 2026-03-08 11:10 UTC

**Confidence:** Low (14%) · **Drivers:** 5 · **Evidence:** 1 · **Agents:** 0

---

## Inside View

**Probability: 9.9%**

Starting from a 12.5% base rate, our model moderately decreases the probability to 9.9%. The key factors are: current_team_strength, coaching_quality, competition_strength. Most influential: competition_strength (39%), coaching_quality (26%), injury_and_form (22%).

**Forecast Confidence:** Low (14%)

**Divergence from base rate:** 3pp below (9.9% vs 12.5%)

---

## Outside View (Base Rate)

**12.5%** — World Cup wins by any nation in modern era (1930-2022)

- **Sample size:** n=8
- **Source:** macro_forecaster

Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors.

---

## 1. current_team_strength `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.60 | 1.00 | 1.25 | multiplier |

> Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

---

## 2. coaching_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.40 | 0.95 | 1.15 | multiplier |

> Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

---

## 3. competition_strength `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.40 | 1.00 | 1.40 | multiplier |

> Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

---

## 4. tournament_location `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.90 | 1.05 | 1.25 | multiplier |

> 2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

---

## 5. injury_and_form `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.30 | 1.20 | 1.40 | multiplier |

> Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

---

## General Evidence (1)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50%

```json
{
  "base_rate": {
    "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
    "historical_frequency": 0.125,
    "sample_size": 8,
    "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
  },
  "drivers": [
    {
      "name": "current_team_strength",
      "display_name": "Current Team Strength",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.0,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "Brazil's current FIFA ranking (#5-6), squad depth, and recent performance in qualifiers. Talented young players like Vinicius Jr but aging core and tactical inconsistency create uncertainty."
    },
    {
      "name": "coaching_quality",
      "display_name": "Coaching and Tactical Setup",
      "type": "continuous",
      "p5": 0.75,
      "p50": 0.95,
      "p95": 1.15,
      "unit": "multiplier",
      "rationale": "Recent coaching instability and tactical struggles in major tournaments (2022 quarterfinal exit). Current setup less proven than historical Brazilian success periods."
    },
    {
      "name": "competition_strength",
      "display_name": "Competition Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.85,
      "p95": 1.0,
      "unit": "multiplier",
      "rationale": "Strong competition from Argentina (current champions), France, England, Spain. European teams have dominated recent tournaments (4 of last 5 winners). Increased global parity reduces any single team's chances."
    },
    {
      "name": "tournament_location",
      "display_name": "Tournament Location Effect",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.05,
      "p95": 1.25,
      "unit": "multiplier",
      "rationale": "2026 World Cup in North America. Moderate travel advantage over European teams, familiar conditions for South American team, strong Brazilian diaspora support. Less advantage than home tournament but meaningful."
    },
    {
      "name": "injury_and_form",
      "display_name": "Injury Luck and Tournament Form",
      "type": "continuous",
      "p5": 0.6,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Key player availability and peak form timing are crucial. Brazil has depth but injuries to stars like Vinicius or Rodrygo would significantly impact chances. Tournament momentum is unpredictable."
    }
  ],
  "evidence": [
    {
      "source": "FIFA World Cup Historical Records",
      "summary": "Brazil has 5 titles (most ever) but last won in 2002. Recent exits: 2022 QF, 2018 QF, 2014 SF.",
      "key_findings": [
        "22-year title drought is longest since 1958-1970",
        "European teams won 4 of last 5 tournaments",
        "Home continent advantage declining in modern era"
      ],
      "relevance": 0.95
    },
    {
      "source": "FIFA Rankings and Recent Performance 2023-2024",
      "summary": "Brazil ranked 5th-6th globally, strong qualifying record but inconsistent against top European opposition.",
      "key_findings": [
        "Solid CONMEBOL qualifying position",
        "Mixed results in friendlies vs top 10 teams",
        "Young attacking talent emerging"
      ],
      "relevance": 0.85
    },
    {
      "source": "Tournament Statistics Analysis",
      "summary": "World Cup winners typically peak at right moment with settled tactics and injury-free squads.",
      "key_findings": [
        "Last 6 winners had coaching continuity 2+ years",
        "Average winner age profile 26-28 years",
        "Knockout stage performance heavily influenced by draw luck"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form",
  "confidence": 0.65,
  "reasoning": "Base rate of 12.5% reflects competitive field of elite nations. Brazil's historical success and talent pool support chances, but recent underperformance, coaching instability, and strong European competition are headwinds. 2026 location provides modest boost. High uncertainty in injury/form timing. Estimate: 8-12% probability."
}
```

**Key findings:**

- "base_rate": {
- "reference_class": "World Cup wins by any nation in modern era (1930-2022)",
- "historical_frequency": 0.125,
- "sample_size": 8,
- "reasoning": "Brazil has won 5 of 22 World Cups (22.7%). However, for next tournament baseline, using 1/8 teams (traditional powers) provides conservative estimate before adjusting for Brazil-specific factors."
- "drivers": [
- "name": "current_team_strength",
- "display_name": "Current Team Strength",
- "type": "continuous",
- "p5": 0.85,

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: current_team_strength * coaching_quality * competition_strength * tournament_location * injury_and_form
```


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v7 · 2026-03-08 11:10 UTC_
