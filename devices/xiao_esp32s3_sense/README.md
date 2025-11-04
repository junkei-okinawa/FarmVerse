# XIAO ESP32S3 Sense - FarmVerse Streaming Camera Device

## 📋 概要

FarmVerse農業監視システムの送信デバイス実装です。XIAO ESP32S3 Senseを使用してOV2640カメラで画像を撮影し、ESP-NOWプロトコルでストリーミング送信を行います。

## 📊 最新テスト結果 (2025年8月11日)

### ✅ **実機テスト成功**
- **画像キャプチャ**: OV2640カメラで **13,291バイト** の実画像を取得
- **チャンク送信**: 250バイト×54チャンクで完全送信成功
- **プロトコル**: ESP-NOWによるHASH→DATA→EOF送信完了
- **設定制御**: cfg.tomlによるテスト設定が正常動作

### 🎯 検証済み機能
```bash
# 実際の送信ログ (2025-08-11実機テスト)
I sensor_data_sender: 画像キャプチャ完了: 13291 bytes
I sensor_data_sender: 画像データ送信完了: 54チャンク送信 (チャンクサイズ: 250バイト)
I sensor_data_sender: ハッシュフレーム送信: HASH:000033eb00117b94,VOLT:0,TEMP:-999.0
I sensor_data_sender: EOF マーカー送信完了
```

## 📡 システムアーキテクチャとデータフロー

### データフロー全体図
```
┌─────────────────────┐
│ xiao_esp32s3_sense  │ ← このデバイス
│   (ESP32-S3)        │
└──────────┬──────────┘
           │ ESP-NOW Frame Protocol
           │ (START: 0xFACEAABB, END: 0xCDEF5678)
           │ big-endian markers / little-endian data
           ▼
┌─────────────────────┐
│ usb_cdc_receiver    │
│  (ESP32-C3)         │
│  1. ESP-NOW受信     │
│  2. バッファリング  │
│  3. USB CDC転送     │
└──────────┬──────────┘
           │ USB CDC (透過転送)
           │ ESP-NOW Frameをそのまま転送
           ▼
┌─────────────────────┐
│sensor_data_receiver │
│   (Python/PC)       │
│  Frame Parser       │
└─────────────────────┘
```

### ESP-NOW Frame Protocol 仕様

**実装場所**:
- 送信: `src/communication/esp_now/sender.rs` - `create_sensor_data_frame()`
- 受信: `server/usb_cdc_receiver/src/esp_now/frame.rs` - `Frame::from_bytes()`
- 解析: `server/sensor_data_reciver/protocol/frame_parser.py` - `FrameParser`

**フレーム構造** (27+ バイト):
```
Offset  Size  Field           Value/Format        Endian
------  ----  --------------  ------------------  -------
0       4     START_MARKER    0xFACEAABB          big
4       6     MAC_ADDRESS     送信元MAC           -
10      1     FRAME_TYPE      1=HASH,2=DATA,3=EOF -
11      4     SEQUENCE_NUM    シーケンス番号       little
15      4     DATA_LEN        データ長             little
19      N     DATA            実データ             -
19+N    4     CHECKSUM        XORチェックサム      little
23+N    4     END_MARKER      0xCDEF5678          big
```

**チェックサム計算** (XOR):
```rust
fn calculate_xor_checksum(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |acc, &byte| acc ^ (byte as u32))
}
```

**重要**: 
- USB CDC Receiverは**透過転送**のみ（フレーム変換なし）
- sensor_data_receiver (Python)がESP-NOW Frameを直接解析
- マーカーはbig-endian、データフィールドはlittle-endian

## 🎯 主要機能

### ✅ 実装済み機能
- **実機カメラキャプチャ**: OV2640センサーによるUXGA(1600x1200)画像撮影 ✅ **動作確認済み**
- **ストリーミング送信**: ESP-NOWプロトコルによる画像チャンク分割送信 ✅ **13.2KB画像送信成功**
- **電力管理**: ADC電圧監視とディープスリープ制御 ✅ **動作確認済み**
- **設定管理**: cfg.tomlによる柔軟な設定変更 ✅ **テスト設定実装完了**
- **テスト・デバッグ機能**: 開発用の詳細制御オプション ✅ **実機テスト対応完了**

### 🚀 次期実装予定
- **ストリーミングプロトコル**: チェックサム検証・リトライ機構
- **マルチデバイス対応**: 複数センサーの並行送信
- **高度な電力管理**: 時刻制御・環境適応スリープ

## 🛠️ ハードウェア構成

### 必要デバイス
- **XIAO ESP32S3 Sense** (8MB PSRAM, WiFi/BLE対応)
- **OV2640カメラモジュール** (内蔵)
- **ADC電圧センサー** (GPIO6)
- **ステータスLED** (GPIO21)

