#!/usr/bin/env python3
"""
Analyze INA226 CSV for A/B standby comparison.
"""

from __future__ import annotations

import argparse
import csv
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, median


@dataclass
class Record:
    timestamp_ms: int
    current_ma: float
    power_mw: float
    standby_mode: str


def load_records(path: Path) -> list[Record]:
    text = path.read_text(encoding="utf-8").strip()
    if "\\n" in text and "\n" not in text:
        text = text.replace("\\n", "\n")
    rows = list(csv.DictReader(line for line in text.splitlines() if line.strip()))
    out: list[Record] = []
    for row in rows:
        try:
            out.append(
                Record(
                    timestamp_ms=int(row["timestamp_ms"]),
                    current_ma=float(row["current_ma"]),
                    power_mw=float(row["power_mw"]),
                    standby_mode=row.get("standby_mode", "unknown"),
                )
            )
        except (KeyError, ValueError):
            continue
    return out


def energy_mwh(records: list[Record]) -> float:
    if len(records) < 2:
        return 0.0
    total = 0.0
    for a, b in zip(records, records[1:]):
        dt_h = max(0, b.timestamp_ms - a.timestamp_ms) / 1000.0 / 3600.0
        total += a.power_mw * dt_h
    return total


def summarize(records: list[Record], sleep_threshold_ma: float) -> str:
    currents = [r.current_ma for r in records]
    sleeps = [r.current_ma for r in records if r.current_ma <= sleep_threshold_ma]
    actives = [r.current_ma for r in records if r.current_ma > sleep_threshold_ma]
    duration_s = 0.0
    if len(records) >= 2:
        duration_s = (records[-1].timestamp_ms - records[0].timestamp_ms) / 1000.0
    mode = records[0].standby_mode if records else "unknown"
    lines = [
        f"standby_mode={mode}",
        f"samples={len(records)} duration_s={duration_s:.1f}",
        f"current_mean_ma={mean(currents):.3f}",
        f"current_median_ma={median(currents):.3f}",
        f"current_min_ma={min(currents):.3f} current_max_ma={max(currents):.3f}",
        f"sleep_samples={len(sleeps)} sleep_mean_ma={(mean(sleeps) if sleeps else 0.0):.3f} threshold_ma={sleep_threshold_ma}",
        f"active_samples={len(actives)} active_mean_ma={(mean(actives) if actives else 0.0):.3f}",
        f"energy_mwh={energy_mwh(records):.3f}",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="Input CSV path")
    parser.add_argument(
        "--sleep-threshold-ma",
        type=float,
        default=35.0,
        help="Threshold for sleep-vs-active classification",
    )
    args = parser.parse_args()

    records = load_records(Path(args.input))
    if not records:
        raise SystemExit("no valid rows found")
    print(summarize(records, args.sleep_threshold_ma))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

