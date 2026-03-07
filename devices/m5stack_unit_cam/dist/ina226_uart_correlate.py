#!/usr/bin/env python3
"""
Correlate UART event timestamps with INA226 CSV current windows.
"""

from __future__ import annotations

import argparse
import csv
import re
from dataclasses import dataclass
from pathlib import Path
from statistics import mean


UART_RE = re.compile(r"\b[IWE] \((\d+)\)")


@dataclass
class Sample:
    ts_ms: int
    current_ma: float


def load_csv(path: Path) -> list[Sample]:
    rows = list(csv.DictReader(path.read_text(encoding="utf-8").splitlines()))
    out: list[Sample] = []
    for r in rows:
        try:
            out.append(Sample(ts_ms=int(r["timestamp_ms"]), current_ma=float(r["current_ma"])))
        except (KeyError, ValueError):
            continue
    out.sort(key=lambda s: s.ts_ms)
    return out


def find_events(log_path: Path, keywords: list[str]) -> list[tuple[int, str]]:
    events: list[tuple[int, str]] = []
    for line in log_path.read_text(encoding="utf-8", errors="ignore").splitlines():
        m = UART_RE.search(line)
        if not m:
            continue
        ts = int(m.group(1))
        if any(k in line for k in keywords):
            events.append((ts, line.strip()))
    return events


def window_mean(samples: list[Sample], start_ms: int, end_ms: int) -> float:
    vals = [s.current_ma for s in samples if start_ms <= s.ts_ms <= end_ms]
    if not vals:
        return float("nan")
    return mean(vals)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--csv", required=True)
    p.add_argument("--log", required=True)
    p.add_argument(
        "--keyword",
        action="append",
        required=True,
        help="event keyword (repeatable), e.g. 'standby verify ok'",
    )
    p.add_argument("--offset-ms", type=int, default=0, help="csv_ts = uart_ts + offset")
    p.add_argument("--pre-ms", type=int, default=3000)
    p.add_argument("--post-ms", type=int, default=3000)
    args = p.parse_args()

    samples = load_csv(Path(args.csv))
    events = find_events(Path(args.log), args.keyword)
    if not samples:
        raise SystemExit("no csv samples")
    if not events:
        raise SystemExit("no matching uart events")

    print("event_ts_ms,keyword_line,pre_mean_ma,post_mean_ma")
    for ts, line in events:
        t = ts + args.offset_ms
        pre = window_mean(samples, t - args.pre_ms, t)
        post = window_mean(samples, t, t + args.post_ms)
        print(f"{ts},\"{line}\",{pre:.3f},{post:.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
