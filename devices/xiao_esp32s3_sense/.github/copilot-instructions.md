# Copilot Instructions for FarmVerse XIAO ESP32S3 Sense

## Language Preference

- **Language**: All interactions, explanations, and code comments must be in **Japanese** (日本語).

## Build, Test, and Lint Commands

- **Host Unit Tests (Preferred)**: Run `./run_tests.sh`. This executes logic tests (voltage, TDS, protocols) on the host machine without hardware.
  - Individual test: `rustc --test src/utils/voltage_calc.rs --edition 2021 -o target/voltage_tests && ./target/voltage_tests` (See `run_tests.sh` for exact commands).
- **Build Firmware**: `cargo build` (Target: `xtensa-esp32s3-espidf`).
- **Flash & Monitor**: `cargo espflash flash --partition-table partitions.csv --monitor`.
- **Integration Tests (Hardware Required)**: `cargo test --lib integration_tests`.
- **Lint**: `cargo check`, `cargo clippy`.

## High-Level Architecture

- **Role**: Streaming camera device using XIAO ESP32S3 Sense. Captures images (OV2640) and sensor data.
- **Communication**: Sends data via **ESP-NOW** to a receiver (USB CDC).
- **Core Loop (Hybrid Sleep)**:
  1.  **Wake/Init**: Unhold pins, load `cfg.toml`, init peripherals.
  2.  **Lazy WiFi**: WiFi/ESP-NOW initialized only if needed (persists across Light Sleep).
  3.  **Measure**: Voltage (ADC), Temperature (DS18B20), TDS (Simulated/Real).
  4.  **Capture**: Image captured if voltage sufficient (check `bypass_voltage_threshold`).
  5.  **Transmit**: Data chunked and sent via ESP-NOW with adaptive retries.
  6.  **Sleep**: Device enters Deep Sleep (default) or Light Sleep based on server command/config.
- **Protocol Details**:
  - **Frame Structure**: `[START:4][MAC:6][TYPE:1][SEQ:4][LEN:4][DATA:N][CSUM:4][END:4]` (Total overhead: 27 bytes).
  - **Endianness**: **Mixed**. Markers (`0xFACEAABB`, `0xCDEF5678`) are Big-Endian. Sequence/Length/Checksum are Little-Endian.
  - **Adaptive Chunking**: Max 250 bytes. Retries with smaller payloads (150, 100, 50...) on failure.
  - **Protocol Definitions**: `src/communication/esp_now/sender.rs` is the source of truth for the OTA frame format. `src/utils/streaming_protocol.rs` defines the logical message structure and is used for platform-independent testing.

## Key Conventions

- **Testing Strategy (3-Layer)**:
  1.  **Host Unit Tests (Primary)**: Logic (calculations, protocols) **MUST** be in `src/utils/` and tested via `./run_tests.sh`. These are pure functions (no `esp-idf-svc` deps).
  2.  **Probe-rs**: For peripheral testing.
  3.  **Manual**: Final verification using `cargo espflash --monitor`.
- **Power Management (Critical)**:
  - **Explicit Cleanup**: Drivers (e.g., Camera) must be explicitly dropped/de-initialized before sleep to prevent leaks.
  - **Deep Sleep**: Use `esp_sleep_enable_timer_wakeup` followed by `esp_deep_sleep_start`.
  - **Pin State**: Be cautious with `gpio_hold` and pin resets. Currently, explicit pin resets might be disabled to support Light Sleep (see `DataService`).
- **Error Handling**:
  - Top-level app uses `anyhow::Result`.
  - Modules use `thiserror` (e.g., `EspNowError`, `DeserializeError`).
  - **Recovery**: ESP-NOW senders must handle `ESP_ERR_ESPNOW_NO_MEM` with backoff delays (`FreeRtos::delay_ms`).
- **Memory Management**:
  - **PSRAM**: Essential for camera buffers.
  - **Chunks**: Large buffers are split into chunks to fit ESP-NOW limits (max 250 bytes).
- **Configuration**: `cfg.toml` controls runtime behavior. Loaded into `Arc<AppConfig>`.
  - **Pin Assignments**: Hardware pins are subject to change. Always verify against `src/hardware/` or `cfg.toml`. Do not assume GPIO numbers from old documentation.
- **Concurrency**:
  - Uses `std::sync::{Arc, Mutex}` for shared resources like `EspNow`.
  - Main loop is sequential; strict resource management (e.g., `wifi_resources` Option) to handle sleep states.
- **Logging**: Use `log::{info, warn, error}`. Avoid `println!`.
