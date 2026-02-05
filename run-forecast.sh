#!/bin/bash
# Simple script to run Fermi forecasts
# Usage: ./run-forecast.sh <file.fpl>

if [ -z "$1" ]; then
    echo "Usage: ./run-forecast.sh <file.fpl>"
    echo "Example: ./run-forecast.sh test_forecast.fpl"
    exit 1
fi

if [ ! -f "$1" ]; then
    echo "Error: File '$1' not found"
    exit 1
fi

echo "🔮 Running Fermi forecast: $1"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Build if needed
if [ ! -f "target/release/fermi" ]; then
    echo "Building Fermi..."
    cargo build --release
fi

# Run the forecast
./target/release/fermi "$1"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ Forecast complete!"
