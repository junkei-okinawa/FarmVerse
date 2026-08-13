#!/usr/bin/env bash
# server/grafana/ 配下の provisioning 設定と .env を Raspberry Pi 上の Grafana に反映する。
# git pull で最新化した後にこのスクリプトを実行する想定。
#
# 事前準備(初回のみ、手動):
#   1. server/grafana/.env を作成し SLACK_WEBHOOK_URL を設定する(.env.example を参照)
#   2. sudo systemctl edit grafana-server.service で以下を追加:
#        [Service]
#        EnvironmentFile=-/etc/grafana/farmverse.env

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GRAFANA_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE="${GRAFANA_DIR}/.env"
PROVISIONING_SRC="${GRAFANA_DIR}/provisioning/alerting"
PROVISIONING_DST="/etc/grafana/provisioning/alerting"
FARMVERSE_ENV_DST="/etc/grafana/farmverse.env"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "エラー: ${ENV_FILE} が見つかりません。${GRAFANA_DIR}/.env.example を参考に作成してください。" >&2
  exit 1
fi

echo "==> ${FARMVERSE_ENV_DST} を更新 (root:grafana, 640)"
sudo install -m 640 -o root -g grafana "${ENV_FILE}" "${FARMVERSE_ENV_DST}"

echo "==> ${PROVISIONING_DST} に alerting 設定を配置"
sudo mkdir -p "${PROVISIONING_DST}"
sudo cp "${PROVISIONING_SRC}"/*.yaml "${PROVISIONING_DST}/"
sudo chown root:grafana "${PROVISIONING_DST}"/*.yaml

echo "==> grafana-server を再起動"
sudo systemctl restart grafana-server

sleep 2
if systemctl is-active --quiet grafana-server; then
  echo "==> grafana-server は正常に起動しています"
else
  echo "エラー: grafana-server の起動に失敗しました。journalctl -u grafana-server -n 50 を確認してください。" >&2
  exit 1
fi