### ピン配置
```
カメラピン配置 (OV2640):
- XCLK: GPIO10    - SIOD: GPIO40    - VSYNC: GPIO46
- PCLK: GPIO13    - SIOC: GPIO39    - HREF:  GPIO38  
- D0-D7: GPIO15,GPIO17,GPIO18,GPIO16,GPIO14,GPIO12,GPIO11,GPIO48

その他:
- ADC: GPIO6 (電圧測定)
- LED: GPIO21 (ステータス表示)
```

## ⚙️ 設定ファイル (cfg.toml)

### 基本設定
```toml
[sensor-data-sender]
# 受信機MACアドレス
receiver_mac = "24:EC:4A:CA:5E:BC"

# スリープ時間設定
sleep_duration_seconds = 600        # 通常スリープ (10分)
sleep_duration_seconds_medium = 3600 # 調整スリープ (1時間)
sleep_duration_seconds_for_long = 32400 # ロングスリープ (9時間)

# カメラ設定  
frame_size = "UXGA"                 # 1600x1200解像度
auto_exposure_enabled = true
camera_warmup_frames = 2

# 通信設定
esp_now_chunk_size = 250           # チャンクサイズ (バイト)
esp_now_chunk_delay_ms = 5         # チャンク間遅延 (ミリ秒)
```

### テスト・デバッグ設定
```toml
# テスト用設定 (実機テスト済み - 2025年8月11日)
force_camera_test = true           # カメラテスト強制実行 ✅ 動作確認済み
bypass_voltage_threshold = true    # 電圧チェックをバイパス ✅ 動作確認済み
debug_mode = true                  # 詳細ログ出力 ✅ 動作確認済み
```

### 実機テスト用設定例
```toml
# カメラテストを強制実行 (電圧0%でも撮影)
force_camera_test = true
bypass_voltage_threshold = true
debug_mode = true

# 期待される動作:
# - ADC電圧0%でもカメラキャプチャ実行
# - 🔧 デバッグログが詳細出力
# - 実画像データ(~13KB)のESP-NOW送信
```

## 🚀 ビルド・実行

### 環境セットアップ
```bash
# Rust ESP32開発環境のインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install cargo-espflash
rustup target add xtensa-esp32s3-espidf

# プロジェクトのビルド
cargo build
```

### フラッシュ・実行
```bash
# デバイスへのフラッシュ (モニター付き)
cargo espflash flash --partition-table partitions.csv --monitor

# ビルドのみ
cargo build
```

### ホストマシンでのユニットテスト (ハードウェア非依存部分)
```bash
# ホストユニットテスト実行
./run_tests.sh

# 実行されるテスト:
# - utils::voltage_calc (電圧計算)
# - utils::tds_calc (TDS計算)
# - utils::streaming_protocol (通信プロトコル)
# - mac_address (MACアドレス処理)
# - core::measured_data (測定データ)
```

### 実機統合テスト (ESP32S3実機が必要)
```bash
# ⚠️ 注意: 統合テストは実機環境が必要

# 1. ESP-IDF環境のセットアップ
# ESP-IDFのインストールパスは環境によって異なります
# 例: . ~/esp/v5.1.6/esp-idf/export.sh
# または環境変数を使用: . $IDF_PATH/export.sh
. ~/esp/v5.1.6/esp-idf/export.sh

# 2. 実機接続確認
ls /dev/tty.usbserial-*  # macOS
ls /dev/ttyUSB*          # Linux

# 3. 統合テスト実行 (実機ビルド・テスト)
cargo test --lib integration_tests

# Note: 統合テストは src/lib.rs::integration_tests モジュールに実装
# ESP-IDF 依存のため、実機が必要
```

## 📊 動作フロー

```
1. 起動・初期化
   ├─ 設定ファイル読み込み (cfg.toml)
   ├─ WiFi/ESP-NOW初期化
   ├─ ADC電圧測定
   └─ ペリフェラル初期化

2. 画像キャプチャ判定
   ├─ 電圧チェック (> 8% or bypass_voltage_threshold)
   ├─ 強制実行チェック (force_camera_test)
   └─ カメラ初期化・撮影

3. データ送信
   ├─ 画像チャンク分割 (250バイト単位)
   ├─ ESP-NOW送信 (チャンク→HASH→EOF)
   └─ 送信完了確認

4. スリープ制御
   ├─ サーバーからのスリープコマンド待機 (10秒)
   ├─ タイムアウト時はデフォルト時間使用
   └─ ディープスリープ移行
```

## 🧪 テスト・デバッグ

