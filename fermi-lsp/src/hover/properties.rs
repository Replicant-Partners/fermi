/// Get hover documentation for driver properties
pub fn get_property_hover(word: &str) -> Option<String> {
    let text = match word {
        // Driver properties
        "display_name" => "**display_name** - Natural language name for driver\n\nProvides a human-readable name that appears in simulation output.\n\n**Example:**\n```fpl\ndisplay_name: \"Base Sales Revenue\"\n```\n\n**Benefits:**\n- Makes output more readable\n- Easier to understand simulation results\n- Better communication with stakeholders",

        "description" => "**description** - Natural language description\n\nExplains what this driver represents in plain English.\n\n**Example:**\n```fpl\ndescription: \"The baseline sales figure before seasonal adjustments\"\n```\n\n**Best practice:** Write clear descriptions that non-technical users can understand",

        "distribution" => "**distribution** - Probability distribution function\n\nDefines how values are distributed for continuous drivers.\n\n**Available functions:**\n- `triangular(p5, p50, p95)` - Expert estimates\n- `normal(mean, stddev)` - Natural variations\n- `lognormal(median, sigma)` - Prices, incomes\n- `uniform(low, high)` - Complete uncertainty\n- `beta(alpha, beta)` - Probabilities\n- `exponential(lambda)` - Wait times",

        "probability" => "**probability** - Probability value for binary drivers\n\nChance that the binary outcome is true (0-1).\n\n**Format:** Decimal (0.65) or with 'p' suffix (0.65p for 65%)\n\n**Example:** `probability: 0.7p` means 70% chance",

        "unit" => "**unit** - Unit of measurement\n\nDescribes what the driver values represent.\n\n**Example:** \"dollars\", \"percent\", \"millions\", \"units per day\"",

        "rationale" => "**rationale** - Explanation of estimate\n\nDocument why you chose these values or this distribution.\n\n**Best practice:** Include reasoning and key assumptions.",

        "impact_multiplier" => "**impact_multiplier** - Multiplier for binary driver impact\n\nHow much this binary driver affects the model when true.\n\n**Example:** `1.3` means 30% increase if true\n\n**Only used with binary drivers in if-then-else expressions.**",

        "values" => "**values** - List of possible values for discrete drivers\n\nDefines the specific numeric outcomes for a discrete distribution.\n\n**Example:**\n```fpl\nvalues: [0.8, 1.0, 1.3]\n```\n\n**Must match:** Length must equal length of weights array\n\n**Use for:** Scenarios, market states, or categorical outcomes",

        "weights" => "**weights** - Probability weights for discrete driver\n\nDefines the probability of each value occurring.\n\n**Example:**\n```fpl\nweights: [0.2, 0.5, 0.3]\n```\n\n**Requirements:**\n- Must sum to 1.0\n- All weights must be non-negative\n- Length must match values array",

        "min" => "**min** - Minimum value\n\nHard lower bound for the driver.\n\n**Example:** `min: 0`",

        "max" => "**max** - Maximum value\n\nHard upper bound for the driver.\n\n**Example:** `max: 100`",

        _ => return None,
    };

    Some(text.to_string())
}
