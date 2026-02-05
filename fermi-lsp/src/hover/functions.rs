/// Get hover documentation for distribution and math functions
pub fn get_function_hover(word: &str) -> Option<String> {
    let text = match word {
        // Distribution functions
        "triangular" => "**triangular(p5, p50, p95)**\n\nThree-point distribution using 5th, 50th, and 95th percentiles.\n\n**Example:** `triangular(1000, 2000, 5000)`\n\n**Best for:** Expert estimates with min/likely/max values\n\n**Properties:** Asymmetric, bounded, intuitive for experts",

        "normal" => "**normal(mean, stddev)**\n\nNormal (Gaussian) distribution with mean and standard deviation.\n\n**Example:** `normal(100, 15)`\n\n**Best for:** Natural variations, measurement errors, averages\n\n**Properties:** Symmetric bell curve, unbounded, 68-95-99.7 rule",

        "lognormal" => "**lognormal(median, sigma)**\n\nLognormal distribution - for positive-only values with right skew.\n\n**Example:** `lognormal(50000, 0.5)`\n\n**Best for:** Prices, incomes, project durations, multiplicative processes\n\n**Properties:** Cannot be negative, right-skewed, median-based",

        "uniform" => "**uniform(low, high)**\n\nUniform distribution - all values equally likely.\n\n**Example:** `uniform(0, 100)`\n\n**Best for:** Complete uncertainty within range, random selection\n\n**Properties:** Flat probability, bounded, maximum entropy",

        "beta" => "**beta(alpha, beta)**\n\nBeta distribution - bounded between 0 and 1, flexible shape.\n\n**Example:** `beta(2, 5)` for right-skewed probability\n\n**Best for:** Probabilities, percentages, proportions\n\n**Properties:** Bounded [0,1], very flexible shape, conjugate prior",

        "exponential" => "**exponential(lambda)**\n\nExponential distribution for time between events.\n\n**Example:** `exponential(0.5)` for mean time of 2 units\n\n**Best for:** Wait times, time to failure, lifetimes\n\n**Properties:** Memoryless, right-skewed, λ = 1/mean",

        // Math functions
        "sqrt" => "**sqrt(x)**\n\nSquare root function.\n\n**Returns:** √x\n\n**Example:** `sqrt(16)` = 4",

        "log" => "**log(x)**\n\nNatural logarithm (base e).\n\n**Returns:** ln(x)\n\n**Example:** `log(2.71828)` ≈ 1",

        "log10" => "**log10(x)**\n\nBase-10 logarithm.\n\n**Returns:** log₁₀(x)\n\n**Example:** `log10(100)` = 2",

        "exp" => "**exp(x)**\n\nExponential function.\n\n**Returns:** e^x\n\n**Example:** `exp(1)` ≈ 2.71828",

        "pow" => "**pow(base, exponent)**\n\nPower function.\n\n**Returns:** base^exponent\n\n**Example:** `pow(2, 8)` = 256",

        "abs" => "**abs(x)**\n\nAbsolute value.\n\n**Returns:** |x|\n\n**Example:** `abs(-5)` = 5",

        "min" => "**min(a, b)**\n\nMinimum of two values.\n\n**Example:** `min(10, 20)` = 10",

        "max" => "**max(a, b)**\n\nMaximum of two values.\n\n**Example:** `max(10, 20)` = 20",

        "round" => "**round(x)**\n\nRound to nearest integer.\n\n**Example:** `round(3.7)` = 4",

        "floor" => "**floor(x)**\n\nRound down to integer.\n\n**Example:** `floor(3.7)` = 3",

        "ceil" => "**ceil(x)**\n\nRound up to integer.\n\n**Example:** `ceil(3.2)` = 4",

        "sin" => "**sin(x)**\n\nSine function (trigonometry).\n\n**Input:** Angle in radians\n\n**Returns:** Sine value (-1 to 1)\n\n**Example:** `sin(1.5708)` ≈ 1",

        "cos" => "**cos(x)**\n\nCosine function (trigonometry).\n\n**Input:** Angle in radians\n\n**Returns:** Cosine value (-1 to 1)\n\n**Example:** `cos(0)` = 1",

        "tan" => "**tan(x)**\n\nTangent function (trigonometry).\n\n**Input:** Angle in radians\n\n**Returns:** Tangent value\n\n**Example:** `tan(0.785398)` ≈ 1",

        _ => return None,
    };

    Some(text.to_string())
}
