# Sensor Data Receiver Python Server

このPythonスクリプトは、USB CDC（Communications Device Class）経由でESP32-C3（usb_cdc_receiver）から送信された画像データを受信し、ホストPCにJPEGファイルとして保存する非同期サーバーです。

## プロジェクト概要

`usb_cdc_receiver` プロジェクトと連携し、ESP-NOW経由で複数のESP32カメラから送信され、ESP32-C3によって中継された画像データを受信・処理します。

## 主な機能

-   **非同期シリアル通信**: `serial_asyncio` を使用して、シリアルポートからのデータを効率的に受信します。
-   **カスタムフレームプロトコル解析**:
    -   `START_MARKER` と `END_MARKER` を使用したフレーム同期。
    -   ヘッダーから送信元MACアドレス、フレームタイプ、シーケンス番号、データ長を抽出。
    -   フッターのチェックサム（現在は検証未実装）。
-   **画像データの再構築**: 送信元MACアドレスごとにデータフレーム (`FRAME_TYPE_DATA`) のペイロードをバッファリングします。
-   **画像ファイルの保存**: 終了フレーム (`FRAME_TYPE_EOF`) を受信すると、対応するMACアドレスのバッファを結合し、タイムスタンプ付きのJPEGファイルとして `images_usb_async/` ディレクトリに保存します。
-   **タイムアウト処理**: 一定時間データ受信がないMACアドレスのバッファを破棄し、リソースリークを防ぎます。
-   **統計情報**: 受信した画像の数と合計バイト数を定期的にログに出力します。
-   **設定可能性**: コマンドライン引数でシリアルポートとボーレートを指定できます。

## 使用方法

### 必要条件

-   Python 3.11以上
-   `pyserial-asyncio` ライブラリ (`pyserial` も自動的にインストールされます)

### セットアップ

1.  **依存関係のインストール**:
    プロジェクトディレクトリ (`examples/python_server`) に移動し、`uv` または `pip` を使用して依存関係をインストールします。
    ```bash
    cd examples/python_server
    # uvを使用する場合 (推奨)
    uv sync
    # pipを使用する場合
    # pip install .
    ```
    これにより、`pyproject.toml` に基づいて必要なライブラリ (`pyserial` と `pyserial-asyncio`) がインストールされます。

2.  **画像保存ディレクトリ**:
    スクリプトは実行時に `images_usb_async` ディレクトリを自動的に作成します。

### 実行

以下のコマンドでサーバーを起動します。ESP32-C3デバイスが接続されているシリアルポートを指定してください。

`uv` を使用して依存関係をインストールした場合、以下のコマンドで実行するのが推奨されます:

```bash
uv run python app.py [オプション]
```

`uv` を使用しない場合や、直接 Python インタープリタを使いたい場合は、以下のように実行することも可能です:

```bash
python app.py [オプション]
```

**オプション:**

-   `-p`, `--port`: シリアルポートのパス (デフォルト: `/dev/ttyACM0`)
-   `-b`, `--baud`: ボーレート (デフォルト: 115200)

**例:**

```bash
# デフォルト設定で実行 (uvを使用)
uv run python app.py

# シリアルポートを指定して実行 (uvを使用)
uv run python app.py -p /dev/ttyUSB0

# ポートとボーレートを指定して実行 (uvを使用)
uv run python app.py -p /dev/cu.usbmodem12341 -b 115200

# デフォルト設定で実行 (pythonを直接使用)
# python app.py
```

サーバーは起動すると、指定されたシリアルポートからのデータ受信を開始します。受信した画像は `images_usb_async` ディレクトリに保存されます。Ctrl+Cでサーバーを停止できます。

## データプロトコル

このサーバーは `usb_cdc_receiver` から送信される以下のカスタムフレーム形式を期待します。

```
[START_MARKER (4B)] [MAC Address (6B)] [Frame Type (1B)] [Sequence Num (4B)] [Data Length (4B)] [Data (variable)] [Checksum (4B)] [END_MARKER (4B)]
```

-   **START_MARKER**: `0xfa 0xce 0xaa 0xbb`
-   **MAC Address**: 送信元カメラのMACアドレス
-   **Frame Type**:
    -   `1`: HASH (現在未使用)
    -   `2`: DATA (画像データの一部)
    -   `3`: EOF (画像の最終フレーム)
-   **Sequence Num**: フレームのシーケンス番号 (ビッグエンディアン)
-   **Data Length**: `Data` フィールドのバイト長 (ビッグエンディアン)
-   **Data**: フレームタイプに応じたペイロード (DATAフレームの場合は画像データの一部)
-   **Checksum**: データ部分のチェックサム (現在はサーバー側で検証していません)
-   **END_MARKER**: `0xcd 0xef 0x56 0x78`

## 設定

以下の定数は `app.py` スクリプト内で直接変更できます。

-   `DEFAULT_SERIAL_PORT`: デフォルトのシリアルポート
-   `BAUD_RATE`: デフォルトのボーレート
-   `IMAGE_DIR`: 画像を保存するディレクトリ名
-   `IMAGE_TIMEOUT`: 画像データ受信のタイムアウト時間（秒）

## デバッグ

`app.py` 内の `DEBUG_FRAME_PARSING` フラグを `True` に設定すると、フレーム解析に関する詳細なログが出力されます。

```python
# app.py
# ...
DEBUG_FRAME_PARSING = True # 詳細ログを有効にする場合
# ...
```

ログは標準出力に表示されます。

## ライセンス

このプロジェクトはリポジトリルートの [LICENSE](../../LICENSE) ファイルに基づきます。

## 貢献

バグ報告や改善提案は、GitHubリポジトリのIssueやPull Requestを通じて歓迎します。
