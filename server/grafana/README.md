# Grafana Alerting Provisioning

Raspberry Pi (`raspberrypi-base.local`) 上で稼働する Grafana (v12, systemd パッケージインストール) の
**アラートルール / contact point / notification policy** を Git 管理し、`sensor_data_reciver` と同じ
git sparse-checkout の仕組みで Pi に配信するためのディレクトリ。

既存の `RoofBalcony` ダッシュボードと InfluxDB データソースは今回の対象外で、引き続き Grafana UI で管理する
(sqlite `/var/lib/grafana/grafana.db` に保存されたまま)。ここで管理するのは新規追加分のアラート設定のみ。

## 構成

```
server/grafana/
├── .env.example                     # SLACK_WEBHOOK_URL のテンプレート
├── provisioning/alerting/
│   ├── rules.yaml                   # アラートルール本体(Fluxクエリ)
│   ├── contact-points.yaml          # Slack contact point
│   └── notification-policies.yaml   # ルートポリシー(→ slack-alerts)
└── scripts/
    └── deploy_grafana_provisioning.sh
```

アラートルールは `expected_silence_s` / `sensor_error_temp` という、`sensor_data_reciver` が InfluxDB の
`data` measurement に書き込むフィールド(ブランチ `feat/server-liveness-monitoring` で追加)を前提にしている。

- **Device liveness: silence exceeded**: デバイスごとに「最後のデータ受信からの経過秒数」が
  「その時点でサーバーが指示した(またはセンサー系の想定)スリープ秒数 + 5分マージン」を超えたら発火。
- **Sensor error: TEMP sentinel detected**: 直近20分以内に `TEMP:-999`(センサー読み取り失敗)を検出。

## 初回セットアップ(Pi上、1回だけ)

1. sparse-checkout に `server/grafana` を追加:
   ```bash
   cd /home/junkei/Documents/production/sensor_data_reciver_repo
   git sparse-checkout add server/grafana
   git pull
   ```
2. Grafana に専用の環境変数ファイルを読み込ませる(パッケージ標準の `/etc/default/grafana-server` は汚さず、
   systemd override で追加する):
   ```bash
   sudo systemctl edit grafana-server.service
   ```
   エディタが開いたら以下を追記して保存:
   ```
   [Service]
   EnvironmentFile=-/etc/grafana/farmverse.env
   ```
   (先頭の `-` は、ファイルが無くても起動失敗しないためのもの)
3. Slack Incoming Webhook を作成し、`.env` を用意:
   ```bash
   cp server/grafana/.env.example server/grafana/.env
   $EDITOR server/grafana/.env   # SLACK_WEBHOOK_URL= に実際のURLを設定
   ```
4. デプロイスクリプトを実行:
   ```bash
   ./server/grafana/scripts/deploy_grafana_provisioning.sh
   ```

## 更新のたびに(2回目以降)

```bash
cd /home/junkei/Documents/production/sensor_data_reciver_repo
git pull
./server/grafana/scripts/deploy_grafana_provisioning.sh
```

`.env`(Slack Webhook URL)を変更しない限り、`.env` は既に Pi 上に存在するのでそのまま使われる。

## 動作確認

1. `journalctl -u grafana-server -n 50` で provisioning エラーが出ていないか確認。
2. Grafana UI → Alerting → Alert rules に「FarmVerse Alerts」フォルダと2ルールが表示されることを確認。
3. Alerting → Contact points → `slack-alerts` の「Test」ボタンで Slack に通知が届くことを確認。
4. 可能であれば一時的に閾値やクエリのしきい値を下げて実際にルールを発火させ、Slack 通知が届く一連の流れを
   確認してから元に戻す。

## 既知の注意点

- Fluxクエリはローカルでは実データに対して検証できていないため、初回デプロイ時に journalctl のエラーメッセージ
  を見ながら微調整が必要になる可能性がある。
- `/etc/grafana/provisioning/alerting/` は `root:grafana` 所有のため、デプロイには `sudo` が必要
  (デプロイスクリプト内で都度 `sudo` を要求される)。