### カメラテストの実行
```toml
# cfg.tomlで以下を設定
force_camera_test = true
bypass_voltage_threshold = true
debug_mode = true
```

### デバッグログ確認
```bash
# シリアルモニター付きフラッシュ
cargo espflash flash --partition-table partitions.csv --monitor

# 期待されるログ出力例:
# I sensor_data_sender: 🔧 デバッグ: 画像キャプチャ開始 - 電圧:0%, force_camera_test:true
# I sensor_data_sender: 🔧 デバッグ: カメラテストを強制実行中
# I sensor_data_sender: 画像キャプチャを開始 (電圧:0%, 強制実行:true)
```

### エラー対処
```bash
# コンパイルエラー時
cargo check

# ビルドクリーン
cargo clean && cargo build

# 設定ファイル再生成
rm cfg.toml.template && cp cfg.toml.template cfg.toml
```

## 📁 プロジェクト構造

```
src/
├── main.rs                    # メイン実行ファイル
├── lib.rs                     # ライブラリルート
├── config.rs                  # 設定管理
├── mac_address.rs             # MACアドレス処理
├── communication/
│   └── esp_now/
│       ├── mod.rs             # ESP-NOWモジュール
│       ├── sender.rs          # データ送信機能
│       ├── receiver.rs        # データ受信機能
│       ├── frame.rs           # フレーム処理
│       └── streaming.rs       # ストリーミング機能 (実装中)
├── hardware/
│   ├── camera/
│   │   ├── mod.rs             # カメラモジュール
│   │   ├── controller.rs      # OV2640制御
│   │   ├── config.rs          # カメラ設定
│   │   ├── ov2640.rs          # OV2640ドライバー
│   │   └── xiao_esp32s3.rs    # XIAO ESP32S3設定
│   ├── led/
│   │   └── status_led.rs      # ステータスLED制御
│   └── voltage_sensor.rs      # ADC電圧測定
├── core/
│   ├── mod.rs
│   ├── app_controller.rs      # アプリケーション制御
│   ├── data_service.rs        # データ処理サービス
│   ├── config.rs              # 設定処理
│   └── rtc_manager.rs         # 時刻管理
├── power/
│   ├── mod.rs
│   └── sleep/
│       └── deep_sleep.rs      # ディープスリープ制御
└── tests/
    ├── mod.rs                 # テストモジュール
    ├── camera_tests.rs        # カメラテスト
    ├── streaming_tests.rs     # ストリーミングテスト
    └── large_image_tests.rs   # 大容量画像テスト
```

## 🎯 開発ロードマップ

### ✅ フェーズ1: 基盤実装 (✅ **完了 - 実機テスト成功**)
- [x] XIAO ESP32S3カメラ初期化 ✅ **OV2640動作確認済み**
- [x] ストリーミング送信基盤 ✅ **13.2KB画像送信成功**
- [x] テスト・デバッグ機能 ✅ **cfg.toml制御実装完了**
- [x] 設定管理システム ✅ **force_camera_test等動作確認済み**

### 🚀 フェーズ2: 中継側実装 (次期)
- [ ] usb_cdc_receiver Streaming Architecture
- [ ] マルチデバイス並行処理
- [ ] バックプレッシャー制御

### 🎯 フェーズ3: システム統合 (将来)
- [ ] End-to-End統合テスト
- [ ] 性能最適化
- [ ] 本番環境対応

## 📋 関連Issue

- **[#11 - FarmVerse Streaming化実装](https://github.com/junkei-okinawa/FarmVerse/issues/11)** (Epic)
- **[#12 - 送信側XIAO ESP32S3 Sense対応実装](https://github.com/junkei-okinawa/FarmVerse/issues/12)** (Current)
- **[#13 - 中継usb_cdc_receiver Streaming Architecture実装](https://github.com/junkei-okinawa/FarmVerse/issues/13)** (Next)

## 🔧 トラブルシューティング

### よくある問題

1. **カメラ初期化失敗**
   ```
   解決方法: 電源供給の確認、ピン配置の再確認
   ```

2. **ESP-NOW送信失敗**
   ```
   解決方法: 受信機MACアドレスの確認、WiFiチャンネル設定
   ```

3. **メモリ不足エラー**
   ```
   解決方法: PSRAMの有効化確認、チャンクサイズの調整
   ```

### サポート情報
- **開発環境**: ESP-IDF v5.1.3, Rust 1.80+
- **対象デバイス**: XIAO ESP32S3 Sense
- **通信プロトコル**: ESP-NOW
- **画像形式**: JPEG (OV2640出力)

---

**更新日**: 2025年8月11日  
**バージョン**: v0.2.0  
**ステータス**: ✅ **フェーズ1完了、実機カメラテスト成功 (13.2KB画像送信確認)**
