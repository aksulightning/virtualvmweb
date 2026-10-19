#!/usr/bin/env python3
"""Check repository-required purpose comments and Rust function/module documentation."""
from pathlib import Path
import re
import sys

errors = []
for path in Path('src').rglob('*.rs'):
    lines = path.read_text().splitlines()
    if not lines or not lines[0].startswith('//!'):
        errors.append(f'{path}: missing file-purpose comment')
    for index, line in enumerate(lines):
        if not re.match(r'\s*(pub(?:\([^)]*\))?\s+)?(async\s+)?(fn|mod)\s+\w+', line):
            continue
        preceding = index - 1
        while preceding >= 0 and (lines[preceding].lstrip().startswith('#[') or not lines[preceding].strip()):
            preceding -= 1
        if preceding < 0 or not lines[preceding].lstrip().startswith('///'):
            errors.append(f'{path}:{index + 1}: missing documentation comment')
if errors:
    print('\n'.join(errors))
    sys.exit(1)
print('Purpose and function/module documentation checks passed.')

