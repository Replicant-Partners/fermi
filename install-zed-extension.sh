#!/bin/bash

# Fermi Zed Extension Installation Script
# This script builds and installs the Fermi extension for Zed editor

set -e

echo "🔧 Installing Fermi Extension for Zed..."
echo ""

# Check prerequisites
command -v cargo >/dev/null 2>&1 || { echo "❌ Error: cargo not found. Install Rust from https://rustup.rs"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "❌ Error: npm not found. Install Node.js from https://nodejs.org"; exit 1; }
command -v zed >/dev/null 2>&1 || { echo "⚠️  Warning: zed command not found. Make sure Zed is installed."; }

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ZED_EXTENSIONS_DIR="${HOME}/.config/zed/extensions"

echo "📦 Step 1/4: Building tree-sitter parser..."
cd "${PROJECT_ROOT}/tree-sitter-fpl"
if [ ! -d "node_modules" ]; then
    npm install
fi
npm run build
echo "✅ Tree-sitter parser built"
echo ""

echo "🔨 Step 2/4: Building LSP server..."
cd "${PROJECT_ROOT}/fermi-lsp"
cargo build --release
LSP_BINARY="${PROJECT_ROOT}/fermi-lsp/target/release/fermi-lsp"
echo "✅ LSP server built at: ${LSP_BINARY}"
echo ""

echo "📋 Step 3/4: Installing Zed extension..."
mkdir -p "${ZED_EXTENSIONS_DIR}"
rm -rf "${ZED_EXTENSIONS_DIR}/fermi"
ln -s "${PROJECT_ROOT}/extensions/fermi" "${ZED_EXTENSIONS_DIR}/fermi"
echo "✅ Extension linked to: ${ZED_EXTENSIONS_DIR}/fermi"
echo ""

echo "⚙️  Step 4/4: Configuring Zed..."
ZEDI_CONFIG="${HOME}/.config/zed/settings.json"

if [ ! -f "${ZED_CONFIG}" ]; then
    echo "Creating new Zed settings.json..."
    mkdir -p "$(dirname "${ZED_CONFIG}")"
    cat > "${ZED_CONFIG}" <<EOF
{
  "lsp": {
    "fermi-lsp": {
      "binary": {
        "path": "${LSP_BINARY}"
      },
      "settings": {
        "RUST_LOG": "info"
      }
    }
  },
  "languages": {
    "FPL": {
      "tab_size": 4,
      "hard_tabs": false,
      "format_on_save": false
    }
  }
}
EOF
    echo "✅ Created ${ZED_CONFIG}"
else
    echo "⚠️  ${ZED_CONFIG} already exists."
    echo ""
    echo "Please add the following to your Zed settings.json manually:"
    echo ""
    cat <<EOF
{
  "lsp": {
    "fermi-lsp": {
      "binary": {
        "path": "${LSP_BINARY}"
      },
      "settings": {
        "RUST_LOG": "info"
      }
    }
  },
  "languages": {
    "FPL": {
      "tab_size": 4,
      "hard_tabs": false,
      "format_on_save": false
    }
  }
}
EOF
fi

echo ""
echo "🎉 Installation complete!"
echo ""
echo "Next steps:"
echo "1. Restart Zed: killall zed && zed"
echo "2. Create a test file: touch test.fpl"
echo "3. Open in Zed and start editing!"
echo ""
echo "Test forecast:"
cat <<EOF

forecast "Test Forecast" {
    driver revenue triangular(100, 200, 500)
    driver costs normal(150, 30)
    estimate revenue - costs
}
EOF
echo ""
echo "Troubleshooting:"
echo "- View LSP logs: Zed → View → Debug → Language Server Logs"
echo "- Enable debug: Set RUST_LOG=debug in settings.json"
echo "- Check extension: ls -la ${ZED_EXTENSIONS_DIR}/fermi"
echo ""
echo "Documentation: ${PROJECT_ROOT}/extensions/fermi/README.md"
