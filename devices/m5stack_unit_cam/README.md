# m5stack_unit_cam

M5Stack Unit Cam (ESP32 + OV2640) 向けの送信ファームウェアです。  
画像を ESP-NOW で分割送信し、最後に HASH/EOF フレームを送信します。

## 機能

- OV2640 で画像撮影
- ESP-NOW で画像チャンク送信
- HASH フレーム送信（電圧情報を含む）
- EOF フレーム送信
- サーバーからのスリープコマンド受信後に Deep Sleep
- 設定で OV2640 の SCCB ソフトスタンバイ試行（`camera_soft_standby_enabled`）

注記:
- 本プロジェクトの M5Stack Unit Cam 実装では、運用スリープは Deep Sleep のみを採用しています。
- Light Sleep は実機検証で 2 サイクル目以降に SCCB 復帰失敗、`NO-SOI`、`Failed to get the frame on time` が再現し、画像品質と安定性を満たせませんでした。
- そのため、安定運用を優先して Light Sleep 経路は実装から除外しています。

## ディレクトリ

- `src/core`: アプリ制御、設定、送信ロジック
- `src/hardware`: カメラ、LED、電圧センサー
- `src/communication`: ESP-NOW 送受信
- `host_frame_tests`: ホストで実行できるユニットテスト
- `qemu_unittest.sh`: QEMU smoke 実行スクリプト

## 前提環境

- ESP-IDF v5.1.6
- Rust `esp` toolchain（`rust-toolchain.toml`）
- `espflash`
- 実機書き込み時: M5Stack Unit Cam 接続

## セットアップ

```bash
cd devices/m5stack_unit_cam
cp cfg.toml.template cfg.toml
. /Users/junkei/esp/v5.1.6/esp-idf/export.sh
```

`build.rs` は `cfg.toml` が無いとビルドを停止します。

## ビルドと書き込み

```bash
cd devices/m5stack_unit_cam
. /Users/junkei/esp/v5.1.6/esp-idf/export.sh
cargo build
cargo espflash flash --release --monitor --partition-table partitions.csv
```

`.cargo/config.toml` でデフォルトターゲットは `xtensa-esp32-espidf` です。

重要:
- 再書き込み後は Unit Cam の電源を一度抜き差ししてから実行してください。
- 電源を切らずに再書き込みすると、カメラがスタンバイ相当の状態を引きずり、`Camera probe failed (ESP_ERR_NOT_FOUND)` が発生する場合があります。

## 設定（cfg.toml）

主な項目:

- `receiver_mac`: 送信先 MAC
- `sleep_duration_seconds`: 通常スリープ秒
- `sleep_duration_seconds_for_long`: 低電圧時スリープ秒
- `sleep_command_timeout_seconds`: スリープコマンド待機秒
- `frame_size`: カメラ解像度
- `camera_warmup_frames`: 捨てフレーム数
- `camera_soft_standby_enabled`: SCCB ソフトスタンバイ有効化
- `adc_voltage_min_mv` / `adc_voltage_max_mv`: 電圧換算キャリブレーション
- `esp_now_chunk_size` / `esp_now_chunk_delay_ms`: 送信チャンク設定
- `timezone`: タイムゾーン

詳細とコメント付きテンプレートは `cfg.toml.template` を参照してください。

## テスト

### 1. ホストユニットテスト（ESP-IDF 不要）

```bash
cd devices/m5stack_unit_cam/host_frame_tests
cargo test --target aarch64-apple-darwin
```

または、現在のホストターゲットを自動指定:

```bash
cd devices/m5stack_unit_cam/host_frame_tests
cargo test --target "$(rustc -vV | awk '/host:/ {print $2}')"
```

`devices/m5stack_unit_cam/.cargo/config.toml` でデフォルトターゲットが
`xtensa-esp32-espidf` に固定されているため、`--target` 指定なしの `cargo test`
は `ldproxy` エラーになります。

対象:

- フレーム構築/デコードの純粋ロジック
- チェックサム
- HASH ペイロード生成
- OV2640 レジスタシーケンス生成

### 2. QEMU smoke

`qemu_unittest.sh` は以下を自動実行します。

- QEMU (`qemu-system-xtensa`) の存在確認
- `-machine esp32` サポート確認
- `cargo +esp build --features qemu-smoke`
- `espflash save-image`
- QEMU起動と `QEMU_SMOKE_PASS` マーカー確認

```bash
cd devices/m5stack_unit_cam
. /Users/junkei/esp/v5.1.6/esp-idf/export.sh
./qemu_unittest.sh
```

ビルド済みバイナリで実行だけ行う場合:

```bash
QEMU_POC_SKIP_BUILD=1 ./qemu_unittest.sh
```

利用可能な環境変数:

- `ESP_QEMU_BIN`
- `QEMU_FEATURES`（デフォルト: `qemu-smoke`）
- `QEMU_SMOKE_MARKER`（デフォルト: `QEMU_SMOKE_PASS`）
- `QEMU_POC_SKIP_BUILD`

## CI

- ワークフロー: `.github/workflows/m5stack_qemu_smoke.yml`
- `espressif/idf:latest` コンテナ上で QEMU 可用性を確認
  - `qemu-system-xtensa` の存在
  - `-machine esp32` サポート

## 注意事項

- GPIO0 はブートストラップに関係するため、起動時の配線に注意してください。
- OV2640 のソフトスタンバイは SCCB レジスタ操作です。PWDN 電源遮断とは別方式です。
- 消費電力や復帰挙動はボード配線と周辺回路に依存します。
- 再書き込み直後にカメラ初期化失敗が出る場合は、まず電源抜き差し後に再試行してください。

## 関連

- 送信先プロトコル実装: `server/usb_cdc_receiver`
- データ受信/保存: `server/sensor_data_reciver`
