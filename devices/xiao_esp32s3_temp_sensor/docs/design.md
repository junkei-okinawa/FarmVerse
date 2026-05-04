# XIAO ESP32-S3 DS18B20 温度センサー 設計書

## 概要

XIAO ESP32-S3 で DS18B20 温度センサーを使い、定期的に温度を計測するモジュール。  
計測間隔・待機方式・送信先は `cfg.toml` で外部設定できる。

| Phase | 機能 |
|-------|------|
| Phase 1 | 計測温度を USB Serial/JTAG 経由でホスト PC のログに出力 |
| Phase 2 | WiFi (ESP-NOW) 経由で `sensor_data_reciver` サーバーに送信 |

---

## ハードウェア構成

### 対象ボード

**Seeed Studio XIAO ESP32-S3**  
- CPU: ESP32-S3 (Xtensa LX7 デュアルコア)
- Flash: 8MB
- USB: USB-C (内蔵 USB Serial/JTAG — UART チップ非搭載)

### DS18B20 配線

```
XIAO ESP32-S3         DS18B20
┌─────────────┐       ┌──────────┐
│ 3.3V ───────┼───┬───┤ VCC      │
│             │   │   │          │
│ D1 (GPIO2) ─┼───┘   │          │  ← 電源制御ピン (計測時のみ HIGH)
│             │       │          │
│ D3 (GPIO4) ─┼───┬───┤ DATA     │  ← 1-Wire データ (RMT channel0)
│             │   │   │          │
│ GND ────────┼───┼───┤ GND      │
└─────────────┘   │   └──────────┘
                  ├── 1kΩ ── 3.3V   ← プルアップ抵抗 (必須)
```

> **注意**: DATA ライン (GPIO4) と 3.3V の間に **1kΩ のプルアップ抵抗**が必要。

### GPIO 割り当て

| GPIO | XIAO ラベル | 用途 |
|------|------------|------|
| GPIO2 | D1 | DS18B20 電源制御 (出力) |
| GPIO4 | D3 | DS18B20 1-Wire データ (RMT) |

カメラピン (GPIO10-18, 38-40, 47-48) とは競合しない。

---

## ソフトウェアアーキテクチャ

### ディレクトリ構成

```
devices/xiao_esp32s3_temp_sensor/
├── src/main.rs          # メインロジック
├── docs/design.md       # 本設計書
├── Cargo.toml           # 依存関係 / wifi feature 定義
├── .cargo/config.toml   # ESP32-S3 ビルドターゲット設定
├── build.rs             # ESP-IDF ビルドスクリプト
├── rust-toolchain.toml  # Espressif カスタム Rust ツールチェーン
├── sdkconfig.defaults   # ESP-IDF SDK 設定
├── cfg.toml.template    # 設定テンプレート (git 管理)
└── .gitignore           # cfg.toml を除外
```

### 実行フロー

```
main()
  │
  ├─ esp_idf_svc::sys::link_patches()
  ├─ EspLogger::initialize_default()
  │
  ├─ [wifi feature] init_esp_now()
  │     ├─ WiFi STA モード起動 (AP 接続なし / ESP-NOW 用)
  │     ├─ ESP-NOW 初期化
  │     └─ ピア登録 (receiver_mac)
  │
  ├─ TempSensor::new(GPIO2, GPIO4, rmt.channel0)
  │
  └─ loop
        ├─ sensor.read_temperature()
        │     ├─ GPIO2 HIGH (電源 ON, 500ms 待機)
        │     ├─ 1-Wire デバイス検索 (family code 0x28)
        │     ├─ CONVERT TEMP コマンド送信 (800ms 待機)
        │     ├─ READ SCRATCHPAD でデータ取得
        │     ├─ GPIO2 LOW (電源 OFF)
        │     └─ 生データ → ℃ 変換 (raw / 16.0)
        │
        ├─ info!("Temperature: {:.2}°C")
        │
        ├─ [wifi feature] send_temperature()
        │     ├─ HASH フレーム送信 (type=1)
        │     └─ EOF フレーム送信 (type=3)
        │
        └─ sleep_or_delay(measure_interval_s)
              ├─ use_deep_sleep=false: FreeRtos::delay_ms()
              └─ use_deep_sleep=true : esp_deep_sleep() → リセット
```

### 待機方式の比較

| 方式 | `use_deep_sleep` | 消費電力 | USB モニタ | 備考 |
|------|-----------------|---------|-----------|------|
| FreeRTOS delay | `false` (デフォルト) | 高 | 継続可 | 開発・デバッグ向け |
| Deep Sleep | `true` | 低 (~20µA) | 切断される | 本番・電池駆動向け |

> **Deep Sleep 時の注意**:  
> `esp_deep_sleep()` 後はデバイスがリセットされ `main()` から再実行される。  
> WiFi (ESP-NOW) も毎回初期化が必要となる。`wifi` feature 使用時も問題なく動作する。

---

## 設定リファレンス (`cfg.toml`)

`cfg.toml.template` を `cfg.toml` にコピーして編集する。  
`cfg.toml` が存在しない場合、または項目が未定義の場合はデフォルト値を使用。

```toml
[xiao-esp32s3-get-temperature]
measure_interval_s = 600      # 計測間隔 (秒) / デフォルト: 600 (10分)
use_deep_sleep = false        # Deep Sleep 使用 / デフォルト: false
receiver_mac = "11:22:33:44:55:66"  # ESP-NOW 送信先 MAC (wifi feature 時のみ)
```

