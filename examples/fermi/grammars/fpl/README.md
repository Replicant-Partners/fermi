# tree-sitter-fpl

Tree-sitter grammar for the Fermi Forecasting Programming Language (FPL).

## Installation

### NPM
```bash
npm install tree-sitter-fpl
```

### Cargo
```toml
[dependencies]
tree-sitter-fpl = "0.1"
```

## Usage

### Node.js
```javascript
const Parser = require('tree-sitter');
const FPL = require('tree-sitter-fpl');

const parser = new Parser();
parser.setLanguage(FPL);

const source = `
  forecast "Q4 Revenue" {
    driver revenue triangular(100, 200, 500)
    driver costs normal(150, 30)
    estimate revenue - costs
  }
`;

const tree = parser.parse(source);
console.log(tree.rootNode.toString());
```

### Rust
```rust
use tree_sitter::Parser;

fn main() {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_fpl::language()).unwrap();
    
    let source = r#"
        forecast "Q4 Revenue" {
            driver revenue triangular(100, 200, 500)
            estimate revenue
        }
    "#;
    
    let tree = parser.parse(source, None).unwrap();
    println!("{}", tree.root_node().to_sexp());
}
```

## Grammar

The FPL grammar supports:

- **Forecast statements** - Define forecasts with titles
- **Driver statements** - Define probability distributions
- **Estimate statements** - Express calculations
- **Distributions** - triangular, normal, lognormal, uniform, beta
- **Expressions** - Binary operators (+, -, *, /, ^), function calls
- **Comments** - Line (//) and block (/* */) comments

## Example

```fpl
forecast "AMD Q4 2024 Revenue" {
    // Market drivers
    driver gpu_market triangular(20000, 32000, 50000)
    driver market_share normal(0.15, 0.05)
    driver avg_price triangular(800, 1200, 2000)
    
    // Calculate revenue
    estimate gpu_market * market_share * avg_price
}
```

## Development

### Generate Parser
```bash
npm install
npm run build
```

### Test Grammar
```bash
npm test
```

### Build Rust Bindings
```bash
cargo build
cargo test
```

## Integration with Zed

See the `zed-fermi-lsp` extension for Zed editor integration.

## License

MIT
