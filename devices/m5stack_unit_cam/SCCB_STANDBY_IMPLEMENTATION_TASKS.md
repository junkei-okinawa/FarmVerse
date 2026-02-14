# SCCB Standby Implementation Tasks (M5Stack Unit Cam / OV2640)

## Goal
- Use SCCB register control to reduce camera power during Light Sleep without PWDN/power-cut hardware.
- Keep capture/stream reliability after wake-up.

## Assumptions
- `PWDN` hardware cut-off is not available in this device path.
- `esp-camera-rs` provides `sensor.set_reg(...)` and `sensor.set_xclk(...)`.
- Soft standby may reduce power but cannot guarantee full camera-off current.

## Phase 1: API and Register Layer
- Add camera standby APIs in `devices/m5stack_unit_cam/src/hardware/camera/controller.rs`:
  - `enter_standby_via_sccb() -> Result<(), CameraError>`
  - `exit_standby_via_sccb() -> Result<(), CameraError>`
  - Optional helpers:
    - `reduce_xclk_for_sleep() -> Result<(), CameraError>`
    - `restore_xclk_after_wakeup() -> Result<(), CameraError>`
- Add OV2640 register constants and bit masks near the API:
  - `COM7 (0x12)` and required bits for standby/sleep behavior
  - Any additional registers needed for stable wake-up
- Add comments for each register write (purpose and expected effect).

## Phase 2: Sleep/Wake Sequence Integration
- Integrate standby sequence before Light Sleep transition:
  - Enter SCCB standby
  - Optionally reduce/stop XCLK if stable
- Integrate wake sequence after Light Sleep resume:
  - Restore XCLK
  - Exit standby
  - Re-sync sensor state if needed
- Location candidates:
  - `devices/m5stack_unit_cam/src/main.rs`
  - `devices/m5stack_unit_cam/src/core/app_controller.rs`

## Phase 3: Recovery/Fallback Path
- If SCCB command fails:
  - Log error with context
  - Continue with safe fallback path (do not block sleep cycle)
- If wake sequence fails:
  - Re-init camera and continue flow
  - Reuse warm-up frame logic after re-init
- Ensure failure counters are visible in logs.

## Phase 4: Config and Feature Flag
- Add `cfg.toml` flags (default safe/off):
  - `camera_soft_standby_enabled = false`
  - Optional `camera_sleep_xclk_hz`
- Wire config into app flow:
  - Only run SCCB standby when enabled.

## Phase 5: Test Coverage (Host + Device)
- Host-testable logic (pure):
  - Standby sequence decision logic
  - Register-value mapping/bit composition
  - Error-to-fallback decision rules
- Extend `devices/m5stack_unit_cam/host_frame_tests/src/lib.rs` with unit tests for above.
- Device validation:
  - Standby enter/exit logs
  - Wake-up capture success over repeated cycles

## Phase 6: Measurement and Acceptance
- A/B test under same interval/load:
  - `soft_standby=off` vs `on`
- Record:
  - Voltage drop rate / average current trend
  - Wake capture success rate
  - First-frame validity after wake
  - SCCB error rate
- Acceptance criteria:
  - Stable operation for at least 10 consecutive sleep/wake cycles
  - Measurable power reduction vs baseline
  - Automatic recovery from standby/wake errors

## Risks
- SCCB standby may not reach low current comparable to hardware power cut.
- Some register combinations may cause unstable wake or bus lockups.
- XCLK handling may differ across board revisions and camera modules.

## Implementation Order (Recommended)
1. Phase 1 API + constants
2. Phase 2 integration (minimal path)
3. Phase 3 fallback
4. Phase 4 config toggle
5. Phase 5 tests
6. Phase 6 measurement
