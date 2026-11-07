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

## ホストのストレージ監視 (host_storage_monitor)

`monitoring/host_storage_monitor.py` を 10 分ごとに実行し、ルート FS の使用量・`images/` のサイズ・
`~/.xsession-errors*` のサイズを InfluxDB の measurement `host_storage` に書き込みます
(tag: `host`, `mount` / field: `total_bytes`, `used_bytes`, `free_bytes`, `used_percent`,
`images_bytes`, `images_files`, `xsession_errors_bytes`)。Grafana から参照できます。

```bash
# <user_name> を実際のユーザー名に置換してから配置する
sudo cp systemd/host_storage_monitor.service systemd/host_storage_monitor.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now host_storage_monitor.timer
```

> `/etc/systemd/system/` の unit はリポジトリと自動同期されません。unit を更新したら再度コピーして
> `daemon-reload` してください。

Grafana (Flux) のクエリ例:

```flux
from(bucket: "balcony")
  |> range(start: v.timeRangeStart, stop: v.timeRangeStop)
  |> filter(fn: (r) => r._measurement == "host_storage" and r._field == "used_percent")
```
