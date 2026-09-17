#!/usr/bin/env python3
"""bank.py TAG — copy parity-baseline results json + captures to scratch_n47/board_TAG.json / captures_TAG."""
import shutil
import sys

tag = sys.argv[1]
shutil.copy('parity-baseline/parity_test_results.json', f'scratch_n47/board_{tag}.json')
shutil.copytree('parity-baseline/captures', f'scratch_n47/captures_{tag}', dirs_exist_ok=True)
print('banked', tag)
