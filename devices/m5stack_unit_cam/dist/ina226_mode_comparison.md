# INA226 Mode Comparison

- threshold_ma: 60.0

| sensor | mode | samples | duration_s | mean_ma | median_ma | min_ma | max_ma | sleep_like_ratio | sleep_like_mean_ma | active_mean_ma | energy_mwh |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ov2640 | full | 169 | 168.0 | 74.08 | 27.40 | 0.00 | 151.80 | 0.521 | 22.95 | 129.62 | 16.80 |
| ov2640 | minimal | 478 | 579.6 | 52.22 | 23.50 | 0.00 | 202.10 | 0.730 | 22.67 | 132.17 | 36.39 |
| ov2640 | off | 199 | 198.0 | 87.68 | 131.30 | 0.00 | 198.60 | 0.437 | 24.75 | 136.57 | 23.38 |
| ov3660 | full | 1020 | 1023.0 | 40.53 | 0.10 | 0.00 | 175.70 | 0.711 | 1.70 | 135.96 | 54.09 |
| ov3660 | minimal | 369 | 2675.8 | 35.08 | 0.10 | -0.10 | 155.10 | 0.743 | 1.59 | 131.69 | 17.35 |
| ov3660 | off | 492 | 491.0 | 61.34 | 33.40 | 33.10 | 162.60 | 0.728 | 35.01 | 131.68 | 40.11 |

## Sensor Delta
- off: mean_ma delta(ov3660-ov2640)=-26.35, sleep_like_mean_ma delta=10.26, active_mean_ma delta=-4.89
- minimal: mean_ma delta(ov3660-ov2640)=-17.14, sleep_like_mean_ma delta=-21.08, active_mean_ma delta=-0.48
- full: mean_ma delta(ov3660-ov2640)=-33.55, sleep_like_mean_ma delta=-21.26, active_mean_ma delta=6.34

## Inferred Segments
- ov2640/full: segments=7 sleep_like_count=4 active_count=3 sleep_like_avg_duration_s=21.0 active_avg_duration_s=26.0
- ov3660/full: segments=47 sleep_like_count=24 active_count=23 sleep_like_avg_duration_s=29.2 active_avg_duration_s=11.8
- ov2640/minimal: segments=41 sleep_like_count=21 active_count=20 sleep_like_avg_duration_s=21.1 active_avg_duration_s=5.5
- ov3660/minimal: segments=19 sleep_like_count=10 active_count=9 sleep_like_avg_duration_s=256.9 active_avg_duration_s=9.8
- ov2640/off: segments=9 sleep_like_count=5 active_count=4 sleep_like_avg_duration_s=16.4 active_avg_duration_s=27.0
- ov3660/off: segments=35 sleep_like_count=18 active_count=17 sleep_like_avg_duration_s=18.9 active_avg_duration_s=6.9

## Notes
- `sleep_like` is inferred from current only (no UART log correlation in this report).
- For strict `verify-before/after` timing correlation, add a synchronized timestamp marker to UART logs.
