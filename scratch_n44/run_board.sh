#!/bin/bash
# run_board.sh LABEL — run the full campaign board from the repo root, log + copy results under scratch_n44.
cd /Users/petecopeland/Repos/hiwave/hiwave-macos || exit 1
label="$1"
python3 scripts/parity_test.py > "scratch_n44/board_${label}.log" 2>&1
cp parity-baseline/parity_test_results.json "scratch_n44/board_${label}.json"
echo "done $(date)" >> "scratch_n44/board_${label}.log"