| 項目 | 型 | デフォルト | 説明 |
|------|----|----------|------|
| `measure_interval_s` | u32 | `600` | 計測間隔 (秒) |
| `use_deep_sleep` | bool | `false` | Deep Sleep 使用フラグ |
| `receiver_mac` | str | `"11:22:33:44:55:66"` | ESP-NOW 送信先 MAC |

---

## ビルド・フラッシュ手順

### 前提条件

```bash
# Espressif Rust ツールチェーンのインストール
espup install

# ツールチェーン有効化
source ~/export-esp.sh  # または ~/.config/espup/export-esp.sh

# espflash のインストール
cargo install espflash
```

### Phase 1 (ログのみ)

```bash
cd devices/xiao_esp32s3_temp_sensor

# フラッシュ & モニタ起動
cargo run

# または release ビルド
cargo run --release
```

### Phase 2 (ESP-NOW 送信)

```bash
cd devices/xiao_esp32s3_temp_sensor

# 1. cfg.toml を作成
cp cfg.toml.template cfg.toml

# 2. cfg.toml を編集: receiver_mac を設定
#    ゲートウェイデバイス (sensor_data_reciver 側) の MAC を確認して記入

# 3. フラッシュ & モニタ
cargo run --features wifi
```

---

## Phase 2: ESP-NOW プロトコル仕様

### アーキテクチャ概要

```
XIAO ESP32-S3        XIAO ESP32-C3 (usb_cdc_receiver)      PC (sensor_data_reciver)
     │                          │                                      │
     │── ESP-NOW (生テキスト) ──>│                                      │
     │                          │── USB CDC (バイナリフレーム) ────────>│
     │                          │   [START][MAC][TYPE][SEQ][LEN]       │
     │                          │   [DATA][CHECKSUM][END]              │
     │                          │                                      │ InfluxDB 書き込み
```

**ESP-NOW ペイロードは生テキスト**で送信する。バイナリフレームへのラップは
ESP32-C3 (`usb_cdc_receiver`) が USB CDC 送信時に自動的に行う。

ESP32-C3 の `detect_frame_type` は ESP-NOW ペイロードの先頭テキストで種別を判定:
- `"HASH:"` で始まる → Hash フレームとして中継
- `"EOF!"` (4 バイト) → Eof フレームとして中継
- その他 → Data フレームとして中継 (温度センサでは使用しない)

### 温度送信シーケンス

```
XIAO ESP32-S3          ESP32-C3 (usb_cdc_receiver)   sensor_data_reciver
    │                           │                            │
    │── ESP-NOW ──────────────>│                            │
    │  "HASH:000...0,VOLT:100, │                            │
    │   TEMP:25.5,TDS_VOLT:    │                            │
    │   -999.0,..."            │── USB CDC フレーム ────────>│
    │                           │  (バイナリラップ済み)       │
    │  (50ms 待機)              │                            │
    │                           │                            │
    │── ESP-NOW ──────────────>│                            │
    │  "EOF!" (4バイト)         │── USB CDC フレーム ────────>│
    │                           │                            │ InfluxDB に temperature 書き込み
```

### HASH ペイロード仕様

| フィールド | 値 | 説明 |
|-----------|-----|------|
| `HASH:` | 64桁ゼロ | 画像なし (ダミーハッシュ) |
| `VOLT:` | `100` | 電圧センサなし (プレースホルダ) |
| `TEMP:` | `25.5` (小数1桁) | 温度 (℃) |
| `TDS_VOLT:` | `-999.0` | TDS センサなしのセンチネル値 (サーバー側で None として扱われ InfluxDB には書き込まれない) |

### ESP-NOW ペイロードサイズ検証

```
HASH ペイロード: ~128 バイト  ← ESP-NOW 上限 250 バイト以内 ✓
EOF ペイロード:    4 バイト   ← ESP-NOW 上限 250 バイト以内 ✓
```

### USB CDC フレーム構造 (ESP32-C3 が付与)

ESP32-C3 が USB CDC 送信時に付与するバイナリフレームの参考情報:

```
[START(4B)][MAC(6B)][TYPE(1B)][SEQ(4B LE)][LEN(4B LE)][DATA][CHECKSUM(4B LE)][END(4B)]
```

| フィールド | 値 |
|-----------|-----|
| START_MARKER | `0xFA 0xCE 0xAA 0xBB` |
| MAC | ESP-NOW 送信元 MAC (ESP32-S3 の STA MAC) |
| TYPE | 1=HASH, 3=EOF |
| DATA | ESP-NOW ペイロード (生テキスト) |

---

## 関連ファイル

| ファイル | 役割 |
|---------|------|
| [esp-temp-sensor (GitHub)](https://github.com/junkei-okinawa/esp-temp-sensor) | DS18B20 ライブラリ本体 |
| `devices/xiao_esp32s3_sense/src/communication/esp_now/sender.rs` | ESP-NOW 送信実装 (参照実装) |
| `devices/xiao_esp32s3_sense/src/communication/network_manager.rs` | WiFi 初期化パターン (参照実装) |
| `server/sensor_data_reciver/protocol/frame_parser.py` | サーバー側フレームパーサー |
