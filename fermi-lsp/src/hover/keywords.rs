/// Get hover documentation for keywords and control flow
pub fn get_keyword_hover(word: &str) -> Option<String> {
    let text = match word {
        // Top-level keywords
        "question" => "**question** - Define the forecast question\n\nThe core prediction you want to make.\n\n**Syntax:** `question \"Will X happen by date Y?\"`\n\n**Example:**\n```fpl\nquestion \"Will AMD reach $200 by 2026-12-31?\"\n```",

        "driver" => "**driver** - Define a driver variable\n\nA factor that influences the forecast outcome.\n\n**Syntax:** `driver <name> <type> { ... }`\n\n**Types:** `continuous`, `binary`, `discrete`\n\n**Example:**\n```fpl\ndriver market_size continuous {\n    distribution: triangular(100, 200, 500)\n    unit: \"millions\"\n}\n```",

        "model" => "**model** - Define the forecast model\n\nMathematical expression combining drivers.\n\n**Syntax:** `model: <expression>`\n\n**Example:**\n```fpl\nmodel: base_revenue * growth_rate * market_multiplier\n```",

        "simulate" => "**simulate** - Run Monte Carlo simulation\n\nGenerate probabilistic outcomes from your model.\n\n**Syntax:** `simulate <n> iterations`\n\n**Example:**\n```fpl\nsimulate 10000 iterations\n```\n\nHigher iteration counts give more accurate results but take longer.",

        "evidence" => "**evidence** - Document supporting evidence\n\nTrack sources and data that inform your forecast.\n\n**Syntax:** `evidence <name> { ... }`\n\n**Example:**\n```fpl\nevidence analyst_report {\n    source: \"Morgan Stanley Q4 2025\"\n    summary: \"Projected 25% YoY growth\"\n    relevance: 0.85\n}\n```",

        "agent" => "**agent** - Create automated research agent\n\nScheduled agent that monitors and tracks information over time.\n\n**Syntax:** `agent <name> { ... }`\n\n**Example:**\n```fpl\nagent market_monitor {\n    query: \"semiconductor market growth\"\n    schedule: every 1 week\n}\n```",

        "base_rate" => "**base_rate** - Define base rate (outside view)\n\nEstablish a reference class and historical frequency following Tetlock's superforecasting methodology.\n\n**Syntax:** Inside question block\n\n**Example:**\n```fpl\nquestion \"Will AMD reach $200?\" {\n    base_rate {\n        reference_class: \"Tech stocks doubling in 1 year\"\n        historical_frequency: 0.15p\n        sample_size: 100\n        source: \"Historical analysis 2015-2025\"\n        reasoning: \"Few tech stocks double quickly\"\n        generated_by: human\n    }\n}\n```\n\n**Key concept:** Start with how often similar things happened before (outside view) before adjusting for specifics (inside view).",

        // Driver types
        "continuous" => "**continuous** - Continuous probability distribution\n\nFor numeric values that can vary across a range.\n\n**Use for:** prices, sizes, counts, rates, percentages\n\n**Requires:** `distribution` property with a distribution function\n\n**Example:**\n```fpl\ndriver revenue continuous {\n    distribution: triangular(1000, 2000, 5000)\n    unit: \"dollars\"\n}\n```",

        "binary" => "**binary** - Binary outcome (yes/no)\n\nFor true/false, will-it-happen questions.\n\n**Use for:** events that either happen or don't\n\n**Requires:** `probability` property (0-1)\n\n**Optional:** `impact_multiplier` for model effect\n\n**Example:**\n```fpl\ndriver major_deal binary {\n    probability: 0.65p\n    impact_multiplier: 1.4\n}\n```",

        "discrete" => "**discrete** - Discrete values with probabilities\n\nFor specific outcome options with assigned probabilities.\n\n**Use for:** categorical outcomes, multiple scenarios\n\n**Requires:** `values` array and `weights` array (must sum to 1)\n\n**Example:**\n```fpl\ndriver market_scenario discrete {\n    values: [0.8, 1.0, 1.3]\n    weights: [0.2, 0.5, 0.3]\n}\n```",

        // Control flow
        "if" => "**if-then-else**\n\nConditional expression.\n\n**Syntax:** `if condition then true_value else false_value`\n\n**Example:** `if revenue > 1000 then 1.2 else 1.0`",

        "then" => "**then** - True branch of conditional\n\nValue to use when condition is true.\n\n**Used in:** `if condition then <this_value> else other_value`",

        "else" => "**else** - False branch of conditional\n\nValue to use when condition is false.\n\n**Used in:** `if condition then other_value else <this_value>`",

        // Logical operators
        "and" => "**and** - Logical AND operator\n\nReturns true only if both conditions are true.\n\n**Example:** `if price > 100 and volume > 1000 then ...`",

        "or" => "**or** - Logical OR operator\n\nReturns true if either condition is true.\n\n**Example:** `if scenario_a or scenario_b then ...`",

        "not" => "**not** - Logical NOT operator\n\nInverts a boolean value.\n\n**Example:** `if not failed then 1.0 else 0.5`",

        _ => return None,
    };

    Some(text.to_string())
}
