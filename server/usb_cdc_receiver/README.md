# ESP-NOW USB CDC Reciver

ESP-NOWプロトコルを使用してESP32カメラモジュールから送信された画像データを受信し、USB CDC（Communications Device Class）を通じてホストPCに転送するソフトウェアです。

## プロジェクト概要

このプロジェクトは、ESP32-C3マイクロコントローラーを使用して構築されており、以下の主要な機能を提供します：

- 複数のESP32カメラからESP-NOWプロトコルを通じて送信される画像データの受信
- 受信データのバッファリングとキューイング
- USB CDCを使用してホストPCへのデータ転送
- MACアドレスによるデータ送信元の識別
- 設定ファイルを使用したカメラ登録と管理

## アーキテクチャ

プロジェクトは以下のモジュールで構成されています：

### モジュール構成

- **config**: カメラの設定とMACアドレス管理
- **esp_now**: ESP-NOWプロトコル処理、フレーム操作
- **mac_address**: MACアドレスのパースと検証
- **queue**: データキューの実装とスレッド間通信
- **usb**: USB CDC通信の管理

### データフロー

1. ESP32カメラから送信されたデータをESP-NOWプロトコルで受信
2. 受信したデータを内部キューにバッファリング
3. メインループでキューからデータを取得
4. USB CDC経由でホストPCにデータを送信

## 使用方法

### 必要条件

- Rust（1.70以上）
- ESP-IDF（v5.1以上）
- ESP32-C3マイクロコントローラー
- cargo tools：`cargo-espflash`

### セットアップと構成

1. リポジトリをクローン：
   ```bash
   git clone https://github.com/junkei-okinawa/esp-camera-rs.git
   cd esp-camera-rs/examples/usb_cdc_receiver
   ```

2. 設定ファイルのセットアップ：
   ```bash
   cp cfg.toml.example cfg.toml
   ```

3. `cfg.toml`を編集して、受信するカメラのMACアドレスを設定：
   ```toml
   image_sender_cam1 = "AA:BB:CC:DD:EE:FF"
   image_sender_cam2 = "11:22:33:44:55:66"
   ```

### ビルドと書き込み

プロジェクトをビルドして、ESP32-C3デバイスにフラッシュするには：

```bash
cargo espflash flash --release --port /dev/your-port --monitor
```

### USBシリアルポートの使用

プログラムが起動すると、ESP32-C3は仮想シリアルポートとしてホストPCに認識されます。このシリアルポートを介して、画像データが送信されます。

## テスト

### 単体テスト実行

コードの単体テストを実行するには：

```bash
cargo test --lib --target=riscv32imc-esp-espidf
```

または特定のモジュールのみテスト：

```bash
cargo test --lib mac_address --target=riscv32imc-esp-espidf
```

テストカバレッジ：

- mac_address: MACアドレス処理のテスト
- esp_now: フレームの検出、チェックサム計算のテスト
- queue: データキューのテスト
- config: 設定ファイルからのカメラ構成のテスト

## モジュール解説

### config

設定ファイル（cfg.toml）からカメラ情報を読み込み、ESP-NOWプロトコル用のピア管理を行います。

```rust
// カメラ設定の例
pub struct CameraConfig {
    pub name: String,
    pub mac_address: MacAddress,
}
```

### esp_now

ESP-NOWプロトコルを使用したデータ受信と処理を行います。フレーム検出、チェックサム検証、シーケンス番号管理などの機能があります。

### mac_address

MACアドレスの解析、検証、フォーマット機能を提供します。

```rust
// MACアドレス使用例
let mac = MacAddress::from_str("AA:BB:CC:DD:EE:FF").unwrap();
println!("フォーマットされたMAC: {}", mac);
```

### queue

スレッドセーフなデータキューを実装し、ESP-NOWコールバックとメインループ間の通信を可能にします。

### usb

USB CDC通信を管理し、受信したデータをホストPCに送信します。

## デバッグ

ログレベルは`main.rs`の以下の行で設定できます：

```rust
log::set_max_level(log::LevelFilter::Info);
```

ログレベルを`Debug`に変更することで、より詳細な情報を得られます。

## 拡張性

- 新しいカメラを追加するには、`cfg.toml`ファイルにMACアドレスを追加するだけです。現状`esp-now`の`peer`登録上限の6デバイスまでに対応。

## ライセンス

このプロジェクトは[MIT License](LICENSE)の下でライセンスされています。

## 貢献

バグ報告や機能リクエストについては、Issueトラッカーを使用してください。プルリクエストも歓迎します。
