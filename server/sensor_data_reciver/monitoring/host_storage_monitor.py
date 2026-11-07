"""ホストのストレージ使用量を InfluxDB に書き込む (systemd timer から定期実行する)。

計測対象:
- ルートファイルシステムの total / used / free / used_percent
- 受信画像ディレクトリ (images/) のサイズとファイル数
- ~/.xsession-errors* のサイズ (デスクトップセッションのログ肥大化の検知用)

InfluxDB の接続情報は受信サービスと同じ .env (INFLUXDB_URL / INFLUXDB_TOKEN) を使う。
"""

import logging
import os
import shutil
import socket
import sys
from pathlib import Path

from dotenv import load_dotenv

logger = logging.getLogger("host_storage_monitor")

BASE_DIR = Path(__file__).resolve().parent.parent
MEASUREMENT = "host_storage"


def dir_size(path: Path) -> tuple[int, int]:
    """ディレクトリ配下の (合計バイト数, ファイル数) を返す。存在しなければ (0, 0)。"""
    total = 0
    count = 0
    if not path.is_dir():
        return 0, 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.lstat(os.path.join(root, name)).st_size
                count += 1
            except OSError:
                continue
    return total, count


def collect(mount: str, images_dir: Path, home: Path) -> dict[str, float]:
    """InfluxDB に書き込むフィールドを収集する。"""
    usage = shutil.disk_usage(mount)
    images_bytes, images_files = dir_size(images_dir)
    xsession_bytes = 0
    for path in home.glob(".xsession-errors*"):
        try:
            xsession_bytes += path.lstat().st_size
        except OSError:
            continue
    return {
        "total_bytes": usage.total,
        "used_bytes": usage.used,
        "free_bytes": usage.free,
        "used_percent": round(usage.used / usage.total * 100, 2),
        "images_bytes": images_bytes,
        "images_files": images_files,
        "xsession_errors_bytes": xsession_bytes,
    }


def write(fields: dict[str, float], mount: str) -> None:
    from influxdb_client import InfluxDBClient, Point
    from influxdb_client.client.write_api import SYNCHRONOUS

    org = os.environ.get("INFLUXDB_ORG", "agri")
    bucket = os.environ.get("INFLUXDB_BUCKET", "balcony")
    point = Point(MEASUREMENT).tag("host", socket.gethostname()).tag("mount", mount)
    for key, value in fields.items():
        point.field(key, float(value))

    with InfluxDBClient(
        url=os.environ.get("INFLUXDB_URL", "http://localhost:8086"),
        token=os.environ.get("INFLUXDB_TOKEN", ""),
        org=org,
        timeout=10_000,
    ) as client:
        client.write_api(write_options=SYNCHRONOUS).write(bucket=bucket, org=org, record=point)


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s")
    load_dotenv(BASE_DIR / ".env")

    mount = os.environ.get("STORAGE_MONITOR_MOUNT", "/")
    images_dir = Path(os.environ.get("STORAGE_MONITOR_IMAGES_DIR", BASE_DIR / "images"))
    fields = collect(mount, images_dir, Path.home())
    logger.info("Collected: %s", fields)

    try:
        write(fields, mount)
    except Exception as e:
        logger.error("Failed to write to InfluxDB: %s", e)
        return 1
    logger.info("Wrote %s to InfluxDB", MEASUREMENT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
