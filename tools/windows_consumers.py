#!/usr/bin/env python3
"""Transitional compatibility transport; native compiler policy lives in xtask."""
import json
from pathlib import Path
import subprocess
import sys
ROOT=Path(__file__).resolve().parent.parent

def compiler_environment(directory):
    # The environment stays in a private pipe. Only the caller's explicit
    # allowlist may enter evidence; never print this response or a failed stdout.
    result=subprocess.run(['cargo','xtask','compiler-environment','--directory',str(directory)],cwd=ROOT,capture_output=True,check=True)
    return json.loads(result.stdout)

if __name__=='__main__':raise SystemExit(subprocess.call(['cargo','xtask','windows-consumers',*sys.argv[1:]],cwd=ROOT))
