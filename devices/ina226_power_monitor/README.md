# INA226 Power Monitor (XIAO ESP32S3)

XIAO ESP32S3 + INA226 で電圧/電流/電力を計測し、SoftAP経由のWeb UIで監視するプロジェクトです。

## Features
- INA226 計測（Bus Voltage / Current / Power）
- CSV形式シリアル出力
- SoftAP + Web UI (`/`)
- JSON API (`/metrics`)
- IndexedDB 保存 + CSVダウンロード
- 表/折れ線グラフタブ
- 異常値ガード（invalid）と Sensor Offline 表示

## Wiring (Summary)
- I2C: `SDA=GPIO5(D4)`, `SCL=GPIO6(D5)`
- INA226 address: `0x40` (default)
- `V-IN+ -> V-IN-` を測定対象5Vラインに直列挿入
- `VBS/VBUS` 端子があるモジュールは `V-IN-` へ接続
- 全GNDを共通化

詳細は `docs/REQUIREMENTS_JA.md` を参照。

## Configuration
`cfg.toml.template` を元に `cfg.toml` を編集。

主な項目:
- AP: `ap_ssid`, `ap_password`, `ap_channel`
- INA226: `ina226_addr`, `shunt_resistor_ohm`, `i2c_frequency_hz`
- Sampling: `sample_interval_ms`
- Guard: `invalid_guard_enabled`, `bus_voltage_min_v`, `bus_voltage_max_v`

## Build / Flash
```bash
cd devices/ina226_power_monitor
cargo check --target xtensa-esp32s3-espidf
cargo espflash flash --monitor --port /dev/cu.usbmodem101
```

## Tests
### Core logic tests (hardware independent)
```bash
cd devices/ina226_power_monitor
cargo test --manifest-path crates/power_monitor_core/Cargo.toml --target aarch64-apple-darwin
```

### Device-side build check
```bash
cd devices/ina226_power_monitor
cargo check --target xtensa-esp32s3-espidf
```
