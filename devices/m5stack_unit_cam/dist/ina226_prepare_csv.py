#!/usr/bin/env python3
"""
Normalize INA226 CSV and inject standby_mode column for A/B tests.

Input may be either:
- normal multi-line CSV
- single-line CSV containing literal "\\n"
"""

from __future__ import annotations

import argparse
import csv
from pathlib import Path


VALID_MODES = {"off", "minimal", "full"}


def load_rows(path: Path) -> list[list[str]]:
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        return []
    if "\\n" in text and "\n" not in text:
        text = text.replace("\\n", "\n")
    lines = [line for line in text.splitlines() if line.strip()]
    return [row for row in csv.reader(lines)]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="Input CSV path")
    parser.add_argument("--output", required=True, help="Output CSV path")
    parser.add_argument("--mode", required=True, help="off|minimal|full")
    args = parser.parse_args()

    mode = args.mode.strip().lower()
    if mode not in VALID_MODES:
        raise SystemExit(f"invalid --mode: {mode} (use off|minimal|full)")

    rows = load_rows(Path(args.input))
    if not rows:
        raise SystemExit("input is empty")

    header = rows[0]
    data_rows = rows[1:]
    if "standby_mode" not in header:
        header = header + ["standby_mode"]
        data_rows = [row + [mode] for row in data_rows]
    else:
        idx = header.index("standby_mode")
        for row in data_rows:
            if len(row) <= idx:
                row.extend([""] * (idx - len(row) + 1))
            row[idx] = mode

    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(header)
        writer.writerows(data_rows)

    print(f"Wrote: {out} ({len(data_rows)} rows, mode={mode})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

