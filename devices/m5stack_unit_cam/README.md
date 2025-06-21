# ESP32 Camera ImageSender

[M5Stack Unit Cam Wi-Fi Camera (OV2640)](https://docs.m5stack.com/en/unit/unit_cam)で撮影した画像をESP-NOWプロトコル経由で送信するソフトウェアです。

## プロジェクト概要

このプロジェクトは、ESP32マイクロコントローラーとカメラモジュールを使用して構築されており、以下の主要な機能を提供します：

- ESP32カメラモジュールによる定期的な画像撮影
- 撮影した画像のSHA256ハッシュ計算と検証
- ESP-NOWプロトコルを使用した画像データのチャンク送信
- 省電力のためのディープスリープ制御（サーバーからの指示またはフォールバック設定に基づく）
- LED表示によるステータス通知

## 受信機との連携

このプロジェクトは、同じリポジトリ内の`usb_cdc_receiver`プロジェクトと連携して動作します。送信機（このプロジェクト）が撮影した画像を、受信機（`usb_cdc_receiver`）がESP-NOWプロトコルで受信し、USBシリアル経由でPCに転送します。

詳細な受信機の設定と使い方については、[usb_cdc_receiver README](../usb_cdc_receiver/README.md)を参照してください。

## アーキテクチャ

プロジェクトは以下のモジュールで構成されています：

### モジュール構成

- **camera**: カメラ初期化と画像キャプチャ処理
- **config**: 設定ファイルからの構成読み込み
- **esp_now**: ESP-NOWプロトコル通信と画像フレーム処理
- **led**: ステータス表示用LEDの制御
- **mac_address**: MACアドレス処理
- **sleep**: ディープスリープ制御。サーバーから次回の起動間隔を受信し、それに従ってスリープします。受信できない場合は設定ファイルに基づいたフォールバック動作を行います。

### データフロー

1. ESP32カメラで画像を撮影
2. 撮影した画像のSHA256ハッシュを計算
3. 画像データをチャンクに分割
4. ESP-NOWプロトコルを使用して受信機に送信
   - 最初にハッシュ情報を送信
   - 次に画像データをチャンクに分けて送信
   - 最後にEOFマーカーを送信
5. 送信完了後、サーバーからの応答を待ち、次回の起動時刻指示を受信。
6. 指示された時間、または指示がない場合は設定に基づいた時間ディープスリープに移行し、その後再び起動して次の撮影サイクルを開始。

## 使用方法

### 必要条件

- Rust（1.71以上）
- ESP-IDF（v5.1以上）
- ESP32 + カメラモジュール (本プロジェクトでは`Unit Cam Wi-Fi Camera (OV2640)`を使用)
- cargo tools：`cargo-espflash`

### セットアップと構成

1. リポジトリをクローン：
   ```bash
   git clone https://github.com/junkei-okinawa/esp-camera-rs.git
   cd esp-camera-rs/examples/image_sender
   ```

2. 設定ファイルのセットアップ：
   ```bash
   cp cfg.toml.template cfg.toml
   ```

3. `cfg.toml`を編集して、受信機のMACアドレスとディープスリープ時間を設定：
   ```toml
   [image-sender]
   receiver_mac = "1A:2B:3C:4D:5E:6F"  # 受信機のMACアドレスに変更
   sleep_duration_seconds = 60         # フォールバック用のディープスリープ時間（秒）
   ```
   
   `sleep_duration_seconds`の値を変更することで、撮影と送信の間隔を調整できます。例えば、5分間隔で撮影したい場合は`300`に設定します。

### 異なるカメラモジュールへの対応

異なるESP32ボードやカメラモジュールを使用する場合は、以下の設定が必要になります：

1. `.cargo/config.toml`の修正：ESP32のターゲットに合わせて変更
2. `sdkconfig.defaults`の変更：PSRAMの設定などを調整
3. カメラのピン設定：`src/camera/controller.rs`のピン設定を変更

### ビルドと書き込み

プロジェクトをビルドして、ESP32デバイスにフラッシュするには：

```bash
cargo espflash flash --release --port /dev/your-port --monitor --partition-table ../partitions.csv
```

`/dev/your-port` は、お使いの環境におけるESP32デバイスのシリアルポートに置き換えてください。
`--partition-table ../partitions.csv` オプションにより、プロジェクトルートの一つ上の階層にある `partitions.csv` をカスタムパーティションテーブルとして使用します。これにより、アプリケーションのバイナリサイズが大きい場合に発生する "image_too_big" エラーを回避できます。

## 設定 (`cfg.toml`)

アプリケーションの動作は、プロジェクトのルートディレクトリ（`image_sender` ディレクトリ直下）に配置する `cfg.toml` ファイルで設定できます。
リポジトリには `cfg.toml.template` が含まれているので、これをコピーして `cfg.toml` というファイル名で保存し、必要に応じて値を編集してください。

```toml
// cfg.toml の例
[image-sender]
# データ送信先のMacAddress（example/usb_cdc_receiver の受信機デバイス）
receiver_mac = "11:22:33:44:55:66"

# フォールバック用のディープスリープ時間（秒）
sleep_duration_seconds = 60

# 起動時刻の調整用パラメータ (オプション)
# これらが設定されている場合、sleep_duration_seconds で指定されたおおよそのスリープ後、
# さらに指定された分の下一桁・秒の下一桁に合致する最も近い未来の時刻まで調整して起動します。
# 例: target_minute_last_digit = 0, target_second_last_digit = 1 の場合、
#   おおよそ sleep_duration_seconds 後に、xx時x0分x1秒のような時刻に起動します。

# 複数デバイスを運用する場合、できる限りデータ送信タイミングをズラしたいので送信タイミングをズラせるようにコメントアウトで目標設定を可能にする
# 起動する「分」の下一桁 (0-9)。コメントアウトまたは未設定の場合はこの条件を無視。
# target_minute_last_digit = 0

# 起動する「秒」の上一桁 (0-5)。コメントアウトまたは未設定の場合はこの条件を無視。
# target_second_last_digit = 1

# ソーラーパネル電圧がゼロになった場合（日没）次の実行までDeepSleepする時間（秒）
sleep_duration_seconds_for_long = 3600

# カメラ解像度（SVGA = 800*600）
frame_size = "SVGA"
# 利用可能な値の例 (詳細は esp-idf-sys のドキュメントを参照):
# "96X96", "QQVGA", "QCIF", "HQVGA", "240X240", "QVGA", "CIF", "HVGA", "VGA", "SVGA",
# "XGA", "HD", "SXGA", "UXGA", "FHD", "P_HD", "P_3MP", "QXGA", "QHD", "WQXGA", "P_FHD", "QSXGA"

# カメラの自動露光調整のON/OFF
auto_exposure_enabled = true

# カメラ撮影画像品質を安定させるために捨て画像撮影回数
camera_warmup_frames = 2

# タイムゾーン (例: "Asia/Tokyo", "America/New_York")
# 有効なタイムゾーン文字列は chrono-tz クレートのドキュメントを参照してください。
timezone = "Asia/Tokyo"
```

### 設定可能な項目

-   `receiver_mac`: (必須) データ送信先のESP-NOW受信側デバイスのMACアドレス。
-   `sleep_duration_seconds`: (必須) 通常のディープスリープ時間（秒）。サーバーから次回の起動指示を受信できなかった場合のフォールバックとして使用されます。
-   `target_minute_last_digit`: (オプション) 起動する「分」の下一桁 (0-9)。コメントアウトまたは未設定の場合はこの条件を無視します。
-   `target_second_last_digit`: (オプション) 起動する「秒」の上一桁 (0-5)。コメントアウトまたは未設定の場合はこの条件を無視します。
    -   `target_minute_last_digit` と `target_second_last_digit` が両方設定されている場合、`sleep_duration_seconds` で指定されたおおよそのスリープ後、さらに指定された分の下一桁・秒の下一桁に合致する最も近い未来の時刻まで起動を遅延させます。
-   `sleep_duration_seconds_for_long`: (必須) ソーラーパネル電圧がゼロになった場合（日没と判断される場合）など、長期間スリープする場合のディープスリープ時間（秒）。
-   `frame_size`: (必須) カメラの解像度。例: `"SVGA"`, `"QVGA"`, `"HD"` など。利用可能な値の完全なリストは `esp-idf-sys` のドキュメントを参照してください。
-   `auto_exposure_enabled`: (必須) カメラの自動露光調整を有効にするか (`true` または `false`)。
-   `camera_warmup_frames`: (必須) カメラ起動時に撮影する捨て画像の枚数。画質安定化のために使用します。
-   `timezone`: (必須) タイムゾーンを指定する文字列。ログ時刻の表示などに使用されます。例: `"Asia/Tokyo"`, `"America/New_York"`。有効なタイムゾーン文字列については、`chrono-tz` クレートから参照されている[List of tz database time zones](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones)のドキュメントを参照してください。

## スリープ制御について (About Sleep Control)

本デバイスのディープスリープ間隔は、主に連携するサーバーアプリケーション (`sensor_data_reciver/app.py`) によって制御されます。

1.  デバイスはデータ（ハッシュ情報、画像データ）を送信後、サーバー（ゲートウェイ経由）からの応答を短時間待ちます。
2.  サーバーは、データ受信時に適切な次のスリープ時間（秒単位）を計算し、ゲートウェイデバイスに特定のシリアルコマンド (`CMD_SEND_ESP_NOW:<target_mac>:<duration_seconds>\n`) で指示します。
3.  ゲートウェイデバイスは、このシリアルコマンドを解釈し、該当するM5Stack Unit CamデバイスにESP-NOWパケットでスリープ時間を送信します。
4.  M5Stack Unit Camデバイスは、このESP-NOWパケットを受信すると、指定された期間スリープします。
5.  サーバーからの応答がない場合や、受信したデータが無効な場合は、`cfg.toml` の `sleep_duration_seconds` や電圧に基づくフォールバックの睡眠ロジックが作動します。

この仕組みにより、SNTPによる時刻同期なしに、サーバー側で柔軟にデバイスの動作間隔を調整できます。

**重要:** この機能は、ESP-NOWとシリアル間の通信を中継するゲートウェイデバイスが、サーバーからのシリアルコマンドを解釈し、M5Stack Unit CamへESP-NOWで適切に応答を転送するよう改造されていることを前提とします。ゲートウェイの改造については本リポジトリの範囲外です。

## テスト

### 単体テスト実行

コードの単体テストを実行するには：

```bash
cargo test --lib
```

PCの開発環境でテストを実行する場合は問題ありませんが、ESP32実機上でテストを実行する際には注意が必要です：

```bash
cargo test --lib --target xtensa-esp32-espidf
```

※注：一部のテストはESP32実機環境では実行できないため、`#[ignore]`属性でマークされています。

テストカバレッジ：

- mac_address: MACアドレス処理のテスト
- esp_now/frame: ハッシュ計算やメッセージ準備のテスト
- camera: カメラ制御のテスト（ハードウェア依存）
- led: LEDパターン制御のテスト（ハードウェア依存）

## テストに関する注意点

現在、ESP32実機上で一部の単体テストを実行（`cargo test --lib --target xtensa-esp32-espidf`）しようとすると、デバイスのスタックサイズ制限によりエラーが発生する場合があります。この問題は今後の課題として認識しており、解決に向けて調査中です。

ホストOS（PC）上でのテストは、ESP-IDFへの依存関係により現状では困難です。今後のリファクタリングでESP-IDF非依存モジュールをテスト可能にするしていきたいと考えています。

## モジュール解説

### camera

カメラの初期化、設定、画像撮影を担当します。M5Stack Unit Camなどの異なるカメラモジュールに対応できるよう、柔軟なピン設定が可能です。

### config

設定ファイル（cfg.toml）から受信機のMACアドレスなどの構成情報を読み込みます。

### esp_now

ESP-NOWプロトコルを使用した通信処理を担当します。主な機能として：
- 送信機の初期化とピア登録
- 画像データのチャンク分割送信
- ハッシュ計算と検証

### led

ステータスLEDの制御を行います。撮影中、送信中、エラー状態などを異なるLEDパターンで表示します。

### mac_address

MACアドレスの解析、検証、文字列変換などの機能を提供します。

### sleep

ディープスリープの制御と電力管理を担当します。撮影・送信サイクルの間の省電力化に貢献します。

## トラブルシューティング

### カメラが認識されない場合

- カメラのピン設定を確認してください。本ソフトウェアは**[Unit Cam Wi-Fi Camera (OV2640)](https://docs.m5stack.com/en/unit/unit_cam)**で動作するように設定されています。
- 電源供給が十分か確認してください

### 画像送信が失敗する場合

- 受信機が起動しているか確認してください
- MACアドレスが正しく設定されているか確認してください
- 送信機と受信機の距離が遠すぎないか確認してください

### ディープスリープが正常に動作しない場合

- ボードがディープスリープに対応しているか確認してください
