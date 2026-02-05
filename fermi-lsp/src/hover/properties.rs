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

        "evidence_refs" => "**evidence_refs** - Link to supporting evidence\n\nReferences evidence blocks that support this driver's assumptions.\n\n**Example:**\n```fpl\nevidence_refs: [\"market_report\", \"internal_data\"]\n```\n\n**Best practice:** Link all major drivers to evidence sources for transparency",

        "key_findings" => "**key_findings** - Key points from evidence\n\nList of important findings or data points from the evidence.\n\n**Example:**\n```fpl\nkey_findings: [\n    \"Revenue up 22% YoY\",\n    \"Customer count: 1,240\"\n]\n```\n\n**Used in:** evidence blocks to document specific insights",

        "source" => "**source** - Evidence source name\n\nThe title or name of the evidence source.\n\n**Example:** `source: \"Gartner Market Analysis 2026\"`\n\n**Used in:** evidence blocks",

        "summary" => "**summary** - Brief summary of evidence\n\nA concise summary of what the evidence shows.\n\n**Example:** `summary: \"Market expected to grow 15-18% in 2026\"`\n\n**Used in:** evidence blocks",

        "url" => "**url** - Link to evidence source\n\nA URL link to the full evidence source.\n\n**Example:** `url: \"https://example.com/report-2026\"`\n\n**Used in:** evidence blocks",

        "relevance" => "**relevance** - Evidence relevance score (0-1)\n\nHow relevant this evidence is to your forecast.\n\n**Example:** `relevance: 0.85` (85% relevant)\n\n**Used in:** evidence blocks\n\n**Display:** High (0.8+) = green, Medium (0.5-0.79) = yellow, Low (<0.5) = red",

        "date" => "**date** - Evidence date\n\nWhen the evidence was published or collected.\n\n**Example:** `date: 2026-01-15` or `date: \"2026-01-15\"`\n\n**Used in:** evidence blocks",

        "strength" => "**strength** - Evidence quality/strength score (0-1)\n\nHow strong or reliable this evidence is.\n\n**Example:** `strength: 0.9` (90% confidence in evidence quality)\n\n**Used in:** evidence blocks\n\n**Interpretation:** High (0.8+) = strong evidence, Medium (0.5-0.79) = moderate, Low (<0.5) = weak",

        // Agent properties
        "executor" => "**executor** - Agent execution backend\n\nSpecifies how this agent executes its query.\n\n**Values:**\n- `llm` - Use LLM (Claude) to research query\n- `mcp` - Call MCP tools for data\n- `manual` - Human-in-the-loop\n- `skill` - Invoke Anthropic skill\n\n**Example:**\n```fpl\nagent market_research {\n    executor: \"llm\"\n}\n```\n\n**Default:** llm (if not specified)",

        "driver_refs" => "**driver_refs** - Drivers this agent informs\n\nList of driver names that this agent's research should update or inform.\n\n**Format:** Array of driver names\n\n**Example:**\n```fpl\nagent market_research {\n    type: \"research\"\n    query: \"AMD market share trends\"\n    driver_refs: [\"market_share\", \"growth_rate\"]\n}\n```\n\n**Used in:** agent blocks\n\n**Validation:** Driver names must exist in the forecast",

        // Base rate properties (Outside View - Tetlock methodology)
        "reference_class" => "**reference_class** - Reference class definition\n\nDefine the broader category of similar situations to establish a base rate.\n\n**Example:** `reference_class: \"Major tech acquisitions in semiconductor industry\"`\n\n**Used in:** base_rate blocks\n\n**Best practice:** Choose a class that is as similar as possible to your question while having enough historical data.",

        "historical_frequency" => "**historical_frequency** - How often it happened before\n\nThe proportion of times the outcome occurred in the reference class.\n\n**Example:** `historical_frequency: 0.35p` (35% of the time)\n\n**Used in:** base_rate blocks\n\n**Format:** Probability value (0-1) with optional 'p' suffix",

        "sample_size" => "**sample_size** - Number of historical cases\n\nHow many cases were examined to determine the base rate.\n\n**Example:** `sample_size: 127`\n\n**Used in:** base_rate blocks\n\n**Best practice:** Larger samples = more reliable base rates",

        "reasoning" => "**reasoning** - Explanation of the analysis\n\nExplain why you chose this reference class and how you determined the frequency.\n\n**Example:** `reasoning: \"Analyzed all tech M&A deals over $1B since 2010\"`\n\n**Used in:** base_rate blocks and driver blocks",

        "generated_by" => "**generated_by** - Source of the base rate\n\nWho or what generated this base rate estimate.\n\n**Values:** `human` or agent name\n\n**Example:** `generated_by: human`\n\n**Used in:** base_rate blocks",

        _ => return None,
    };

    Some(text.to_string())
}
