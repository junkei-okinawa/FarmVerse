"""InfluxDB client for sensor data storage."""

import asyncio
import logging
import os
import time
from threading import Lock

import influxdb_client
from influxdb_client import Point
from influxdb_client.client.write_api import SYNCHRONOUS

import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config

logger = logging.getLogger(__name__)


class InfluxDBClient:
    """InfluxDB クライアント管理クラス"""

    _INIT_RETRY_INTERVAL_SECONDS = 30.0
    
    def __init__(self):
        self.token = os.environ.get("INFLUXDB_TOKEN")
        self.client = None
        self.write_api = None
        self._active_tasks = set()  # アクティブタスクの追跡
        self._init_lock = Lock()
        self._last_init_failure_at = 0.0
        
        self._initialize_client(force=True)

    def _close_client_resources(self, client=None, write_api=None):
        """指定されたクライアント資源を安全に閉じる"""
        try:
            if write_api:
                write_api.close()
        except Exception as e:
            logger.debug(f"Error closing InfluxDB write API: {e}")

        try:
            if client:
                client.close()
        except Exception as e:
            logger.debug(f"Error closing InfluxDB client: {e}")

    def _initialize_client(self, force=False):
        """InfluxDBクライアントを初期化する"""
        with self._init_lock:
            if self.client and self.write_api and not force:
                return True

            new_client = None
            new_write_api = None

            try:
                new_client = influxdb_client.InfluxDBClient(
                    url=config.INFLUXDB_URL,
                    token=self.token,
                    org=config.INFLUXDB_ORG,
                )
                new_write_api = new_client.write_api(write_options=SYNCHRONOUS)

                health = new_client.health()
                if health.status == "pass":
                    self._close_client_resources(self.client, self.write_api)
                    self.client = new_client
                    self.write_api = new_write_api
                    self._last_init_failure_at = 0.0
                    logger.info(f"InfluxDB client initialized successfully: {config.INFLUXDB_URL}")
                    return True

                logger.warning(f"InfluxDB health check failed: {health.status}")
            except Exception as e:
                logger.warning(f"Failed to initialize InfluxDB client: {e} - will retry later")
            finally:
                if self.client is not new_client:
                    self._close_client_resources(new_client, new_write_api)

            self._disable_client_locked()
            return False

    def _should_retry_initialization(self):
        """再初期化を試すべきタイミングか判定する"""
        if self.client and self.write_api:
            return True
        if self._last_init_failure_at == 0.0:
            return True
        return (time.monotonic() - self._last_init_failure_at) >= self._INIT_RETRY_INTERVAL_SECONDS

    async def _ensure_client_ready_async(self, sender_mac: str = None):
        """必要ならクライアントを再初期化する"""
        if self.client and self.write_api:
            return True

        if not self._should_retry_initialization():
            logger.warning(
                f"InfluxDB client not initialized, retry suppressed for {sender_mac or 'unknown sender'}"
            )
            return False

        return await asyncio.to_thread(self._initialize_client, False)

    def _ensure_client_ready_sync(self, sender_mac: str = None):
        """同期的にクライアントを再初期化する"""
        if self.client and self.write_api:
            return True

        if not self._should_retry_initialization():
            logger.warning(
                f"InfluxDB client not initialized, retry suppressed for {sender_mac or 'unknown sender'}"
            )
            return False

        if self._initialize_client(force=False):
            return True

        self._last_init_failure_at = time.monotonic()
        return False

    def _disable_client_locked(self):
        """InfluxDBクライアントを無効化する（ロック保持前提）"""
        try:
            if self.write_api:
                self._close_client_resources(write_api=self.write_api)
            if self.client:
                self._close_client_resources(client=self.client)
        except Exception as e:
            logger.debug(f"Failed to fully close InfluxDB client resources while disabling client: {e}")
        self.client = None
        self.write_api = None
        self._last_init_failure_at = time.monotonic()

    def _disable_client(self):
        """InfluxDBクライアントを無効化"""
        with self._init_lock:
            self._disable_client_locked()
    
    def write_sensor_data(self, sender_mac: str, voltage: float = None, temperature: float = None, tds_voltage: float = None) -> bool:
        """センサーデータをInfluxDBに書き込み（非同期実行・エラー耐性付き）"""
        # テスト環境ではInfluxDB書き込みをスキップ
        if config.IS_TEST_ENV:
            logger.info(f"Test environment detected, skipping InfluxDB write for {sender_mac}")
            return False
            
        # イベントループが実行されているかチェック
        try:
            asyncio.get_running_loop()  # イベントループの存在確認のみ
        except RuntimeError:
            logger.warning(f"No running event loop for InfluxDB write for {sender_mac}, skipping")
            return False
            
        # InfluxDBへの書き込みを非同期で実行し、エラーが発生しても処理を継続する
        # asyncio.gatherを使用した構造化タスク管理
        write_task = self._write_sensor_data_async(sender_mac, voltage, temperature, tds_voltage)
        cleanup_task = self._cleanup_completed_tasks()
        
        # 両方のタスクを同時実行し、例外を適切に処理
        async def execute_tasks():
            return await asyncio.gather(write_task, cleanup_task, return_exceptions=True)
        
        try:
            main_task = asyncio.create_task(execute_tasks())
            
            # 作成したタスクをアクティブタスクとして追跡
            self._active_tasks.add(main_task)
            return True  # 非同期実行のため、即座にTrueを返す
        except Exception as e:
            logger.error(f"Error creating InfluxDB write task for {sender_mac}: {e}")
            return False
    
    async def _write_sensor_data_async(self, sender_mac: str, voltage: float = None, temperature: float = None, tds_voltage: float = None):
        """非同期でInfluxDBにデータを書き込み"""
        try:
            # 必要であればクライアントを再初期化する
            if not await self._ensure_client_ready_async(sender_mac):
                logger.warning(f"InfluxDB client not available for {sender_mac}")
                return

            # 以降の書き込みはこの呼び出し時点の write_api を使う
            write_api = self.write_api
            if not write_api:
                logger.warning(f"InfluxDB write API not available for {sender_mac}")
                return
            
            point = Point("data").tag("mac_address", sender_mac)
            
            if voltage is not None:
                point.field("voltage", float(voltage))
            
            if temperature is not None:
                point.field("temperature", float(temperature))
            
            if tds_voltage is not None:
                point.field("tds_voltage", float(tds_voltage))
            
            if voltage is not None or temperature is not None or tds_voltage is not None:
                logger.info(f"Writing data to InfluxDB for {sender_mac}: voltage={voltage}, temperature={temperature}, tds_voltage={tds_voltage}")
                # タイムアウトを設定して書き込み実行
                await asyncio.wait_for(
                    asyncio.to_thread(
                        write_api.write,
                        bucket=config.INFLUXDB_BUCKET, 
                        org=config.INFLUXDB_ORG, 
                        record=point
                ),
                    timeout=3.0  # 3秒でタイムアウト（10→3秒に短縮）
                )
                logger.info(f"Successfully wrote data to InfluxDB for {sender_mac}")
            else:
                logger.warning(f"No valid data to write for {sender_mac}")
                
        except asyncio.TimeoutError:
            logger.error(f"Timeout writing to InfluxDB for {sender_mac} (continuing with other operations)")
            with self._init_lock:
                self._last_init_failure_at = time.monotonic()
        except ConnectionError as e:
            logger.error(f"Connection error writing to InfluxDB for {sender_mac}: {e} (continuing with other operations)")
            self._disable_client()
        except Exception as e:
            logger.error(f"Unexpected error writing to InfluxDB for {sender_mac}: {e}")
            self._disable_client()
            
    async def _cleanup_completed_tasks(self):
        """バックグラウンドで完了したタスクをクリーンアップ"""
        try:
            # 完了したタスクを特定してセットから削除
            completed_tasks = {task for task in self._active_tasks if task.done()}
            for task in completed_tasks:
                self._active_tasks.discard(task)
                # 例外が発生したタスクをログに記録
                if task.exception():
                    logger.warning(f"Task completed with exception: {task.exception()}")
            
            if completed_tasks:
                logger.debug(f"Cleaned up {len(completed_tasks)} completed tasks")
            
            # 他のタスクに実行権を譲る
            await asyncio.sleep(0.01)
        except Exception as e:
            logger.error(f"Error during task cleanup: {e}")
        
        logger.debug("Background cleanup completed")
    
    async def close(self):
        """リソースのクリーンアップ - 全てのアクティブタスクを待機"""
        try:
            # 全てのアクティブタスクが完了するまで待機
            if self._active_tasks:
                logger.info(f"Waiting for {len(self._active_tasks)} active tasks to complete...")
                await asyncio.gather(*self._active_tasks, return_exceptions=True)
                self._active_tasks.clear()
                logger.info("All active tasks completed")
            
            # InfluxDBクライアントのクリーンアップ
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during InfluxDB client cleanup: {e}")

    def close_sync(self):
        """同期的なクリーンアップメソッド（後方互換性のため）"""
        try:
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during InfluxDB client cleanup: {e}")


# グローバルInfluxDBクライアントインスタンス
influx_client = InfluxDBClient()
