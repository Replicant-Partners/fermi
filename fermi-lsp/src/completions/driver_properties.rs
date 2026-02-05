use super::builder::CompletionBuilder;
use tower_lsp::lsp_types::*;

/// Get driver property completions (inside driver blocks)
pub fn get_driver_property_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::property("display_name")
            .detail("Natural language name for this driver")
            .docs("Makes simulation output more readable. Example: \"Base Sales Revenue\"")
            .snippet("display_name: \"${1:Human Readable Name}\"")
            .sort("00_display_name")
            .build(),
        CompletionBuilder::property("description")
            .detail("Explain what this driver represents in plain English")
            .docs("Example: \"The baseline sales figure before adjustments\"")
            .snippet("description: \"${1:Natural language description}\"")
            .sort("01_description")
            .build(),
        CompletionBuilder::property("distribution")
            .detail("Probability distribution function for continuous drivers")
            .docs("Choose from: triangular, normal, lognormal, uniform, beta")
            .snippet("distribution: ${1:triangular(${2:p5}, ${3:p50}, ${4:p95})}")
            .sort("02_distribution")
            .build(),
        CompletionBuilder::property("probability")
            .detail("Probability value for binary drivers (0-1 or with 'p' suffix)")
            .docs("Example: probability: 0.65p for 65%")
            .snippet("probability: ${1:0.5}${2|,p|}")
            .sort("03_probability")
            .build(),
        CompletionBuilder::property("unit")
            .detail("Unit of measurement for the driver")
            .docs("Example: \"dollars\", \"percent\", \"millions\"")
            .snippet("unit: \"${1:units}\"")
            .sort("04_unit")
            .build(),
        CompletionBuilder::property("rationale")
            .detail("Explanation of your estimate or assumptions")
            .docs("Document why you chose these values")
            .snippet("rationale: \"${1:reasoning}\"")
            .sort("05_rationale")
            .build(),
        CompletionBuilder::property("impact_multiplier")
            .detail("Multiplier for how this driver affects the model (for binary drivers)")
            .docs("Example: 1.3 means 30% increase if true")
            .snippet("impact_multiplier: ${1:1.0}")
            .sort("06_impact_multiplier")
            .build(),
        CompletionBuilder::property("min")
            .detail("Minimum value for the driver")
            .docs("Hard lower bound")
            .snippet("min: ${1:0}")
            .sort("07_min")
            .build(),
        CompletionBuilder::property("max")
            .detail("Maximum value for the driver")
            .docs("Hard upper bound")
            .snippet("max: ${1:100}")
            .sort("08_max")
            .build(),
        CompletionBuilder::property("values")
            .detail("List of possible values for discrete drivers")
            .docs("Example: values: [10, 20, 30, 40]")
            .snippet("values: [${1:value1}, ${2:value2}, ${3:value3}]")
            .sort("09_values")
            .build(),
        CompletionBuilder::property("weights")
            .detail("Probability weights for discrete driver values")
            .docs("Must sum to 1.0")
            .snippet("weights: [${1:0.25}, ${2:0.5}, ${3:0.25}]")
            .sort("10_weights")
            .build(),
    ]
}
