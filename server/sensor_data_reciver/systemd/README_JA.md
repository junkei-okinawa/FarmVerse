# Python USB CDC Sensor Data Receiver サービスの systemd 常駐化手順

## 前提条件
リポジトリのルートディレクトリで以下の手順を実行しアプリケーションが動作することを確認しておく

```bash
uv venv # 仮想環境作成
source .venv/bin/activate # 仮想環境有効化
uv sync # 依存関係のインストール
.venv/bin/python app.py
```

## systemd サービスとして常駐起動する手順

1. サービスファイルの配置
   
   `systemd/sensor_data_reciver.service` を `/etc/systemd/system/` にコピーします。
   
   ```bash
   sudo cp systemd/sensor_data_reciver.service /etc/systemd/system/
   ```

2. サービスファイルの編集
   
   - `<user_name>` を実行ユーザー名に書き換えてください。
   - `Group=<group_name>` は必要な場合のみグループ名に書き換え、不要なら削除してください。

3. systemd のリロード
   
   ```bash
   sudo systemctl daemon-reload
   ```

4. サービスの有効化と起動
   
   ```bash
   sudo systemctl enable sensor_data_reciver
   sudo systemctl start sensor_data_reciver
   ```

5. ステータス確認
   
   ```bash
   sudo systemctl status sensor_data_reciver
   ```

---

- サービスのログは `journalctl -u sensor_data_reciver` で確認できます。
- サービスの停止は `sudo systemctl stop sensor_data_reciver` で行えます。
