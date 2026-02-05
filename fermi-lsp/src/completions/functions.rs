use super::builder::CompletionBuilder;
use tower_lsp::lsp_types::*;

/// Get distribution function completions
pub fn get_distribution_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::function("triangular")
            .detail("Three-point distribution (p5, p50, p95)")
            .docs("Best for expert estimates with min/likely/max.\nExample: triangular(1000, 2000, 5000)")
            .snippet("triangular(${1:p5}, ${2:p50}, ${3:p95})")
            .sort("00_triangular")
            .build(),

        CompletionBuilder::function("normal")
            .detail("Normal distribution (mean, stddev)")
            .docs("Best for natural variations, symmetric.\nExample: normal(100, 15)")
            .snippet("normal(${1:mean}, ${2:stddev})")
            .sort("01_normal")
            .build(),

        CompletionBuilder::function("lognormal")
            .detail("Lognormal distribution (median, sigma)")
            .docs("Best for prices, incomes (positive only).\nExample: lognormal(50000, 0.5)")
            .snippet("lognormal(${1:median}, ${2:sigma})")
            .sort("02_lognormal")
            .build(),

        CompletionBuilder::function("uniform")
            .detail("Uniform distribution (low, high)")
            .docs("Best for complete uncertainty within range.\nExample: uniform(0, 100)")
            .snippet("uniform(${1:low}, ${2:high})")
            .sort("03_uniform")
            .build(),

        CompletionBuilder::function("beta")
            .detail("Beta distribution (alpha, beta)")
            .docs("Best for probabilities, percentages [0-1].\nExample: beta(2, 5)")
            .snippet("beta(${1:alpha}, ${2:beta})")
            .sort("04_beta")
            .build(),

        CompletionBuilder::function("exponential")
            .detail("Exponential distribution (lambda)")
            .docs("Best for wait times, time to failure.\nExample: exponential(0.5) for mean of 2")
            .snippet("exponential(${1:lambda})")
            .sort("05_exponential")
            .build(),
    ]
}

/// Get math function completions
pub fn get_math_function_completions() -> Vec<CompletionItem> {
    vec![
        CompletionBuilder::function("sqrt")
            .detail("Square root")
            .docs("Returns √x. Example: sqrt(16) = 4")
            .snippet("sqrt(${1:x})")
            .sort("00_sqrt")
            .build(),
        CompletionBuilder::function("log")
            .detail("Natural logarithm (base e)")
            .docs("Returns ln(x). Example: log(2.71828) ≈ 1")
            .snippet("log(${1:x})")
            .sort("01_log")
            .build(),
        CompletionBuilder::function("log10")
            .detail("Base-10 logarithm")
            .docs("Returns log₁₀(x). Example: log10(100) = 2")
            .snippet("log10(${1:x})")
            .sort("02_log10")
            .build(),
        CompletionBuilder::function("exp")
            .detail("Exponential function")
            .docs("Returns e^x. Example: exp(1) ≈ 2.71828")
            .snippet("exp(${1:x})")
            .sort("03_exp")
            .build(),
        CompletionBuilder::function("pow")
            .detail("Power function")
            .docs("Returns base^exponent. Example: pow(2, 8) = 256")
            .snippet("pow(${1:base}, ${2:exp})")
            .sort("04_pow")
            .build(),
        CompletionBuilder::function("abs")
            .detail("Absolute value")
            .docs("Returns |x|. Example: abs(-5) = 5")
            .snippet("abs(${1:x})")
            .sort("05_abs")
            .build(),
        CompletionBuilder::function("min")
            .detail("Minimum of two values")
            .docs("Example: min(10, 20) = 10")
            .snippet("min(${1:a}, ${2:b})")
            .sort("06_min")
            .build(),
        CompletionBuilder::function("max")
            .detail("Maximum of two values")
            .docs("Example: max(10, 20) = 20")
            .snippet("max(${1:a}, ${2:b})")
            .sort("07_max")
            .build(),
        CompletionBuilder::function("round")
            .detail("Round to nearest integer")
            .docs("Example: round(3.7) = 4")
            .snippet("round(${1:x})")
            .sort("08_round")
            .build(),
        CompletionBuilder::function("floor")
            .detail("Round down to integer")
            .docs("Example: floor(3.7) = 3")
            .snippet("floor(${1:x})")
            .sort("09_floor")
            .build(),
        CompletionBuilder::function("ceil")
            .detail("Round up to integer")
            .docs("Example: ceil(3.2) = 4")
            .snippet("ceil(${1:x})")
            .sort("10_ceil")
            .build(),
        CompletionBuilder::function("sin")
            .detail("Sine function (radians)")
            .docs("Returns sine value (-1 to 1). Example: sin(1.5708) ≈ 1")
            .snippet("sin(${1:x})")
            .sort("11_sin")
            .build(),
        CompletionBuilder::function("cos")
            .detail("Cosine function (radians)")
            .docs("Returns cosine value (-1 to 1). Example: cos(0) = 1")
            .snippet("cos(${1:x})")
            .sort("12_cos")
            .build(),
        CompletionBuilder::function("tan")
            .detail("Tangent function (radians)")
            .docs("Returns tangent value. Example: tan(0.785398) ≈ 1")
            .snippet("tan(${1:x})")
            .sort("13_tan")
            .build(),
    ]
}
