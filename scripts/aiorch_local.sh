#!/usr/bin/env bash
#
# Local wrapper for ai-orchestrator
# The orchestrator itself is gitignored; this script provides a tracked entry point.
#
# Usage:
#   ./scripts/aiorch_local.sh canary run --profile release --duration-ms 5000 --dump-frame
#   ./scripts/aiorch_local.sh verify <work_order_id>
#   ./scripts/aiorch_local.sh status
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
AIORCH_PATH="${AIORCH_PATH:-$REPO_ROOT/tools/ai-orchestrator/aiorch.py}"

if [[ ! -f "$AIORCH_PATH" ]]; then
    echo "ERROR: ai-orchestrator not found at $AIORCH_PATH"
    echo ""
    echo "The ai-orchestrator is not tracked in this repository."
    echo "To use it, copy or symlink your local orchestrator to:"
    echo "  $REPO_ROOT/tools/ai-orchestrator/"
    echo ""
    echo "Or set AIORCH_PATH to point to your aiorch.py:"
    echo "  export AIORCH_PATH=/path/to/your/aiorch.py"
    echo "  $0 $*"
    exit 1
fi

cd "$REPO_ROOT"
exec python3 "$AIORCH_PATH" "$@"

