#!/usr/bin/env python3
"""Compatibility entry point; workspace check orchestration lives in xtask."""
import os
from pathlib import Path
import subprocess
import sys

if __name__ == '__main__':
    env = os.environ.copy(); env['WEEKEND_PYTHON'] = sys.executable
    raise SystemExit(subprocess.call(['cargo', 'xtask', 'check', *sys.argv[1:]],
        cwd=Path(__file__).resolve().parent.parent, env=env))
