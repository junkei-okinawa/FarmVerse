# INA226 Power Monitor テスト計画

## 1. 目的
- 計測・Web監視・異常値ガード・オフライン表示の主要要件を、実機試験と単体試験で検証する。

## 2. テスト方針
- 単体テスト: ホスト実行可能な純粋ロジックを Rust の `#[test]` で検証する。
- 実機テスト: XIAO ESP32S3 + INA226 + 計測対象を使い、I2C/計測/SoftAP/WebUI/CSV を確認する。

## 3. 単体テスト項目
1. `Sample::to_json`
- JSONに `sensor_online`, `quality`, `status_message` を含む。
- `target`/`status_message` のエスケープが正しい。

2. I2Cアドレス補助ロジック
- `format_addrs` が期待フォーマット (`0x40,0x41`) を返す。
- `resolve_ina226_address` が:
  - 優先アドレスを返す
  - 優先不在時に 0x40..0x4F をフォールバックする
  - 候補なしでエラーを返す

## 4. 実機テスト項目
1. 起動シーケンス
- ログに I2C検出、Manufacturer/Die ID、設定値が出る。
- Wi-Fi AP が起動し `GET /` と `GET /metrics` が応答する。

2. 正常計測
- `bus_voltage_v` が 5V付近で推移する。
- `current_ma` と `power_mw` が動作状態に応じて変動する。

3. Sensor Offline
- INA226のSDA/SCLまたは電源を外したとき、WebUIに `Sensor Offline` が表示される。
- オフライン中、IndexedDBの保存件数が増えない。

4. 異常値ガード
- `cfg.toml` の `bus_voltage_min_v/max_v` を厳しく設定して異常判定を強制する。
- WebUIに警告表示され、IndexedDB保存が停止する。

5. グラフ表示
- タブ切替で表/グラフが切り替わる。
- 30秒/1分/3分/5分/10分は固定幅で表示される。
- 全データは 30秒→1分→3分→5分→10分→10分刻み拡張で表示幅が変化する。

## 5. 実行コマンド
```bash
cd devices/ina226_power_monitor
cargo test --manifest-path crates/power_monitor_core/Cargo.toml --target aarch64-apple-darwin
```

```bash
cd devices/ina226_power_monitor
cargo test
```

```bash
cd devices/ina226_power_monitor
cargo espflash flash --monitor --port /dev/cu.usbmodem101
```
