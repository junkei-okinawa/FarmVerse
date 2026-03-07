#!/usr/bin/env python3
"""
Compare INA226 captures across standby modes and sensors, and infer active/sleep phases.
"""

from __future__ import annotations

import argparse
import csv
import re
from dataclasses import dataclass
from pathlib import Path
from statistics import mean, median


@dataclass
class Sample:
    ts_ms: int
    current_ma: float
    power_mw: float


@dataclass
class Segment:
    state: str
    start_ms: int
    end_ms: int
    duration_s: float
    avg_current_ma: float
    max_current_ma: float
    n: int


FILE_RE = re.compile(r"ina226_samples_(off|minimal|full)_(ov2640|ov3660)\.csv$")


def load_samples(path: Path) -> list[Sample]:
    rows = list(csv.DictReader(path.read_text(encoding="utf-8").splitlines()))
    out: list[Sample] = []
    for row in rows:
        try:
            out.append(
                Sample(
                    ts_ms=int(row["timestamp_ms"]),
                    current_ma=float(row["current_ma"]),
                    power_mw=float(row["power_mw"]),
                )
            )
        except (KeyError, ValueError):
            continue
    out.sort(key=lambda s: s.ts_ms)
    return out


def energy_mwh(samples: list[Sample]) -> float:
    if len(samples) < 2:
        return 0.0
    total = 0.0
    for a, b in zip(samples, samples[1:]):
        dt_h = max(0, b.ts_ms - a.ts_ms) / 3_600_000.0
        total += a.power_mw * dt_h
    return total


def segment(samples: list[Sample], active_threshold_ma: float) -> list[Segment]:
    if not samples:
        return []
    segments: list[Segment] = []
    current_state = "active" if samples[0].current_ma >= active_threshold_ma else "sleep_like"
    start = 0
    for i in range(1, len(samples)):
        state = "active" if samples[i].current_ma >= active_threshold_ma else "sleep_like"
        if state != current_state:
            segments.append(_make_segment(samples, start, i - 1, current_state))
            start = i
            current_state = state
    segments.append(_make_segment(samples, start, len(samples) - 1, current_state))
    return segments


def _make_segment(samples: list[Sample], start: int, end: int, state: str) -> Segment:
    block = samples[start : end + 1]
    start_ms = block[0].ts_ms
    end_ms = block[-1].ts_ms
    duration_s = max(0.0, (end_ms - start_ms) / 1000.0)
    currents = [s.current_ma for s in block]
    return Segment(
        state=state,
        start_ms=start_ms,
        end_ms=end_ms,
        duration_s=duration_s,
        avg_current_ma=mean(currents),
        max_current_ma=max(currents),
        n=len(block),
    )


def summarize(samples: list[Sample], sensor: str, mode: str, threshold: float) -> dict[str, float | str]:
    currents = [s.current_ma for s in samples]
    duration_s = (samples[-1].ts_ms - samples[0].ts_ms) / 1000.0 if len(samples) >= 2 else 0.0
    sleep_like = [s.current_ma for s in samples if s.current_ma < threshold]
    active = [s.current_ma for s in samples if s.current_ma >= threshold]
    return {
        "sensor": sensor,
        "mode": mode,
        "samples": len(samples),
        "duration_s": duration_s,
        "mean_ma": mean(currents),
        "median_ma": median(currents),
        "min_ma": min(currents),
        "max_ma": max(currents),
        "sleep_like_ratio": (len(sleep_like) / len(samples)) if samples else 0.0,
        "sleep_like_mean_ma": mean(sleep_like) if sleep_like else 0.0,
        "active_mean_ma": mean(active) if active else 0.0,
        "energy_mwh": energy_mwh(samples),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist-dir", default=".", help="Directory containing ina226_samples_*.csv")
    parser.add_argument("--threshold-ma", type=float, default=60.0, help="active/sleep-like threshold")
    parser.add_argument(
        "--output",
        default="ina226_mode_comparison.md",
        help="Output report path (markdown)",
    )
    args = parser.parse_args()

    dist_dir = Path(args.dist_dir)
    files: list[tuple[str, str, Path]] = []
    for path in sorted(dist_dir.glob("ina226_samples_*_ov*.csv")):
        m = FILE_RE.fullmatch(path.name)
        if not m:
            continue
        mode, sensor = m.groups()
        files.append((sensor, mode, path))
    if not files:
        raise SystemExit("no sensor/mode csv files found")

    rows: list[dict[str, float | str]] = []
    segment_lines: list[str] = []
    for sensor, mode, path in files:
        samples = load_samples(path)
        if not samples:
            raise SystemExit(f"no valid rows in {path}")
        rows.append(summarize(samples, sensor, mode, args.threshold_ma))

        segs = segment(samples, args.threshold_ma)
        sleep_durations = [s.duration_s for s in segs if s.state == "sleep_like"]
        active_durations = [s.duration_s for s in segs if s.state == "active"]
        segment_lines.append(
            f"- {sensor}/{mode}: segments={len(segs)} sleep_like_count={len(sleep_durations)} "
            f"active_count={len(active_durations)} "
            f"sleep_like_avg_duration_s={(mean(sleep_durations) if sleep_durations else 0.0):.1f} "
            f"active_avg_duration_s={(mean(active_durations) if active_durations else 0.0):.1f}"
        )

    rows.sort(key=lambda r: (str(r["sensor"]), str(r["mode"])))
    md = []
    md.append("# INA226 Mode Comparison")
    md.append("")
    md.append(f"- threshold_ma: {args.threshold_ma}")
    md.append("")
    md.append("| sensor | mode | samples | duration_s | mean_ma | median_ma | min_ma | max_ma | sleep_like_ratio | sleep_like_mean_ma | active_mean_ma | energy_mwh |")
    md.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        md.append(
            "| {sensor} | {mode} | {samples} | {duration_s:.1f} | {mean_ma:.2f} | {median_ma:.2f} | {min_ma:.2f} | {max_ma:.2f} | {sleep_like_ratio:.3f} | {sleep_like_mean_ma:.2f} | {active_mean_ma:.2f} | {energy_mwh:.2f} |".format(
                **r
            )
        )
    md.append("")
    md.append("## Sensor Delta")
    by_mode: dict[str, dict[str, dict[str, float | str]]] = {}
    for row in rows:
        by_mode.setdefault(str(row["mode"]), {})[str(row["sensor"])] = row
    for mode in ("off", "minimal", "full"):
        sensors = by_mode.get(mode, {})
        ov2640 = sensors.get("ov2640")
        ov3660 = sensors.get("ov3660")
        if not ov2640 or not ov3660:
            continue
        md.append(
            "- {mode}: mean_ma delta(ov3660-ov2640)={delta_mean:.2f}, sleep_like_mean_ma delta={delta_sleep:.2f}, active_mean_ma delta={delta_active:.2f}".format(
                mode=mode,
                delta_mean=float(ov3660["mean_ma"]) - float(ov2640["mean_ma"]),
                delta_sleep=float(ov3660["sleep_like_mean_ma"]) - float(ov2640["sleep_like_mean_ma"]),
                delta_active=float(ov3660["active_mean_ma"]) - float(ov2640["active_mean_ma"]),
            )
        )
    md.append("")
    md.append("## Inferred Segments")
    md.extend(segment_lines)
    md.append("")
    md.append("## Notes")
    md.append("- `sleep_like` is inferred from current only (no UART log correlation in this report).")
    md.append("- For strict `verify-before/after` timing correlation, add a synchronized timestamp marker to UART logs.")

    out = Path(args.output)
    out.write_text("\n".join(md) + "\n", encoding="utf-8")
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
