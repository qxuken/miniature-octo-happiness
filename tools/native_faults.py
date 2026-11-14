#!/usr/bin/env python3
"""Compatibility CLI for cargo xtask faults native."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
import supervision

def main():
    supervision.command(['faults','native',*sys.argv[1:]])

if __name__ == '__main__':main()
