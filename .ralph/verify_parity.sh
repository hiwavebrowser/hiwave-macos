#!/bin/bash
# Custom verification script for parity tests

set -e

echo "🧪 Running parity verification..."

# Build RustKit
echo "🔨 Building RustKit..."
cargo build --release

# Run parity tests
echo "🎯 Running parity tests..."
python3 scripts/parity_test.py --scope all > /tmp/parity_results.txt 2>&1

# Parse results
PASSED=$(grep "Passed:" /tmp/parity_results.txt | awk '{print $2}' | cut -d'/' -f1)
TOTAL=$(grep "Passed:" /tmp/parity_results.txt | awk '{print $2}' | cut -d'/' -f2)
AVG_DIFF=$(grep "Average Diff:" /tmp/parity_results.txt | awk '{print $3}' | sed 's/%//')

echo ""
echo "📊 Results:"
echo "   Passed: $PASSED/$TOTAL"
echo "   Average Diff: $AVG_DIFF%"

# Success criteria: 90% pass rate
REQUIRED_PASSED=21
if [ "$PASSED" -ge "$REQUIRED_PASSED" ]; then
    echo "✅ SUCCESS: Parity goals achieved!"
    exit 0
else
    echo "⏳ In progress: Need $((REQUIRED_PASSED - PASSED)) more passing tests"
    exit 1
fi
