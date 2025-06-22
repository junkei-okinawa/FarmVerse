# ESP32 M5Stack Unit Cam - Image Sensor Data Sender

[M5Stack Unit Cam Wi-Fi Camera (OV2640)](https://docs.m5stack.com/en/unit/unit_cam)を使用したESP32ベースの画像センサーデータ送信システムです。効率的で保守しやすいモジュラー構造により、安定した画像撮影・送信機能を提供します。

## 🎯 プロジェクト概要

このプロジェクトは、ESP32マイクロコントローラーとOV2640カメラモジュールを使用して、以下の機能を提供します：

### 🌟 主要機能

- **📸 自動画像撮影**: 定期的な高品質画像キャプチャ
- **📡 ESP-NOW通信**: 低遅延のワイヤレス画像送信
- **🔋 インテリジェント電源管理**: 電圧レベルに基づく動作制御とディープスリープ
- **⚡ 効率的データ処理**: チャンク分割による大容量画像データの確実な送信
- **🔍 データ整合性**: SHA256ハッシュによる画像データ検証
- **💡 ステータス表示**: LEDによる動作状態の視覚的フィードバック
- **🌐 IoT連携**: FarmVerseプラットフォームとの統合

## 🏗️ アーキテクチャ

### モジュラー構造

```
📁 src/
├── 🎯 core/                    # コアシステム
│   ├── config.rs              # 設定管理・TOML解析
│   ├── app_controller.rs      # アプリケーション制御・スリープ管理
│   ├── data_service.rs        # データ収集・送信サービス
│   └── rtc_manager.rs         # RTC時刻管理
├── 🔧 hardware/               # ハードウェア制御
│   ├── camera/                # OV2640カメラ制御
│   ├── led/                   # ステータスLED制御
│   ├── pins.rs                # GPIO ピン管理
│   └── voltage_sensor.rs      # ADC電圧監視
├── 📡 communication/          # 通信機能
│   ├── esp_now/               # ESP-NOW プロトコル実装
│   └── network_manager.rs     # WiFi・ネットワーク管理
├── 🔋 power/                  # 電源管理
│   └── sleep/                 # ディープスリープ制御
└── 📍 mac_address.rs          # MACアドレス処理
```

### 🔄 データフロー

```mermaid
graph TD
    A[起動・初期化] --> B[電圧チェック]
    B --> C{電圧OK?}
    C -->|Yes| D[カメラ初期化]
    C -->|No| E[スリープモード]
    D --> F[画像撮影]
    F --> G[画像データ処理]
    G --> H[ESP-NOW送信]
    H --> I[送信完了確認]
    I --> J[ディープスリープ]
    E --> J
    J --> A
```

## 🚀 機能詳細

### 📸 スマート画像撮影
- **電圧監視**: ADC電圧8%以下で撮影スキップ
- **品質最適化**: ウォームアップフレームによる画質安定化
- **解像度設定**: SVGA (800x600) から QSXGA (2592x1944) まで対応
  - ⚠️ **注意**: M5Stack Unit Cam (ESP32-WROOM-32E) はメモリ制限により、SVGA以上の解像度ではメモリ不足エラーが発生する可能性があります

### 📡 効率的通信
- **ESP-NOW**: WiFiルーター不要の直接通信
- **チャンク送信**: 大容量画像の分割送信（1024バイトチャンク）
- **エラーハンドリング**: 再送機能とタイムアウト制御

### 🔋 電源最適化
- **アダプティブスリープ**: 60秒（通常） / 3600秒（ADC電圧低下時）
- **タイミング制御**: 複数デバイス運用時の送信時刻分散

## 🛠️ セットアップ

### 必要条件

- **Rust** 1.70以上 + ESP32ツールチェーン
- **cargo-espflash** ESP32フラッシュツール
- **M5Stack Unit Cam** (ESP32 + OV2640カメラ)
- **ESP-IDF** 5.0以上

### インストール

1. **ESP32 Rustツールチェーンのセットアップ**
   ```bash
   # espupツールのインストール
   cargo install espup
   espup install
   
   # 環境変数の設定
   . $HOME/export-esp.sh
   
   # espflashツールのインストール
   cargo install cargo-espflash
   ```

2. **プロジェクトのクローン**
   ```bash
   git clone <repository-url>
   cd FarmVerse/devices/m5stack_unit_cam
   ```

3. **設定ファイルの作成**
   ```bash
   cp cfg.toml.template cfg.toml
   # cfg.tomlを編集して設定をカスタマイズ
   ```

4. **ビルドとフラッシュ**
   ```bash
   # デバッグビルド
   cargo build
   
   # リリースビルド
   cargo build --release
   
   # ESP32にフラッシュ（シリアルポートは環境に合わせて変更）
   cargo espflash flash --release --port /dev/tty.usbserial-xxxxxxxx --monitor --partition-table partitions.csv
   
   # macOSの場合のポート例
   # /dev/tty.usbserial-xxxxxxxx または /dev/tty.SLAB_USBtoUART
   
   # Linuxの場合のポート例  
   # /dev/ttyUSB0 または /dev/ttyACM0
   
   # Windowsの場合のポート例
   # COM3 または COM4
   ```

## ⚙️ 設定

### cfg.toml設定項目

| 項目 | 説明 | デフォルト値 |
|------|------|--------------|
| `receiver_mac` | 送信先MACアドレス | "11:22:33:44:55:66" |
| `timezone` | タイムゾーン | "Asia/Tokyo" |
| `sleep_duration_seconds` | 通常スリープ時間(秒) | 60 |
| `sleep_duration_seconds_for_long` | 長時間スリープ時間(秒) | 3600 |
| `frame_size` | カメラ解像度 ⚠️ M5Stack Unit CamはSVGA推奨 | "SVGA" |
| `auto_exposure_enabled` | 自動露光調整 | true |
| `camera_warmup_frames` | ウォームアップフレーム数 | 2 |

### ピン配置 (M5Stack Unit Cam)

| 機能 | GPIO | 説明 |
|------|------|------|
| LED | GPIO4 | ステータス表示 |
| 電圧センサー | GPIO0 | ADC電圧測定 ⚠️ **重要**: 起動時注意 |
| カメラクロック | GPIO27 | OV2640 XCLK |
| カメラデータ | GPIO32,35,34,5,39,18,36,19 | D0-D7 |
| カメラ制御 | GPIO22,26,21 | VSYNC,HREF,PCLK |
| I2C | GPIO25,23 | SDA,SCL |

#### ⚠️ GPIO0 (電圧センサー) 取り扱い注意

**GPIO0は起動モード制御ピンです。取り扱いには十分注意してください：**

1. **初回起動時**: GPIO0をショートさせず、通常起動させる
2. **初回ディープスリープ確認**: デバイスが正常にディープスリープに入ったことを確認
3. **電圧測定開始**: ディープスリープ確認後にGPIO0をショートさせる

**⚠️ 警告**: デバイス起動時（電源投入時やリセット時）にGPIO0がGNDにショートしていると、ESP32がブートモードに入り正常に動作しません。

## 🔄 システム連携

### FarmVerse エコシステム

```
┌─────────────────┐    ESP-NOW   ┌─────────────────┐    HTTP/WebSocket   ┌─────────────────┐
│  M5Stack Unit   │ ──────────── │   Receiver      │ ─────────────────── │  FarmVerse      │
│  Cam (Sender)   │              │   (ESP32/PC)    │                     │  Server         │
└─────────────────┘              └─────────────────┘                     └─────────────────┘
```

### 受信機との連携

このプロジェクトは、以下の受信機と連携します：
- **ESP32受信機**: `../examples/usb_cdc_receiver/`
- **PC受信機**: `../../server/sensor_data_reciver/`

詳細は各プロジェクトのREADMEを参照してください。

## 🧪 開発・テスト

### ログ監視
```bash
# シリアル出力監視（フラッシュと同時）
cargo espflash flash --release --port /dev/tty.usbserial-xxxxxxxx --monitor --partition-table partitions.csv

# すでにフラッシュ済みの場合のモニターのみ
cargo espflash monitor --port /dev/tty.usbserial-xxxxxxxx

# ログレベル調整
export RUST_LOG=debug
cargo espflash flash --release --port /dev/tty.usbserial-xxxxxxxx --monitor --partition-table partitions.csv
```

### デバッグ機能
- **LEDステータス**: 
  - 点灯: 処理中
  - 短い点滅: 成功
  - 長い点滅: エラー
- **電圧監視**: ADC電圧8%以下で撮影スキップ
- **ハッシュ検証**: データ整合性確認

### トラブルシューティング

| 問題 | 原因 | 解決方法 |
|------|------|----------|
| カメラ初期化失敗 | ピン接続・電源 | 配線確認、電圧測定 |
| ESP-NOW送信失敗 | MACアドレス・距離 | 設定確認、受信機の状態 |
| 頻繁なスリープ | ADC電圧低下 | 電源供給の改善 |
| メモリ不足エラー | 高解像度設定 | frame_sizeをSVGA以下に変更 |
| デバイス起動失敗 | GPIO0ショート | 起動時はGPIO0をオープンにする |
| ブートモードに入る | GPIO0がGNDにショート | GPIO0の接続を確認、起動後に接続 |

## 📊 パフォーマンス

### 通信性能
- **画像サイズ**: ~30KB (SVGA, JPEG圧縮)
- **送信時間**: ~3-5秒 (1024バイトチャンク)
- **通信距離**: ~100m (見通し良好時)

## 🔧 カスタマイズ

### 解像度変更
```toml
# cfg.toml
frame_size = "VGA"  # 640x480（M5Stack Unit Camで安全）
# frame_size = "QVGA"  # 320x240（より低解像度、メモリ使用量少）
# frame_size = "SVGA"  # 800x600（デフォルト、推奨最大）
```
⚠️ **注意**: M5Stack Unit Cam (ESP32-WROOM-32E) では、SVGA以上の解像度は避けてください。

### 送信間隔調整
```toml
# cfg.toml
sleep_duration_seconds = 300  # 5分間隔
```

### 複数デバイス運用
```toml
# cfg.toml
target_minute_last_digit = 0  # 毎時x0分に送信
target_second_last_digit = 1  # 毎分x1秒に送信
```

## 📄 ライセンス

このプロジェクトはMITライセンスの下で公開されています。詳細は[LICENSE](../../LICENSE)ファイルを参照してください。

---

**FarmVerse Project** - 持続可能な農業のためのIoTプラットフォーム